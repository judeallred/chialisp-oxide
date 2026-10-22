//! Explicit immutable output for a fully generated local Rust module.
//!
//! Generation and complete dependency admission happen before this operation.
//! The writer never replaces different bytes and never discovers or edits a
//! Cargo project. Local directories must not be replaced by a hostile process.
//! An error after publication can leave a complete destination and possibly a
//! staging link; no rollback, hostile-filesystem or power-loss guarantee exists.
use super::GeneratedRust;
use super::witness::GeneratedWitnessRust;
use crate::{Digest32, Error, ErrorKind, Result, dependency, digest, fs, store::Registry};
use serde::Serialize;
use std::{
    ffi::OsString,
    fs::{Metadata, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputStatus {
    Created,
    AlreadyPresent,
    Checked,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OutputRecord {
    pub status: OutputStatus,
    pub source_sha256: Digest32,
    pub source_bytes: u64,
}

/// Write a new source file, admit byte-identical existing source, or check the
/// existing file read-only. All paths are explicit. The supplied lock must
/// still have the exact bytes used during generation. Parent directories must
/// exist; the destination must be outside the selected registry and distinct
/// from the lock and reserved Cargo files. New writes require Unix filesystem
/// identities; generation and read-only checks remain portable.
///
/// A post-publication sync or cleanup error may leave complete source visible.
/// Retrying verifies it; a leftover hardlink stage must first be removed before
/// single-link admission can succeed. This operation performs no compilation.
pub fn write_generated_rust(
    generated: &GeneratedRust,
    output: &Path,
    lock_path: &Path,
    registry: &Registry,
    check: bool,
) -> Result<OutputRecord> {
    write_with(
        generated,
        output,
        lock_path,
        registry,
        check,
        &mut |_, _| Ok(()),
    )
}

/// Publish or check an explicitly generated witness helper using the same
/// immutable local-file contract as [`write_generated_rust`]. This is a separate
/// source recipe; the writer never switches modes or compiles either helper.
pub fn write_generated_witness_rust(
    generated: &GeneratedWitnessRust,
    output: &Path,
    lock_path: &Path,
    registry: &Registry,
    check: bool,
) -> Result<OutputRecord> {
    write_witness_with(
        generated,
        output,
        lock_path,
        registry,
        check,
        &mut |_, _| Ok(()),
    )
}

// Only the two immutable generated value types can reach the shared writer.
// This closed private view is not an arbitrary source or public trait API.
enum GeneratedSource<'a> {
    Static(&'a GeneratedRust),
    Witness(&'a GeneratedWitnessRust),
}
struct SourceRecord {
    lock_sha256: Digest32,
    source_sha256: Digest32,
    source_bytes: u64,
    max_source_bytes: usize,
}
impl GeneratedSource<'_> {
    fn source(&self) -> &str {
        match self {
            Self::Static(generated) => generated.source(),
            Self::Witness(generated) => generated.source(),
        }
    }
    fn record(&self) -> SourceRecord {
        match self {
            Self::Static(generated) => {
                let record = generated.record();
                SourceRecord {
                    lock_sha256: record.lock_sha256,
                    source_sha256: record.source_sha256,
                    source_bytes: record.source_bytes,
                    max_source_bytes: record.limits.max_source_bytes,
                }
            }
            Self::Witness(generated) => {
                let record = generated.record();
                SourceRecord {
                    lock_sha256: record.lock_sha256,
                    source_sha256: record.source_sha256,
                    source_bytes: record.source_bytes,
                    max_source_bytes: record.limits.max_source_bytes,
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    BeforeStageWrite,
    BeforeStageSync,
    BeforeReadback,
    BeforeCommit,
    AfterCommit,
    BeforeStageRemove,
    BeforeParentSync,
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidPath, message)
}
fn conflict(message: &str) -> Error {
    Error::new(ErrorKind::Conflict, message)
}
fn plain_file(metadata: &Metadata) -> Result<()> {
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(invalid(
            "generated output and lock must be plain regular files",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(invalid(
                "generated output and lock cannot be hardlink aliases",
            ));
        }
    }
    Ok(())
}
#[cfg(unix)]
fn same_file(left: &Metadata, right: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}
fn outside_registry(parent: &Path, registry: &Path) -> Result<()> {
    fs::require_directory(registry)?;
    #[cfg(unix)]
    {
        let root = std::fs::metadata(registry)
            .map_err(|error| Error::io("inspect generated source registry identity", error))?;
        for ancestor in parent.ancestors() {
            let metadata = std::fs::metadata(ancestor)
                .map_err(|error| Error::io("inspect generated source parent identity", error))?;
            if same_file(&root, &metadata) {
                return Err(invalid(
                    "generated source cannot be inside the selected registry",
                ));
            }
        }
    }
    #[cfg(not(unix))]
    {
        let root = registry
            .canonicalize()
            .map_err(|error| Error::io("resolve generated source registry", error))?;
        if parent.starts_with(&root) {
            return Err(invalid(
                "generated source cannot be inside the selected registry",
            ));
        }
    }
    Ok(())
}

struct Location {
    output: PathBuf,
    parent: PathBuf,
    lock: PathBuf,
}
impl Location {
    fn new(output: &Path, lock: &Path, registry: &Registry) -> Result<Self> {
        let output = fs::absolute_path(output)?;
        let name = output
            .file_name()
            .ok_or_else(|| invalid("generated source requires a file basename"))?;
        if name.to_str().is_some_and(|name| {
            ["cargo.toml", "cargo.lock"].iter().any(|reserved| {
                name.chars()
                    .flat_map(char::to_lowercase)
                    .eq(reserved.chars())
            })
        }) {
            return Err(invalid(
                "Cargo.toml and Cargo.lock are reserved generated source destinations",
            ));
        }
        let parent = output
            .parent()
            .ok_or_else(|| invalid("generated source has no parent"))?;
        fs::require_directory(parent)?;
        let parent = parent
            .canonicalize()
            .map_err(|error| Error::io("resolve generated source parent", error))?;
        outside_registry(&parent, registry.root())?;
        fs::check_no_symlink_components(lock)?;
        let lock_metadata = std::fs::symlink_metadata(lock)
            .map_err(|error| Error::io("inspect generation dependency lock", error))?;
        plain_file(&lock_metadata)?;
        let lock = lock
            .canonicalize()
            .map_err(|error| Error::io("resolve generation dependency lock", error))?;
        let output = parent.join(name);
        if output == lock || output.canonicalize().is_ok_and(|path| path == lock) {
            return Err(invalid(
                "generated source cannot replace its dependency lock",
            ));
        }
        Ok(Self {
            output,
            parent,
            lock,
        })
    }
    fn validate(&self, registry: &Registry, expected_lock: Digest32) -> Result<()> {
        fs::require_directory(&self.parent)?;
        outside_registry(&self.parent, registry.root())?;
        fs::check_no_symlink_components(&self.lock)?;
        let metadata = std::fs::symlink_metadata(&self.lock)
            .map_err(|error| Error::io("inspect generation dependency lock", error))?;
        plain_file(&metadata)?;
        let checked = dependency::read_lock(&self.lock)?;
        if digest(checked.bytes()) != expected_lock {
            return Err(conflict("dependency lock changed since source generation"));
        }
        #[cfg(unix)]
        if let Ok(output) = std::fs::symlink_metadata(&self.output)
            && same_file(&metadata, &output)
        {
            return Err(invalid("generated source aliases its dependency lock"));
        }
        Ok(())
    }
    fn snapshot(&self, max_bytes: usize) -> Result<Option<Vec<u8>>> {
        fs::require_directory(&self.parent)?;
        match std::fs::symlink_metadata(&self.output) {
            Ok(metadata) => {
                plain_file(&metadata)?;
                Ok(Some(fs::read_regular_file(&self.output, max_bytes)?))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Error::io("inspect generated source destination", error)),
        }
    }
    fn stage(
        &self,
        bytes: &[u8],
        hook: &mut impl FnMut(Phase, &Path) -> Result<()>,
    ) -> Result<Stage> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::new(ErrorKind::Io, "local clock precedes Unix epoch"))?
            .as_nanos();
        let mut name = OsString::from(".");
        name.push(
            self.output
                .file_name()
                .expect("checked generated source basename"),
        );
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
            .map_err(|error| Error::io("create private generated source stage", error))?;
        let stage = Stage { path };
        hook(Phase::BeforeStageWrite, &stage.path)?;
        file.write_all(bytes)
            .map_err(|error| Error::io("write generated source stage", error))?;
        hook(Phase::BeforeStageSync, &stage.path)?;
        file.sync_all()
            .map_err(|error| Error::io("sync generated source stage", error))?;
        drop(file);
        Ok(stage)
    }
}
struct Stage {
    path: PathBuf,
}
impl Stage {
    fn remove(&self) -> Result<()> {
        std::fs::remove_file(&self.path)
            .map_err(|error| Error::io("remove generated source staging link", error))
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn write_with(
    generated: &GeneratedRust,
    output: &Path,
    lock_path: &Path,
    registry: &Registry,
    check: bool,
    hook: &mut impl FnMut(Phase, &Path) -> Result<()>,
) -> Result<OutputRecord> {
    write_source_with(
        GeneratedSource::Static(generated),
        output,
        lock_path,
        registry,
        check,
        hook,
    )
}

fn write_witness_with(
    generated: &GeneratedWitnessRust,
    output: &Path,
    lock_path: &Path,
    registry: &Registry,
    check: bool,
    hook: &mut impl FnMut(Phase, &Path) -> Result<()>,
) -> Result<OutputRecord> {
    write_source_with(
        GeneratedSource::Witness(generated),
        output,
        lock_path,
        registry,
        check,
        hook,
    )
}

fn write_source_with(
    generated: GeneratedSource<'_>,
    output: &Path,
    lock_path: &Path,
    registry: &Registry,
    check: bool,
    hook: &mut impl FnMut(Phase, &Path) -> Result<()>,
) -> Result<OutputRecord> {
    #[cfg(not(unix))]
    if !check {
        return Err(Error::io(
            "generated source writes require reviewed Unix filesystem identities",
            io::Error::new(
                io::ErrorKind::Unsupported,
                "unsupported generated source writer platform",
            ),
        ));
    }
    let bytes = generated.source().as_bytes();
    let record = generated.record();
    if bytes.len() > record.max_source_bytes
        || record.source_bytes != u64::try_from(bytes.len()).unwrap_or(u64::MAX)
        || digest(bytes) != record.source_sha256
    {
        return Err(Error::new(
            ErrorKind::DigestMismatch,
            "generated source record differs from source bytes",
        ));
    }
    let location = Location::new(output, lock_path, registry)?;
    location.validate(registry, record.lock_sha256)?;
    let outcome = |status| OutputRecord {
        status,
        source_bytes: record.source_bytes,
        source_sha256: record.source_sha256,
    };
    if let Some(existing) = location.snapshot(record.max_source_bytes)? {
        if existing != bytes {
            return Err(conflict(
                "different bytes already occupy generated source destination",
            ));
        }
        return Ok(outcome(if check {
            OutputStatus::Checked
        } else {
            OutputStatus::AlreadyPresent
        }));
    }
    if check {
        return Err(conflict("generated source destination is absent"));
    }
    let stage = location.stage(bytes, hook)?;
    hook(Phase::BeforeReadback, &stage.path)?;
    let metadata = std::fs::symlink_metadata(&stage.path)
        .map_err(|error| Error::io("inspect generated source stage", error))?;
    plain_file(&metadata)?;
    let staged = fs::read_regular_file(&stage.path, record.max_source_bytes)?;
    if staged != bytes || digest(&staged) != record.source_sha256 {
        return Err(conflict(
            "generated source stage differs from exact generated bytes",
        ));
    }
    hook(Phase::BeforeCommit, &stage.path)?;
    location.validate(registry, record.lock_sha256)?;
    let status = match std::fs::hard_link(&stage.path, &location.output) {
        Ok(()) => {
            hook(Phase::AfterCommit, &location.output)?;
            OutputStatus::Created
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => OutputStatus::AlreadyPresent,
        Err(error) => {
            return Err(Error::io(
                "commit generated source without replacement",
                error,
            ));
        }
    };
    hook(Phase::BeforeStageRemove, &stage.path)?;
    stage.remove()?;
    // A competing writer may have won; admit its final destination only after
    // dropping our staging link. Never infer identity from AlreadyExists alone.
    if location.snapshot(record.max_source_bytes)?.as_deref() != Some(bytes) {
        return Err(conflict(
            "concurrent generated source destination has different bytes",
        ));
    }
    hook(Phase::BeforeParentSync, &location.parent)?;
    fs::sync_directory(&location.parent)?;
    Ok(outcome(status))
}

#[cfg(test)]
#[path = "output/tests.rs"]
mod tests;
