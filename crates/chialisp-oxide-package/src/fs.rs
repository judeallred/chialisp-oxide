//! Bounded local-file admission. This is not a hostile filesystem race sandbox.
use crate::{Error, ErrorKind, Result};
use std::{
    fs::{File, Metadata},
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

const MAX_PATH_BYTES: usize = 32 * 1024;
const MAX_PATH_COMPONENTS: usize = 1024;

pub(crate) fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty()
        || path.as_os_str().as_encoded_bytes().len() > MAX_PATH_BYTES
        || path.components().count() > MAX_PATH_COMPONENTS
    {
        return Err(Error::new(
            ErrorKind::InvalidPath,
            "empty or excessive local path",
        ));
    }
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .map_err(|error| Error::io("locate current directory", error))?
            .join(path))
    }
}

fn inspect(path: &Path) -> Result<Metadata> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| Error::io(format!("inspect {}", path.display()), error))?;
    if metadata.file_type().is_symlink() {
        return Err(Error::new(
            ErrorKind::InvalidPath,
            format!("symlink path component: {}", path.display()),
        ));
    }
    Ok(metadata)
}

/// Reject symlinks in every existing path component, including the final file.
///
/// Checks precede opening files. Callers assume directories are not concurrently
/// replaced by a hostile process; no openat-style filesystem sandbox is claimed.
pub(crate) fn check_no_symlink_components(path: &Path) -> Result<()> {
    let absolute = absolute_path(path)?;
    let mut current = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => continue,
            Component::ParentDir => {
                if !inspect(&current)?.is_dir() {
                    return Err(Error::new(
                        ErrorKind::InvalidPath,
                        "path parent is not a directory",
                    ));
                }
                current.pop();
            }
            _ => current.push(component.as_os_str()),
        }
        inspect(&current)?;
    }
    Ok(())
}

pub(crate) fn require_directory(path: &Path) -> Result<()> {
    check_no_symlink_components(path)?;
    if !inspect(path)?.is_dir() {
        return Err(Error::new(
            ErrorKind::InvalidPath,
            format!("expected local directory: {}", path.display()),
        ));
    }
    Ok(())
}

/// Create missing directories one component at a time, checking existing ones.
pub(crate) fn create_directories(path: &Path) -> Result<()> {
    let absolute = absolute_path(path)?;
    let mut current = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => continue,
            Component::ParentDir => {
                require_directory(&current)?;
                current.pop();
            }
            _ => current.push(component.as_os_str()),
        }
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(Error::new(
                        ErrorKind::InvalidPath,
                        format!(
                            "registry path is not a plain directory: {}",
                            current.display()
                        ),
                    ));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let mut builder = std::fs::DirBuilder::new();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                match builder.create(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => {
                        return Err(Error::io(format!("create {}", current.display()), error));
                    }
                }
                require_directory(&current)?;
                if let Some(parent) = current.parent() {
                    sync_directory(parent)?;
                }
            }
            Err(error) => return Err(Error::io(format!("inspect {}", current.display()), error)),
        }
    }
    Ok(())
}

fn same_file(left: &Metadata, right: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        left.dev() == right.dev() && left.ino() == right.ino()
    }
    #[cfg(not(unix))]
    {
        left.len() == right.len() && left.modified().ok() == right.modified().ok()
    }
}

/// Read one bounded regular file; never open a pre-existing FIFO/device/socket.
pub(crate) fn read_regular_file(path: &Path, max_bytes: usize) -> Result<Vec<u8>> {
    check_no_symlink_components(path)?;
    let before = inspect(path)?;
    if !before.is_file() {
        return Err(Error::new(
            ErrorKind::InvalidPath,
            "expected regular local file",
        ));
    }
    let bound = u64::try_from(max_bytes).unwrap_or(u64::MAX);
    if before.len() > bound {
        return Err(Error::new(
            ErrorKind::ResourceLimit,
            "local file exceeds byte limit",
        ));
    }
    let mut file =
        File::open(path).map_err(|error| Error::io(format!("open {}", path.display()), error))?;
    let opened = file
        .metadata()
        .map_err(|error| Error::io("inspect opened local file", error))?;
    if !opened.is_file() || !same_file(&before, &opened) {
        return Err(Error::new(
            ErrorKind::InvalidPath,
            "local file changed during open",
        ));
    }
    check_no_symlink_components(path)?;
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let remaining = max_bytes.saturating_sub(bytes.len());
        let read_limit = chunk.len().min(remaining.saturating_add(1));
        let count = file
            .read(&mut chunk[..read_limit])
            .map_err(|error| Error::io(format!("read {}", path.display()), error))?;
        if count == 0 {
            break;
        }
        if count > remaining {
            return Err(Error::new(
                ErrorKind::ResourceLimit,
                "local file grew beyond byte limit",
            ));
        }
        bytes.try_reserve(count).map_err(|_| {
            Error::new(
                ErrorKind::ResourceLimit,
                "cannot allocate bounded local file",
            )
        })?;
        bytes.extend_from_slice(&chunk[..count]);
    }
    let after = file
        .metadata()
        .map_err(|error| Error::io("inspect read local file", error))?;
    if before.len() != after.len()
        || bytes.len() as u64 != after.len()
        || before.modified().ok() != after.modified().ok()
    {
        return Err(Error::io(
            "local file changed during read",
            io::Error::other("unstable local input"),
        ));
    }
    Ok(bytes)
}

/// Directory entry durability is requested on Unix, where opening directories
/// for fsync is supported. Other hosts retain no-replace atomic visibility only.
pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    require_directory(path)?;
    #[cfg(unix)]
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| Error::io(format!("sync directory {}", path.display()), error))?;
    Ok(())
}

#[cfg(test)]
#[path = "fs_tests.rs"]
mod tests;
