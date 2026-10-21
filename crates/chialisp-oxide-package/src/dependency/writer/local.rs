use super::Phase;
use crate::{Error, ErrorKind, Result, fs};
use std::{
    ffi::OsString,
    fs::{File, Metadata, OpenOptions, TryLockError},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) struct Location {
    path: PathBuf,
    parent: PathBuf,
    sidecar: PathBuf,
}

pub(super) struct WriterGuard {
    file: File,
    locked: bool,
}
impl WriterGuard {
    pub(super) fn release(mut self) -> Result<()> {
        self.file
            .unlock()
            .map_err(|error| Error::io("release dependency writer lock", error))?;
        self.locked = false;
        Ok(())
    }
}
impl Drop for WriterGuard {
    fn drop(&mut self) {
        if self.locked {
            // Closing one handle need not release a lock while a duplicated or
            // fork-inherited handle survives. End this guard's lock explicitly
            // before closing; error/unwind cleanup remains best effort.
            // https://doc.rust-lang.org/std/fs/struct.File.html#method.lock
            let _ = self.file.unlock();
        }
    }
}
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidPath, message)
}
fn no_alias(metadata: &Metadata) -> Result<()> {
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(invalid("dependency lock paths must be plain regular files"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(invalid("dependency lock paths cannot be hardlink aliases"));
        }
    }
    Ok(())
}
fn same_file(a: &Metadata, b: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        a.dev() == b.dev() && a.ino() == b.ino()
    }
    #[cfg(not(unix))]
    {
        a.len() == b.len() && a.modified().ok() == b.modified().ok()
    }
}
fn check_sidecar(metadata: &Metadata) -> Result<()> {
    no_alias(metadata)?;
    if metadata.len() != 0 {
        return Err(invalid("dependency writer sidecar must remain empty"));
    }
    Ok(())
}
fn outside_registry(parent: &Path, registry: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        let registry_metadata = std::fs::metadata(registry)
            .map_err(|e| Error::io("inspect dependency registry identity", e))?;
        for ancestor in parent.ancestors() {
            let metadata = std::fs::metadata(ancestor)
                .map_err(|e| Error::io("inspect dependency parent identity", e))?;
            if same_file(&registry_metadata, &metadata) {
                return Err(invalid(
                    "dependency lock cannot be inside the selected registry",
                ));
            }
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (parent, registry);
        Err(Error::io(
            "dependency lock writes require reviewed Unix filesystem identities",
            io::Error::new(
                io::ErrorKind::Unsupported,
                "unsupported dependency writer platform",
            ),
        ))
    }
}
impl Location {
    pub(super) fn new(registry: &Path, path: &Path) -> Result<Self> {
        let path = fs::absolute_path(path)?;
        let filename = path
            .file_name()
            .ok_or_else(|| invalid("dependency lock requires a file basename"))?;
        // Compare reserved Unicode case variants without changing OS path
        // bytes. In particular, Kelvin-sign K aliases ASCII K on APFS.
        if filename.to_str().is_some_and(|name| {
            ["cargo.toml", "cargo.lock"].iter().any(|reserved| {
                name.chars()
                    .flat_map(char::to_lowercase)
                    .eq(reserved.chars())
            })
        }) {
            return Err(invalid(
                "Cargo.toml and Cargo.lock are reserved dependency lock destinations",
            ));
        }
        let parent = path
            .parent()
            .ok_or_else(|| invalid("dependency lock has no parent"))?;
        fs::require_directory(parent)?;
        // Validate lexical components before resolving the existing parent, so
        // canonicalization never erases a forbidden symlink from admission.
        let parent = parent
            .canonicalize()
            .map_err(|e| Error::io("resolve dependency lock parent", e))?;
        outside_registry(&parent, registry)?;
        let path = parent.join(filename);
        let mut sidecar = OsString::from(".");
        sidecar.push(filename);
        sidecar.push(".write-lock");
        Ok(Self {
            sidecar: parent.join(sidecar),
            parent,
            path,
        })
    }
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
    pub(super) fn snapshot(&self) -> Result<Option<Vec<u8>>> {
        fs::require_directory(&self.parent)?;
        match std::fs::symlink_metadata(&self.path) {
            Ok(metadata) => {
                no_alias(&metadata)?;
                Ok(Some(fs::read_regular_file(
                    &self.path,
                    super::super::MAX_LOCK_BYTES,
                )?))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Error::io("inspect dependency lock", error)),
        }
    }
    pub(super) fn compare_snapshot(&self, old: &Option<Vec<u8>>) -> Result<()> {
        if &self.snapshot()? != old {
            return Err(Error::new(
                ErrorKind::Conflict,
                "dependency lock changed since initial snapshot",
            ));
        }
        Ok(())
    }
    pub(super) fn writer_lock(&self) -> Result<WriterGuard> {
        fs::require_directory(&self.parent)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = match options.open(&self.sidecar) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                fs::check_no_symlink_components(&self.sidecar)?;
                let before = std::fs::symlink_metadata(&self.sidecar)
                    .map_err(|e| Error::io("inspect dependency writer sidecar", e))?;
                check_sidecar(&before)?;
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&self.sidecar)
                    .map_err(|e| Error::io("open dependency writer sidecar", e))?;
                let opened = file
                    .metadata()
                    .map_err(|e| Error::io("inspect opened dependency sidecar", e))?;
                if !same_file(&before, &opened) {
                    return Err(invalid("dependency writer sidecar changed during open"));
                }
                file
            }
            Err(error) => return Err(Error::io("create dependency writer sidecar", error)),
        };
        check_sidecar(
            &file
                .metadata()
                .map_err(|e| Error::io("inspect dependency writer sidecar", e))?,
        )?;
        file.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => {
                Error::new(ErrorKind::Conflict, "dependency lock has an active writer")
            }
            TryLockError::Error(error) => Error::io("try dependency writer lock", error),
        })?;
        // Construct immediately after acquisition, so even a subsequent path
        // validation failure explicitly unlocks this description before close.
        let guard = WriterGuard { file, locked: true };
        fs::check_no_symlink_components(&self.sidecar)?;
        Ok(guard)
    }
    pub(super) fn stage(
        &self,
        bytes: &[u8],
        hook: &mut impl FnMut(Phase, &Path) -> Result<()>,
    ) -> Result<Stage> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        fs::require_directory(&self.parent)?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::new(ErrorKind::Io, "local clock precedes Unix epoch"))?
            .as_nanos();
        let mut name = OsString::from(".");
        name.push(self.path.file_name().expect("checked lock basename"));
        name.push(format!(
            ".stage-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let path = self.parent.join(name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&path)
            .map_err(|e| Error::io("create dependency lock stage", e))?;
        let stage = Stage { path };
        hook(Phase::BeforeStageWrite, &stage.path)?;
        file.write_all(bytes)
            .map_err(|e| Error::io("write dependency lock stage", e))?;
        hook(Phase::BeforeStageSync, &stage.path)?;
        file.sync_all()
            .map_err(|e| Error::io("sync dependency lock stage", e))?;
        drop(file);
        Ok(stage)
    }
}

pub(super) struct Stage {
    path: PathBuf,
}
impl Stage {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
    pub(super) fn commit(self, destination: &Path, existed: bool) -> Result<()> {
        #[cfg(not(unix))]
        if existed {
            return Err(Error::io(
                "atomic dependency replacement unsupported on this platform",
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "POSIX local replacement required",
                ),
            ));
        }
        if existed {
            std::fs::rename(&self.path, destination)
                .map_err(|e| Error::io("commit dependency lock replacement", e))?;
        } else {
            match std::fs::hard_link(&self.path, destination) {
                Ok(()) => (),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    return Err(Error::new(
                        ErrorKind::Conflict,
                        "dependency destination appeared before commit",
                    ));
                }
                Err(e) => {
                    return Err(Error::io(
                        "commit new dependency lock without replacement",
                        e,
                    ));
                }
            }
            std::fs::remove_file(&self.path)
                .map_err(|e| Error::io("remove committed dependency staging link", e))?;
        }
        fs::sync_directory(destination.parent().expect("checked lock parent"))
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn writer_guard_unlocks_with_a_surviving_duplicate_on_drop_error_and_release() {
        for action in ["drop", "error", "release"] {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().canonicalize().unwrap();
            let registry = root.join("registry");
            std::fs::create_dir(&registry).unwrap();
            let location = Location::new(&registry, &root.join("consumer.lock")).unwrap();
            let guard = location.writer_lock().unwrap();
            let duplicate = guard.file.try_clone().unwrap();
            let contender = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&location.sidecar)
                .unwrap();
            assert!(matches!(
                contender.try_lock(),
                Err(TryLockError::WouldBlock)
            ));
            match action {
                "drop" => drop(guard),
                "release" => guard.release().unwrap(),
                "error" => {
                    let result: Result<()> = {
                        let _guard = guard;
                        Err(Error::io(
                            "constructed body failure",
                            io::Error::other("fail after acquiring guard"),
                        ))
                    };
                    assert_eq!(result.unwrap_err().kind(), ErrorKind::Io);
                }
                _ => unreachable!(),
            }
            // The duplicate deliberately remains open. A close-only guard
            // retains its flock here; explicit unlock permits immediate use.
            contender.try_lock().unwrap();
            contender.unlock().unwrap();
            drop(duplicate);
        }
    }
}
