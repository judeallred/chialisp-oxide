//! Bounded cooperating-writer storage. No hostile-directory or power-loss
//! guarantee. A post-rename durability error must never be retried as though
//! the original journal were still present.
use crate::{Digest, Journal, digest};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{File, Metadata, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug)]
pub enum CommitError {
    BeforeCommit(anyhow::Error),
    CommittedDurabilityUnknown(anyhow::Error),
}

impl std::fmt::Display for CommitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BeforeCommit(error) => write!(f, "journal was not committed: {error}"),
            Self::CommittedDurabilityUnknown(error) => {
                write!(f, "journal was committed; durability is unknown: {error}")
            }
        }
    }
}
impl std::error::Error for CommitError {}

pub struct StoredJournal {
    pub journal: Journal,
    pub byte_digest: Digest,
}

pub struct JournalStore {
    path: PathBuf,
}

fn path_components(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        ensure!(
            matches!(component, Component::RootDir | Component::Normal(_)),
            "journal path must be normalized"
        );
        current.push(component.as_os_str());
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink(),
                "symlink journal path component"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn regular(metadata: &Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    ensure!(
        metadata.is_file() && metadata.nlink() == 1,
        "journal file must be regular with one link"
    );
    Ok(())
}
#[cfg(not(unix))]
fn regular(_: &Metadata) -> Result<()> {
    anyhow::bail!("journal writes require the reviewed Unix platform")
}

#[cfg(unix)]
fn same_file(first: &Metadata, second: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    first.dev() == second.dev() && first.ino() == second.ino()
}
#[cfg(not(unix))]
fn same_file(_: &Metadata, _: &Metadata) -> bool {
    false
}

fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

fn open_checked(path: &Path, write: bool) -> Result<File> {
    path_components(path)?;
    let before = std::fs::symlink_metadata(path)?;
    regular(&before)?;
    let file = OpenOptions::new().read(true).write(write).open(path)?;
    let opened = file.metadata()?;
    regular(&opened)?;
    ensure!(
        same_file(&before, &opened),
        "journal file changed while opening"
    );
    Ok(file)
}

impl JournalStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        ensure!(
            cfg!(unix),
            "journal writes require the reviewed Unix platform"
        );
        let path = if path.as_ref().is_absolute() {
            path.as_ref().to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        path_components(&path)?;
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .context("journal basename must be UTF-8")?;
        ensure!(
            name.to_ascii_lowercase().ends_with(".lifecycle.json"),
            "journal requires a dedicated .lifecycle.json basename"
        );
        ensure!(
            path.parent().context("journal parent missing")?.is_dir(),
            "journal parent directory must already exist"
        );
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn bytes(&self) -> Result<Option<Vec<u8>>> {
        let mut file = match open_checked(&self.path, false) {
            Ok(file) => file,
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        ensure!(
            file.metadata()?.len() <= MAX_BYTES as u64,
            "journal file byte bound"
        );
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= MAX_BYTES, "journal grew beyond byte bound");
        Ok(Some(bytes))
    }

    pub fn load(&self) -> Result<StoredJournal> {
        let bytes = self.bytes()?.context("journal does not exist")?;
        Ok(StoredJournal {
            journal: Journal::from_json(&bytes)?,
            byte_digest: digest(&bytes),
        })
    }

    pub fn create(&self, journal: &Journal) -> std::result::Result<Digest, CommitError> {
        self.commit(None, journal)
    }

    pub fn commit(
        &self,
        expected: Option<Digest>,
        journal: &Journal,
    ) -> std::result::Result<Digest, CommitError> {
        self.commit_with(expected, journal, |_| Ok(()))
    }

    fn commit_with(
        &self,
        expected: Option<Digest>,
        journal: &Journal,
        mut hook: impl FnMut(bool) -> Result<()>,
    ) -> std::result::Result<Digest, CommitError> {
        let before = |error| CommitError::BeforeCommit(error);
        path_components(&self.path).map_err(before)?;
        let sidecar = self.path.with_file_name(format!(
            "{}.lock",
            self.path
                .file_name()
                .expect("validated filename")
                .to_string_lossy()
        ));
        path_components(&sidecar).map_err(before)?;
        let lock = match options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&sidecar)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                open_checked(&sidecar, true).map_err(before)?
            }
            Err(error) => return Err(before(error.into())),
        };
        regular(&lock.metadata().map_err(|error| before(error.into()))?).map_err(before)?;
        if lock.metadata().map_err(|error| before(error.into()))?.len() != 0 {
            return Err(before(anyhow::anyhow!("journal sidecar must be empty")));
        }
        lock.try_lock()
            .map_err(|error| before(anyhow::anyhow!("journal writer lock unavailable: {error}")))?;
        let original = self.bytes().map_err(before)?;
        if original.as_deref().map(digest) != expected {
            return Err(before(anyhow::anyhow!(
                "journal byte identity changed or create target already exists"
            )));
        }
        if let Some(bytes) = &original {
            let previous = Journal::from_json(bytes).map_err(before)?;
            if previous.plan() != journal.plan()
                || previous.entries().len() > journal.entries().len()
                || &journal.entries()[..previous.entries().len()] != previous.entries()
            {
                return Err(before(anyhow::anyhow!(
                    "journal is unrelated or rewrites retained history"
                )));
            }
        }
        let bytes = journal.to_json().map_err(before)?;
        let result = digest(&bytes);
        if original.as_deref() == Some(bytes.as_slice()) {
            return Ok(result);
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary = self.path.with_file_name(format!(
            ".{}.stage-{}-{}",
            self.path
                .file_name()
                .expect("validated filename")
                .to_string_lossy(),
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut staged = options()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| before(error.into()))?;
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(temporary.clone());
        staged
            .write_all(&bytes)
            .map_err(|error| before(error.into()))?;
        staged.sync_all().map_err(|error| before(error.into()))?;
        hook(false).map_err(before)?;
        path_components(&self.path).map_err(before)?;
        if self.bytes().map_err(before)? != original {
            return Err(before(anyhow::anyhow!("journal changed before commit")));
        }
        std::fs::rename(&temporary, &self.path).map_err(|error| before(error.into()))?;
        hook(true).map_err(CommitError::CommittedDurabilityUnknown)?;
        File::open(self.path.parent().expect("validated parent"))
            .and_then(|directory| directory.sync_all())
            .map_err(|error| CommitError::CommittedDurabilityUnknown(error.into()))?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, JournalStore, Journal) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp
            .path()
            .canonicalize()
            .unwrap()
            .join("test.lifecycle.json");
        let store = JournalStore::new(path).unwrap();
        let journal = crate::runner::SimulatorRunner::new()
            .unwrap()
            .into_journal();
        (temp, store, journal)
    }

    #[test]
    fn stale_create_and_updates_preserve_original_bytes() {
        let (_temp, store, journal) = setup();
        let pin = store.create(&journal).unwrap();
        let bytes = std::fs::read(store.path()).unwrap();
        assert!(matches!(
            store.create(&journal),
            Err(CommitError::BeforeCommit(_))
        ));
        assert!(matches!(
            store.commit(Some([0; 32]), &journal),
            Err(CommitError::BeforeCommit(_))
        ));
        assert_eq!(std::fs::read(store.path()).unwrap(), bytes);
        assert_eq!(store.load().unwrap().byte_digest, pin);
    }

    #[test]
    fn failures_before_and_after_commit_are_distinct() {
        let (_temp, store, journal) = setup();
        assert!(matches!(
            store.commit_with(None, &journal, |after| {
                if !after {
                    anyhow::bail!("injected precommit failure")
                }
                Ok(())
            }),
            Err(CommitError::BeforeCommit(_))
        ));
        assert!(!store.path().exists());
        assert!(matches!(
            store.commit_with(None, &journal, |after| {
                if after {
                    anyhow::bail!("injected directory durability failure")
                }
                Ok(())
            }),
            Err(CommitError::CommittedDurabilityUnknown(_))
        ));
        assert_eq!(
            store.load().unwrap().journal.to_json().unwrap(),
            journal.to_json().unwrap()
        );
    }

    #[test]
    fn committed_history_and_plan_cannot_be_replaced() {
        let (_temp, store, _) = setup();
        let mut runner = crate::runner::SimulatorRunner::new().unwrap();
        runner.execute(crate::runner::Command::Advance).unwrap();
        runner.execute(crate::runner::Command::Advance).unwrap();
        let pin = store.create(runner.journal()).unwrap();
        let bytes = runner.journal().to_json().unwrap();
        let mut changed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        changed["entries"][1]["event"]["record"]["native_receipt"] = serde_json::json!([1]);
        let rewritten = Journal::from_json(&serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(store.commit(Some(pin), &rewritten).is_err());
        let mut plan = runner.journal().plan().clone();
        plan.outputs[0].puzzle_hash = [0x43; 32];
        assert!(
            store
                .commit(Some(pin), &Journal::new(plan).unwrap())
                .is_err()
        );
        assert_eq!(std::fs::read(store.path()).unwrap(), bytes);
    }

    #[test]
    fn dedicated_basename_does_not_target_cargo_files() {
        let directory = tempfile::tempdir().unwrap();
        assert!(
            JournalStore::new(directory.path().canonicalize().unwrap().join("Cargo.lock")).is_err()
        );
        assert!(
            JournalStore::new(directory.path().canonicalize().unwrap().join("Cargo.toml")).is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn aliases_unrelated_content_and_busy_lock_are_refused() {
        use std::os::unix::fs::symlink;
        let (temp, store, journal) = setup();
        let other = temp.path().join("unrelated");
        std::fs::write(&other, b"preserve me").unwrap();
        symlink(&other, store.path()).unwrap();
        assert!(store.create(&journal).is_err());
        std::fs::remove_file(store.path()).unwrap();
        std::fs::hard_link(&other, store.path()).unwrap();
        assert!(store.create(&journal).is_err());
        std::fs::remove_file(store.path()).unwrap();
        std::fs::write(store.path(), b"preserve me").unwrap();
        assert!(
            store
                .commit(Some(digest(b"preserve me")), &journal)
                .is_err()
        );
        assert_eq!(std::fs::read(store.path()).unwrap(), b"preserve me");
        std::fs::remove_file(store.path()).unwrap();
        let sidecar = store.path().with_file_name("test.lifecycle.json.lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&sidecar)
            .unwrap();
        lock.try_lock().unwrap();
        assert!(store.create(&journal).is_err());
        assert!(!store.path().exists());
    }
}
