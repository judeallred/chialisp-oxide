use crate::{Error, ErrorKind, Result};
use serde::Serialize;

/// Tighten-only eligibility for the fixed v1 source recipe. These host bounds
/// neither change compiler limits nor promise that an author's graph compiles.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GenerationLimits {
    pub max_artifact_bytes: usize,
    pub max_expanded_nodes: usize,
    pub max_pairs: usize,
    pub max_data_depth_root_one: usize,
    pub max_unique_atoms: usize,
    pub max_atom_payload_bytes: usize,
    pub max_total_unique_atom_payload_bytes: usize,
    /// Maximum logical live pair frontier at every postorder construction,
    /// including peaks within one helper; the opaque environment is excluded.
    pub max_live_pair_references: usize,
    pub pairs_per_helper: usize,
    pub max_construction_helpers: usize,
    pub max_source_bytes: usize,
    pub max_generation_work_units: usize,
}
impl Default for GenerationLimits {
    fn default() -> Self {
        Self {
            max_artifact_bytes: 65_536,
            max_expanded_nodes: 257,
            max_pairs: 128,
            max_data_depth_root_one: 64,
            max_unique_atoms: 128,
            max_atom_payload_bytes: 256,
            max_total_unique_atom_payload_bytes: 8192,
            max_live_pair_references: 5,
            pairs_per_helper: 8,
            max_construction_helpers: 16,
            max_source_bytes: 262_144,
            max_generation_work_units: 2_097_152,
        }
    }
}
impl GenerationLimits {
    pub fn validate(&self) -> Result<()> {
        let fixed = Self::default();
        for (name, value, cap) in [
            (
                "artifact bytes",
                self.max_artifact_bytes,
                fixed.max_artifact_bytes,
            ),
            (
                "expanded nodes",
                self.max_expanded_nodes,
                fixed.max_expanded_nodes,
            ),
            ("pairs", self.max_pairs, fixed.max_pairs),
            (
                "data depth",
                self.max_data_depth_root_one,
                fixed.max_data_depth_root_one,
            ),
            (
                "unique atoms",
                self.max_unique_atoms,
                fixed.max_unique_atoms,
            ),
            (
                "atom payload bytes",
                self.max_atom_payload_bytes,
                fixed.max_atom_payload_bytes,
            ),
            (
                "unique atom payload bytes",
                self.max_total_unique_atom_payload_bytes,
                fixed.max_total_unique_atom_payload_bytes,
            ),
            (
                "live pair references",
                self.max_live_pair_references,
                fixed.max_live_pair_references,
            ),
            (
                "pairs per helper",
                self.pairs_per_helper,
                fixed.pairs_per_helper,
            ),
            (
                "construction helpers",
                self.max_construction_helpers,
                fixed.max_construction_helpers,
            ),
            (
                "source bytes",
                self.max_source_bytes,
                fixed.max_source_bytes,
            ),
            (
                "generation work",
                self.max_generation_work_units,
                fixed.max_generation_work_units,
            ),
        ] {
            if value > cap {
                return Err(Error::new(
                    ErrorKind::InvalidManifest,
                    format!("generation {name} cannot exceed the fixed v1 cap {cap}"),
                ));
            }
        }
        if self.pairs_per_helper == 0 {
            return Err(Error::new(
                ErrorKind::InvalidManifest,
                "generation pairs_per_helper must be positive",
            ));
        }
        Ok(())
    }
}

pub(super) fn resource(message: &str) -> Error {
    Error::new(
        ErrorKind::ResourceLimit,
        format!("Rust generation {message}"),
    )
}
pub(super) fn add(a: usize, b: usize, label: &str) -> Result<usize> {
    a.checked_add(b).ok_or_else(|| resource(label))
}
pub(super) fn within(value: usize, maximum: usize, label: &str) -> Result<()> {
    if value > maximum {
        Err(resource(label))
    } else {
        Ok(())
    }
}
pub(super) fn reserve<T>(items: &mut Vec<T>, additional: usize, label: &str) -> Result<()> {
    items
        .try_reserve_exact(additional)
        .map_err(|_| resource(label))
}
pub(super) struct Work {
    pub used: usize,
    maximum: usize,
}
impl Work {
    pub fn new(maximum: usize) -> Self {
        Self { used: 0, maximum }
    }
    pub fn charge(&mut self, amount: usize) -> Result<()> {
        let next = add(self.used, amount, "work arithmetic overflow")?;
        within(next, self.maximum, "work limit exceeded")?;
        self.used = next;
        Ok(())
    }
}
