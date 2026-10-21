//! Bounded opaque artifact dependencies in an explicitly selected local lock.
//!
//! Pins commit exact package envelopes and artifact identities, not recovered
//! Rust types, source authenticity or source/IR/CLVM equivalence. No compiler,
//! VM, Cargo process or network client is executed by this module.
mod protocol;
mod resolve;
mod writer;

pub use protocol::{CheckedDependencyLock, DependencyPin, read_lock, verify_lock};
pub use resolve::ResolvedDependencies;

use crate::{Digest32, Result, store::Registry};
use serde::Serialize;
use std::path::Path;

pub const LOCK_PROTOCOL: &str = "chialisp-oxide-dependencies/v1";
pub const LOCK_SCOPE: &str = "local_exact_artifact_pins";
pub const MAX_LOCK_BYTES: usize = 1024 * 1024;
pub const MAX_DEPENDENCIES: usize = 128;
pub const VERIFICATION_SCOPE: &str = "Local bounded exact envelope and artifact pin resolution only; no recovered bindings, source authenticity, source/IR/CLVM equivalence, compiler or VM execution, authorization or suitability for real funds.";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AddStatus {
    Added,
    AlreadyPresent,
}

#[derive(Clone, Debug, Serialize)]
pub struct AddOutcome {
    pub status: AddStatus,
    pub pin: DependencyPin,
    pub lock_sha256: Digest32,
    pub dependency_count: usize,
}

/// Resolve the complete old lock and requested candidate in one bounded store
/// scan, then compare the exact snapshot under a nonblocking cooperating-writer
/// lock before committing. Parent directories must already exist.
///
/// An error after atomic commit can leave the complete new lock visible. Retry
/// revalidates that state; AlreadyPresent requires successful staging cleanup.
/// A lingering hardlink stage requires cleanup before the single-link lock can
/// be admitted. No post-commit rollback, hostile-filesystem or power-loss
/// guarantee is claimed. Writes require reviewed local Unix file semantics.
pub fn add_local(
    registry: &Registry,
    lock_path: &Path,
    name: &str,
    module_hash: Digest32,
    object_id: Option<Digest32>,
) -> Result<AddOutcome> {
    writer::add(registry, lock_path, name, module_hash, object_id)
}

#[cfg(test)]
mod tests;
