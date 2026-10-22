use crate::{Error, ErrorKind, Result, artifact::VerifiedArtifact};
use serde::Serialize;

/// Tighten-only host eligibility for the explicit v1 witness recipe. Artifact
/// bytes are never embedded; these bounds do not constrain the supplied runtime
/// program or promise either compiler-mode acceptance or runtime completion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WitnessGenerationLimits {
    pub max_artifact_bytes: usize,
    pub max_expanded_nodes: usize,
    pub max_data_depth_root_one: usize,
    pub max_source_bytes: usize,
    pub max_generation_work_units: usize,
}
impl Default for WitnessGenerationLimits {
    fn default() -> Self {
        Self {
            max_artifact_bytes: 2_097_152,
            max_expanded_nodes: 262_144,
            max_data_depth_root_one: 4096,
            max_source_bytes: 8192,
            max_generation_work_units: 2_097_152,
        }
    }
}
impl WitnessGenerationLimits {
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
            (
                "data depth",
                self.max_data_depth_root_one,
                fixed.max_data_depth_root_one,
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
                    format!("witness generation {name} cannot exceed the fixed v1 cap {cap}"),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn check_artifact(&self, artifact: &VerifiedArtifact) -> Result<()> {
        for (value, maximum, message) in [
            (
                artifact.identity().bytes,
                self.max_artifact_bytes,
                "witness artifact byte limit exceeded",
            ),
            (
                artifact.stats().nodes,
                self.max_expanded_nodes,
                "witness expanded node limit exceeded",
            ),
            (
                artifact.stats().depth,
                self.max_data_depth_root_one,
                "witness data depth limit exceeded",
            ),
        ] {
            // validate() bounds every maximum well below u64::MAX on all
            // supported hosts; comparisons need no artifact byte/tree copy.
            if value > maximum as u64 {
                return Err(super::super::limits::resource(message));
            }
        }
        Ok(())
    }
}
