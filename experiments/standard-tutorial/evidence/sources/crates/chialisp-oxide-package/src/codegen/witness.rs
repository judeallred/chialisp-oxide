//! Explicit ordinary Rust for a lock-derived runtime program witness.
//!
//! The caller supplies a complete program and environment. A local Rust helper
//! hashes the logical program tree, checks one fixed commitment, and only then
//! applies it. This is foreign-program composition, not inferred Rust typing or
//! a replacement for the static source recipe. SHA/Apply remain outside the
//! proof models; runtime resource limits and cryptographic assumptions remain.
mod limits;
mod recipe;

use super::limits::Work;
use crate::{
    Digest32, Error, ErrorKind, Result, artifact::ArtifactIdentity,
    dependency::CheckedDependencyLock, digest, store::Registry,
};
pub use limits::WitnessGenerationLimits;
use serde::Serialize;

pub const RECIPE: &str = "chialisp-oxide-witness-rust/v1";
pub const GENERATION_SCOPE: &str = "Bounded ordinary local Rust source generation after complete exact lock resolution; a local logical-tree SHA-256 guard precedes strict Apply of the supplied complete program to its opaque whole environment. Runtime commitment assumes SHA-256 collision/second-preimage resistance and does not commit original wire encoding, envelope, source or authorship. Host eligibility is not a runtime resource bound, compilation guarantee, inferred nominal ABI, authorization, complete spend, source authentication or compiler-preservation proof.";

/// Host source-generation measurements, not runtime work, VM cost, or RAM.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WitnessGenerationMetrics {
    pub artifact_bytes: u64,
    pub expanded_nodes: u64,
    pub data_depth_root_one: u64,
    pub generated_source_bytes: u64,
    pub source_helpers: u64,
    pub commitment_bytes: u64,
    pub embedded_program_bytes: u64,
    /// Excludes the complete resolver's separately bounded admission work.
    pub generation_work_units: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WitnessGenerationRecord {
    pub schema_version: u32,
    pub recipe: &'static str,
    pub lock_sha256: Digest32,
    pub selected_alias: String,
    pub object_id: Digest32,
    pub artifact: ArtifactIdentity,
    pub source_sha256: Digest32,
    pub source_bytes: u64,
    pub metrics: WitnessGenerationMetrics,
    pub limits: WitnessGenerationLimits,
    pub scope: &'static str,
}

/// Constructed only after successful generation. Source and receipt have no
/// public constructor, deserializer, mutable accessor, or extension trait.
#[derive(Debug)]
pub struct GeneratedWitnessRust {
    source: String,
    record: WitnessGenerationRecord,
}
impl GeneratedWitnessRust {
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn record(&self) -> &WitnessGenerationRecord {
        &self.record
    }
}

/// Generate an explicit witness route; never retry or replace static generation.
/// Registry limits and every pin apply before alias selection. No artifact byte
/// clone, selected-only lookup, default-limit re-admission, Cargo process, VM,
/// compiler or network operation occurs here.
pub fn generate_locked_witness_rust(
    lock: &CheckedDependencyLock,
    registry: &Registry,
    name: &str,
    limits: &WitnessGenerationLimits,
) -> Result<GeneratedWitnessRust> {
    limits.validate()?;
    let resolved = lock.resolve(registry)?;
    let package = resolved.get(name).ok_or_else(|| {
        Error::new(
            ErrorKind::UnknownObject,
            "selected dependency alias is absent from the complete resolved lock",
        )
    })?;
    let artifact = package.artifact();
    limits.check_artifact(artifact)?;

    let mut work = Work::new(limits.max_generation_work_units);
    work.charge(lock.bytes().len())?;
    let lock_sha256 = digest(lock.bytes());
    let source = recipe::emit(
        artifact.identity(),
        lock_sha256,
        package.object_id(),
        limits,
        &mut work,
    )?;
    work.charge(source.len())?;
    let source_sha256 = digest(source.as_bytes());

    // Only fixed-size admitted metadata is copied; no package/tree is cloned.
    work.charge(std::mem::size_of::<ArtifactIdentity>())?;
    let artifact_identity = artifact.identity().clone();
    work.charge(std::mem::size_of::<WitnessGenerationLimits>())?;
    let selected_limits = limits.clone();
    // The checked alias is receipt data only and never enters Rust syntax.
    work.charge(name.len())?;
    let mut selected_alias = String::new();
    selected_alias
        .try_reserve_exact(name.len())
        .map_err(|_| super::limits::resource("cannot allocate witness alias"))?;
    selected_alias.push_str(name);
    let source_bytes = source.len() as u64;
    let metrics = WitnessGenerationMetrics {
        artifact_bytes: artifact.identity().bytes,
        expanded_nodes: artifact.stats().nodes,
        data_depth_root_one: artifact.stats().depth,
        generated_source_bytes: source_bytes,
        source_helpers: 2,
        commitment_bytes: 32,
        embedded_program_bytes: 0,
        generation_work_units: work.used as u64,
    };
    Ok(GeneratedWitnessRust {
        source,
        record: WitnessGenerationRecord {
            schema_version: 1,
            recipe: RECIPE,
            lock_sha256,
            selected_alias,
            object_id: package.object_id(),
            artifact: artifact_identity,
            source_sha256,
            source_bytes,
            metrics,
            limits: selected_limits,
            scope: GENERATION_SCOPE,
        },
    })
}

#[cfg(test)]
pub(super) enum OutputIntegrityFault {
    Text,
    Hash,
    Length,
    Limit,
}
#[cfg(test)]
impl GeneratedWitnessRust {
    /// Crate tests can exercise the writer's closed integrity checks without a
    /// production constructor, mutable record, or arbitrary-string writer.
    pub(super) fn with_output_fault(mut self, fault: OutputIntegrityFault) -> Self {
        match fault {
            OutputIntegrityFault::Text => self.source.push('\n'),
            OutputIntegrityFault::Hash => self.record.source_sha256 = Digest32::new([0; 32]),
            OutputIntegrityFault::Length => self.record.source_bytes += 1,
            OutputIntegrityFault::Limit => self.record.limits.max_source_bytes = 0,
        }
        self
    }
}

#[cfg(test)]
mod tests;
