//! Immutable exact-envelope storage on a trusted local filesystem.
//!
//! A hard link commits a complete synced staging file without replacing an
//! existing object. This guarantees no-replace visibility on supported local
//! filesystems, not protection against hostile concurrent directory replacement.
//! An I/O error after commit may leave the complete object visible; retrying
//! verifies that object before reporting AlreadyPresent. Unix directory fsync
//! requests entry durability; other hosts promise visibility only.
use crate::{
    Digest32, Error, ErrorKind, Limits, Result, VerifiedPackage, digest, fs, verify_package,
};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublishStatus {
    Published,
    AlreadyPresent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct PublishOutcome {
    pub object_id: Digest32,
    pub status: PublishStatus,
}

#[derive(Debug)]
pub struct Registry {
    root: PathBuf,
    limits: Limits,
}

impl Registry {
    /// Open an existing store without creating directories or a lock file.
    pub fn open(path: &Path, limits: Limits) -> Result<Self> {
        fs::require_directory(path)?;
        let root = path
            .canonicalize()
            .map_err(|error| Error::io("resolve local registry directory", error))?;
        let registry = Self { root, limits };
        registry.check_layout()?;
        Ok(registry)
    }

    /// Explicitly create a local store; no network or compiler is involved.
    pub fn create(path: &Path, limits: Limits) -> Result<Self> {
        fs::create_directories(path)?;
        fs::create_directories(&path.join("objects"))?;
        fs::create_directories(&path.join(".staging"))?;
        Self::open(path, limits)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    fn objects(&self) -> PathBuf {
        self.root.join("objects")
    }

    fn object_path(&self, object_id: Digest32) -> PathBuf {
        self.objects().join(format!("{object_id}.json"))
    }

    fn check_layout(&self) -> Result<()> {
        fs::require_directory(&self.root)?;
        fs::require_directory(&self.objects())?;
        match std::fs::symlink_metadata(self.root.join(".staging")) {
            Ok(_) => fs::require_directory(&self.root.join(".staging"))?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::io("inspect registry staging directory", error)),
        }
        Ok(())
    }

    fn object_paths(&self) -> Result<Vec<(Digest32, PathBuf)>> {
        self.check_layout()?;
        let entries = std::fs::read_dir(self.objects())
            .map_err(|error| Error::io("list registry objects", error))?;
        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| Error::io("read registry object entry", error))?;
            if paths.len() >= self.limits.max_objects {
                return Err(Error::new(
                    ErrorKind::ResourceLimit,
                    "registry object count exceeds limit",
                ));
            }
            let name = entry.file_name();
            let raw = name
                .to_str()
                .and_then(|name| name.strip_suffix(".json"))
                .ok_or_else(|| {
                    Error::new(ErrorKind::CorruptObject, "invalid registry object filename")
                })?;
            let identity = raw.parse::<Digest32>().map_err(|_| {
                Error::new(ErrorKind::CorruptObject, "invalid registry object identity")
            })?;
            let file_type = entry
                .file_type()
                .map_err(|error| Error::io("inspect registry object entry", error))?;
            if !file_type.is_file() || file_type.is_symlink() {
                return Err(Error::new(
                    ErrorKind::InvalidPath,
                    "registry object is not a plain regular file",
                ));
            }
            paths.push((identity, entry.path()));
        }
        paths.sort_unstable_by_key(|(identity, _)| *identity);
        Ok(paths)
    }

    fn admit_object(&self, identity: Digest32, bytes: &[u8]) -> Result<VerifiedPackage> {
        if digest(bytes) != identity {
            return Err(Error::new(
                ErrorKind::CorruptObject,
                "registry filename differs from exact object bytes",
            ));
        }
        verify_package(bytes, &self.limits).map_err(|error| {
            if error.kind() == ErrorKind::ResourceLimit {
                error
            } else {
                Error::new(
                    ErrorKind::CorruptObject,
                    format!("invalid retained registry object: {error}"),
                )
            }
        })
    }

    /// Verify one exact object identity. Missing objects are distinct from I/O
    /// failures, malformed objects and locally selected resource limits.
    pub fn get(&self, object_id: Digest32) -> Result<VerifiedPackage> {
        self.check_layout()?;
        let path = self.object_path(object_id);
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(Error::new(
                    ErrorKind::UnknownObject,
                    format!("unknown registry object {object_id}"),
                ));
            }
            Err(error) => return Err(Error::io("inspect requested registry object", error)),
            Ok(_) => {}
        }
        let bytes = fs::read_regular_file(&path, self.limits.max_envelope_bytes)?;
        self.admit_object(object_id, &bytes)
    }

    /// Return every verified exact-envelope variant for a module tree hash.
    /// All objects are checked under bounded aggregate reads; a corrupt object
    /// cannot be hidden by claiming that its module hash does not match.
    pub fn lookup(&self, module_hash: Digest32) -> Result<Vec<VerifiedPackage>> {
        self.scan_for_dependencies(Some(module_hash), &BTreeSet::new())
    }

    /// Admit the complete store once, retaining the union of exact requested
    /// objects and module-hash matches. Missing requested IDs remain the
    /// dependency caller's responsibility. The fixed request bound is separate
    /// from the store's object count and aggregate byte limits.
    pub(crate) fn scan_for_dependencies(
        &self,
        module_hash: Option<Digest32>,
        object_ids: &BTreeSet<Digest32>,
    ) -> Result<Vec<VerifiedPackage>> {
        if object_ids.len() > 128 {
            return Err(Error::new(
                ErrorKind::ResourceLimit,
                "dependency object request count exceeds limit",
            ));
        }
        let mut remaining = self.limits.max_lookup_bytes;
        let mut matches = Vec::new();
        for (identity, path) in self.object_paths()? {
            let bytes =
                fs::read_regular_file(&path, remaining.min(self.limits.max_envelope_bytes))?;
            remaining -= bytes.len();
            let package = self.admit_object(identity, &bytes)?;
            if object_ids.contains(&identity)
                || module_hash.is_some_and(|hash| package.module_tree_hash() == hash)
            {
                matches.push(package);
            }
        }
        Ok(matches)
    }

    fn publication_lock(&self) -> Result<File> {
        self.check_layout()?;
        let path = self.root.join(".publish.lock");
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = match options.open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                fs::check_no_symlink_components(&path)?;
                if !std::fs::symlink_metadata(&path)
                    .map_err(|error| Error::io("inspect registry publication lock", error))?
                    .is_file()
                {
                    return Err(Error::new(
                        ErrorKind::InvalidPath,
                        "registry lock is not a regular file",
                    ));
                }
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                    .map_err(|error| Error::io("open registry publication lock", error))?
            }
            Err(error) => return Err(Error::io("create registry publication lock", error)),
        };
        if !file
            .metadata()
            .map_err(|error| Error::io("inspect opened registry lock", error))?
            .is_file()
        {
            return Err(Error::new(
                ErrorKind::InvalidPath,
                "opened registry lock is not a regular file",
            ));
        }
        file.lock()
            .map_err(|error| Error::io("lock registry publication", error))?;
        fs::check_no_symlink_components(&path)?;
        Ok(file)
    }

    /// Publish a fully checked envelope with no-replace atomic visibility.
    pub fn publish(&self, package: &VerifiedPackage) -> Result<PublishOutcome> {
        self.check_layout()?;
        // A package admitted with looser limits must satisfy this store's limits.
        let admitted = verify_package(package.bytes(), &self.limits)?;
        let identity = admitted.object_id();
        fs::create_directories(&self.root.join(".staging"))?;
        let staged = StagedFile::write(&self.root.join(".staging"), admitted.bytes())?;
        let staged_bytes = fs::read_regular_file(&staged.path, self.limits.max_envelope_bytes)?;
        let checked = self.admit_object(identity, &staged_bytes)?;
        if checked.bytes() != admitted.bytes() {
            return Err(Error::new(
                ErrorKind::Conflict,
                "staged registry object changed",
            ));
        }
        let _lock = self.publication_lock()?;
        let destination = self.object_path(identity);
        // Enforce the object-count budget under the same lock as publication.
        let paths = self.object_paths()?;
        if paths.len() >= self.limits.max_objects
            && !paths.iter().any(|(existing, _)| *existing == identity)
        {
            return Err(Error::new(
                ErrorKind::ResourceLimit,
                "registry object count exceeds limit",
            ));
        }
        let status = match std::fs::hard_link(&staged.path, &destination) {
            Ok(()) => {
                fs::sync_directory(&self.objects())?;
                PublishStatus::Published
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let existing = self.get(identity)?;
                if existing.bytes() != admitted.bytes() {
                    return Err(Error::new(
                        ErrorKind::Conflict,
                        "different exact bytes already occupy registry object",
                    ));
                }
                PublishStatus::AlreadyPresent
            }
            Err(error) => {
                return Err(Error::io(
                    "commit registry object by no-replace hard link",
                    error,
                ));
            }
        };
        staged.remove()?;
        Ok(PublishOutcome {
            object_id: identity,
            status,
        })
    }
}

struct StagedFile {
    path: PathBuf,
}

impl StagedFile {
    fn write(directory: &Path, bytes: &[u8]) -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        fs::require_directory(directory)?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::new(ErrorKind::Io, "local clock precedes Unix epoch"))?
            .as_nanos();
        let nonce = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!("{}-{timestamp}-{nonce}.tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&path)
            .map_err(|error| Error::io("create private registry staging file", error))?;
        let staged = Self { path };
        file.write_all(bytes)
            .map_err(|error| Error::io("write registry staging file", error))?;
        file.sync_all()
            .map_err(|error| Error::io("sync registry staging file", error))?;
        drop(file);
        Ok(staged)
    }

    fn remove(self) -> Result<()> {
        std::fs::remove_file(&self.path)
            .map_err(|error| Error::io("remove registry staging link", error))?;
        fs::sync_directory(self.path.parent().expect("staging file has a parent"))
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
