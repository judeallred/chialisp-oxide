//! Bounded cooperating-writer storage. No hostile-directory or power-loss
//! guarantee. A post-rename durability error must never be retried as though
//! the original outbox were still present.
use super::LiveOutbox;
use crate::{Digest, digest};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{File, Metadata, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) const MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug)]
pub enum CommitError {
    BeforeCommit(anyhow::Error),
    CommittedDurabilityUnknown(anyhow::Error),
}

impl std::fmt::Display for CommitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BeforeCommit(error) => write!(f, "outbox was not committed: {error}"),
            Self::CommittedDurabilityUnknown(error) => {
                write!(f, "outbox was committed; durability is unknown: {error}")
            }
        }
    }
}
impl std::error::Error for CommitError {}

pub struct StoredOutbox {
    pub outbox: LiveOutbox,
    pub byte_digest: Digest,
}

#[derive(Clone)]
pub struct LiveStore {
    path: PathBuf,
}

fn path_components(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        ensure!(
            matches!(component, Component::RootDir | Component::Normal(_)),
            "outbox path must be normalized"
        );
        current.push(component.as_os_str());
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink(),
                "symlink outbox path component"
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
        "outbox file must be regular with one link"
    );
    Ok(())
}
#[cfg(not(unix))]
fn regular(_: &Metadata) -> Result<()> {
    anyhow::bail!("outbox writes require the reviewed Unix platform")
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
    let mut checked = options();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        checked.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = checked.read(true).write(write).open(path)?;
    let opened = file.metadata()?;
    regular(&opened)?;
    ensure!(
        same_file(&before, &opened),
        "outbox file changed while opening"
    );
    Ok(file)
}

impl LiveStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        ensure!(
            cfg!(unix),
            "outbox writes require the reviewed Unix platform"
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
            .context("outbox basename must be UTF-8")?;
        ensure!(
            name.to_ascii_lowercase().ends_with(".live.json"),
            "outbox requires a dedicated .live.json basename"
        );
        ensure!(
            path.parent().context("outbox parent missing")?.is_dir(),
            "outbox parent directory must already exist"
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
            "outbox file byte bound"
        );
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= MAX_BYTES, "outbox grew beyond byte bound");
        Ok(Some(bytes))
    }

    pub fn load(&self) -> Result<StoredOutbox> {
        let bytes = self.bytes()?.context("outbox does not exist")?;
        Ok(StoredOutbox {
            outbox: decode(&bytes)?,
            byte_digest: digest(&bytes),
        })
    }

    pub fn create(&self, outbox: &LiveOutbox) -> std::result::Result<Digest, CommitError> {
        self.commit(None, outbox)
    }

    pub fn commit(
        &self,
        expected: Option<Digest>,
        outbox: &LiveOutbox,
    ) -> std::result::Result<Digest, CommitError> {
        self.commit_with(expected, outbox, |_| Ok(()))
    }

    fn commit_with(
        &self,
        expected: Option<Digest>,
        outbox: &LiveOutbox,
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
            return Err(before(anyhow::anyhow!("outbox sidecar must be empty")));
        }
        lock.try_lock()
            .map_err(|error| before(anyhow::anyhow!("outbox writer lock unavailable: {error}")))?;
        let original = self.bytes().map_err(before)?;
        if original.as_deref().map(digest) != expected {
            return Err(before(anyhow::anyhow!(
                "outbox byte identity changed or create target already exists"
            )));
        }
        if let Some(bytes) = &original {
            let previous = decode(bytes).map_err(before)?;
            if !outbox.follows(&previous) {
                return Err(before(anyhow::anyhow!(
                    "outbox is unrelated or rewrites retained history"
                )));
            }
        }
        outbox.validate_shape().map_err(before)?;
        let bytes = serde_json::to_vec(outbox).map_err(|error| before(error.into()))?;
        if bytes.len() > MAX_BYTES {
            return Err(before(anyhow::anyhow!("live outbox byte bound")));
        }
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
            return Err(before(anyhow::anyhow!("outbox changed before commit")));
        }
        std::fs::rename(&temporary, &self.path).map_err(|error| before(error.into()))?;
        hook(true).map_err(CommitError::CommittedDurabilityUnknown)?;
        File::open(self.path.parent().expect("validated parent"))
            .and_then(|directory| directory.sync_all())
            .map_err(|error| CommitError::CommittedDurabilityUnknown(error.into()))?;
        Ok(result)
    }
}

fn decode(bytes: &[u8]) -> Result<LiveOutbox> {
    ensure!(bytes.len() <= MAX_BYTES, "live outbox byte bound");
    // Every nested object is a deny_unknown_fields typed struct/enum. Serde
    // rejects duplicate known fields; there are no free-form JSON maps.
    let outbox: LiveOutbox = serde_json::from_slice(bytes)?;
    outbox.validate_shape()?;
    Ok(outbox)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn empty() -> LiveOutbox {
        LiveOutbox {
            protocol: super::super::PROTOCOL.into(),
            binding: super::super::LiveBinding {
                network: crate::provider::testnet11_network(),
                wallet_puzzle_hash: [0x21; 32],
                master_public_key_hex: "11".repeat(48),
                artifact_digest: [0x22; 32],
                native_check_digest: [0x23; 32],
                clvm_check_digest: [0x24; 32],
                recipient: [0x21; 32],
                fee: 10_000,
                payment_fee: 10_000,
            },
            events: vec![],
        }
    }
    fn store() -> (tempfile::TempDir, LiveStore) {
        let directory = tempfile::tempdir().unwrap();
        let store = LiveStore::new(
            directory
                .path()
                .canonicalize()
                .unwrap()
                .join("test.live.json"),
        )
        .unwrap();
        (directory, store)
    }
    #[test]
    fn cas_and_bound_plan_preserve_existing_outbox() {
        let (_directory, store) = store();
        let outbox = empty();
        let pin = store.create(&outbox).unwrap();
        let bytes = std::fs::read(store.path()).unwrap();
        assert!(matches!(
            store.create(&outbox),
            Err(CommitError::BeforeCommit(_))
        ));
        assert!(store.commit(Some([0; 32]), &outbox).is_err());
        let mut unrelated = outbox.clone();
        unrelated.binding.fee += 1;
        assert!(store.commit(Some(pin), &unrelated).is_err());
        assert_eq!(std::fs::read(store.path()).unwrap(), bytes);
        assert_eq!(store.load().unwrap().byte_digest, pin);
    }
    #[test]
    fn duplicate_unknown_fields_and_stages_are_rejected() {
        let raw = String::from_utf8(serde_json::to_vec(&empty()).unwrap()).unwrap();
        assert!(
            decode(
                raw.replace("\"protocol\":", "\"extra\":0,\"protocol\":")
                    .as_bytes()
            )
            .is_err()
        );
        assert!(
            decode(
                raw.replace("\"fee\":10000", "\"fee\":10000,\"fee\":10000")
                    .as_bytes()
            )
            .is_err()
        );
        assert!(
            decode(
                raw.replace(
                    "\"events\":[]",
                    "\"events\":[{\"event\":\"unknown_stage\"}]"
                )
                .as_bytes()
            )
            .is_err()
        );
        assert!(decode(&vec![b' '; MAX_BYTES + 1]).is_err());
    }
    #[test]
    fn write_failure_and_postcommit_durability_are_distinct() {
        let (_directory, store) = store();
        let outbox = empty();
        assert!(matches!(
            store.commit_with(None, &outbox, |after| {
                if !after {
                    anyhow::bail!("injected before commit");
                }
                Ok(())
            }),
            Err(CommitError::BeforeCommit(_))
        ));
        assert!(!store.path().exists());
        assert!(matches!(
            store.commit_with(None, &outbox, |after| {
                if after {
                    anyhow::bail!("injected after commit");
                }
                Ok(())
            }),
            Err(CommitError::CommittedDurabilityUnknown(_))
        ));
        assert_eq!(store.load().unwrap().outbox, outbox);
    }
    #[cfg(unix)]
    #[test]
    fn symlinks_hardlinks_fifos_and_live_writer_lock_are_refused() {
        use std::os::unix::ffi::OsStrExt;
        let (directory, store) = store();
        let root = directory.path().canonicalize().unwrap();
        assert!(LiveStore::new(root.join("Cargo.lock")).is_err());
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&root, &alias).unwrap();
        assert!(LiveStore::new(alias.join("new.live.json")).is_err());
        let pin = store.create(&empty()).unwrap();
        let hard = root.join("hard.live.json");
        std::fs::hard_link(store.path(), &hard).unwrap();
        assert!(store.load().is_err());
        std::fs::remove_file(&hard).unwrap();
        let sidecar = root.join("test.live.json.lock");
        let lock = File::open(&sidecar).unwrap();
        lock.try_lock().unwrap();
        assert!(store.commit(Some(pin), &empty()).is_err());
        drop(lock);
        let fifo = root.join("pipe.live.json");
        let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let started = std::time::Instant::now();
        assert!(LiveStore::new(&fifo).unwrap().load().is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }
}
