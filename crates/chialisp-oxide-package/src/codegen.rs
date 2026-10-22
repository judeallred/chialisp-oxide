//! Deterministic ordinary local Rust from one exact locked CLVM artifact.
//!
//! This is bounded data generation, not compilation, an inferred Rust ABI,
//! authorization, or a compiler-preservation proof. The generated run helper
//! applies the exact admitted program to its caller's complete opaque Tree.
mod index;
mod limits;
pub mod output;
mod recipe;
pub mod witness;

use crate::{
    Digest32, Error, ErrorKind, Result, artifact::ArtifactIdentity,
    dependency::CheckedDependencyLock, digest, store::Registry,
};
pub use limits::GenerationLimits;
use limits::Work;
use serde::Serialize;

pub const RECIPE: &str = "chialisp-oxide-generated-tree/v1";
pub const GENERATION_SCOPE: &str = "Bounded ordinary local Rust source generation from exact admitted locked bytes; opaque whole environment, no inferred nominal ABI, compilation, authorization, source authentication, complete-spend validation or compiler preservation.";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GenerationMetrics {
    pub artifact_bytes: u64,
    pub expanded_nodes: u64,
    pub pairs: u64,
    pub atoms: u64,
    pub data_depth_root_one: u64,
    pub unique_atoms: u64,
    pub max_atom_payload_bytes: u64,
    pub total_unique_atom_payload_bytes: u64,
    /// Maximum logical pair frontier, including peaks inside helper chunks.
    pub max_live_pair_references: u64,
    pub construction_helpers: u64,
    pub generation_work_units: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GenerationRecord {
    pub schema_version: u32,
    pub recipe: &'static str,
    pub lock_sha256: Digest32,
    pub selected_alias: String,
    pub object_id: Digest32,
    pub artifact: ArtifactIdentity,
    pub source_sha256: Digest32,
    pub source_bytes: u64,
    pub metrics: GenerationMetrics,
    pub limits: GenerationLimits,
    pub scope: &'static str,
}

/// Only a complete successful generation constructs a value. The exact source
/// bytes and their receipt are immutable through this API.
#[derive(Debug)]
pub struct GeneratedRust {
    source: String,
    record: GenerationRecord,
}
impl GeneratedRust {
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn record(&self) -> &GenerationRecord {
        &self.record
    }
}

pub fn generate_locked_rust(
    lock: &CheckedDependencyLock,
    registry: &Registry,
    name: &str,
    limits: &GenerationLimits,
) -> Result<GeneratedRust> {
    limits.validate()?;
    // One complete scan, before alias selection, checks every pin and unrelated
    // stored object under the caller's actual registry limits. No default-limit
    // re-admission, per-alias package clone or selected-only lookup is used.
    let resolved = lock.resolve(registry)?;
    let package = resolved.get(name).ok_or_else(|| {
        Error::new(
            ErrorKind::UnknownObject,
            "selected dependency alias is absent from the complete resolved lock",
        )
    })?;
    let artifact = package.artifact();
    let mut work = Work::new(limits.max_generation_work_units);
    work.charge(lock.bytes().len())?;
    let lock_sha256 = digest(lock.bytes());
    let indexed = index::Index::new(artifact, limits, &mut work)?;
    let recipe = recipe::Recipe::new(indexed, limits, &mut work)?;
    recipe.verify(artifact.identity(), limits, &mut work)?;
    let source = recipe.emit(
        artifact.identity(),
        lock_sha256,
        package.object_id(),
        limits,
        &mut work,
    )?;
    work.charge(source.len())?;
    let source_sha256 = digest(source.as_bytes());
    // The alias is admitted by the checked lock and never used as Rust syntax.
    work.charge(name.len())?;
    let mut selected_alias = String::new();
    selected_alias
        .try_reserve_exact(name.len())
        .map_err(|_| limits::resource("cannot allocate selected alias"))?;
    selected_alias.push_str(name);
    let mut metrics = recipe.metrics(artifact);
    metrics.generation_work_units = work.used as u64;
    Ok(GeneratedRust {
        record: GenerationRecord {
            schema_version: 1,
            recipe: RECIPE,
            lock_sha256,
            selected_alias,
            object_id: package.object_id(),
            artifact: artifact.identity().clone(),
            source_sha256,
            source_bytes: source.len() as u64,
            metrics,
            limits: limits.clone(),
            scope: GENERATION_SCOPE,
        },
        source,
    })
}

#[cfg(test)]
mod tests;
