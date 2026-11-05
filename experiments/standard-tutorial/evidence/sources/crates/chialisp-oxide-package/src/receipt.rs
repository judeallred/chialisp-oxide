//! Closed source declarations for Cargo bundle v2. Consistency is not build
//! authentication, complete input capture, or source/IR/CLVM equivalence.
use crate::{Digest32, Error, ErrorKind, Limits, Result, digest};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const RECEIPT_PROTOCOL: &str = "chialisp-oxide-declared-source/v1";
pub const RECEIPT_SCOPE: &str = "selected_unit_reads_and_launcher_snapshots";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceReceipt {
    pub protocol: String,
    pub scope: String,
    pub build: BuildDeclaration,
    pub unit: UnitDeclaration,
    pub exports: Vec<ExportDeclaration>,
    pub files: Vec<FileDeclaration>,
    pub snapshots: SnapshotDeclaration,
    pub omitted: OmittedReads,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildDeclaration {
    pub package: String,
    pub package_id: String,
    pub compiler_sha256: Digest32,
    pub toolchain: String,
    pub analysis_target: String,
    pub driver_sha256: Digest32,
    pub rue_revision: String,
    pub cargo_options: Vec<String>,
    pub optimize_lir: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitDeclaration {
    pub crate_name: String,
    pub analysis_target: String,
    pub features: Vec<String>,
    pub library_source: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportDeclaration {
    pub entry: String,
    pub ir: String,
    pub bytes: u64,
    pub sha256: Digest32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    CompilerBinary,
    CompilerText,
    LauncherBuildScriptSource,
    LauncherLockfilePostCargo,
    LauncherManifest,
}
impl<'de> Deserialize<'de> for ObservationKind {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        crate::string_enum::deserialize(
            deserializer,
            &[
                "compiler_binary",
                "compiler_text",
                "launcher_build_script_source",
                "launcher_lockfile_post_cargo",
                "launcher_manifest",
            ],
            |value| match value {
                "compiler_binary" => Some(Self::CompilerBinary),
                "compiler_text" => Some(Self::CompilerText),
                "launcher_build_script_source" => Some(Self::LauncherBuildScriptSource),
                "launcher_lockfile_post_cargo" => Some(Self::LauncherLockfilePostCargo),
                "launcher_manifest" => Some(Self::LauncherManifest),
                _ => None,
            },
        )
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileDeclaration {
    pub path: String,
    pub kinds: Vec<ObservationKind>,
    pub compiler_read_count: u64,
    pub bytes: u64,
    pub sha256: Digest32,
    pub blob: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotDeclaration {
    pub package_manifest: String,
    pub workspace_manifest: String,
    #[serde(deserialize_with = "required_optional_label")]
    pub lockfile: Option<String>,
    #[serde(deserialize_with = "required_optional_label")]
    pub build_script: Option<String>,
}
fn required_optional_label<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OmittedReads {
    pub outside_roots_reads: u64,
    pub outside_roots_bytes: u64,
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidManifest, message)
}
fn resource(message: &str) -> Error {
    Error::new(ErrorKind::ResourceLimit, message)
}
pub(crate) fn normalized_label(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}
fn scoped_label(path: &str) -> bool {
    normalized_label(path)
        && (path.starts_with("package/")
            || path.starts_with("generated/")
            || matches!(
                path,
                "workspace/Cargo.toml"
                    | "workspace/Cargo.lock"
                    | "entry/library.rs"
                    | "entry/build-script.rs"
            ))
}
fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}
fn charge(value: u64, total: &mut usize, max: usize, message: &str) -> Result<()> {
    let value = usize::try_from(value).map_err(|_| resource(message))?;
    *total = total
        .checked_add(value)
        .filter(|n| *n <= max)
        .ok_or_else(|| resource(message))?;
    Ok(())
}

impl SourceReceipt {
    /// Check closed declarations and host budgets. This does not read blobs or
    /// authenticate an invocation; complete bundle admission checks their bytes.
    pub fn validate(&self, limits: &Limits) -> Result<()> {
        if self.protocol != RECEIPT_PROTOCOL || self.scope != RECEIPT_SCOPE {
            return Err(invalid(
                "unsupported source receipt protocol or capture scope",
            ));
        }
        if self.exports.is_empty()
            || self.exports.len() > limits.max_objects
            || self.files.is_empty()
            || self.files.len() > limits.max_sources
            || self.unit.features.len() > limits.max_objects.min(128)
        {
            return Err(resource("source receipt collection limit exceeded"));
        }
        let mut feature_bytes = 0usize;
        for feature in &self.unit.features {
            feature_bytes = feature_bytes
                .checked_add(feature.len())
                .filter(|bytes| *bytes <= 64 * 1024)
                .ok_or_else(|| resource("source receipt feature byte limit exceeded"))?;
        }
        if self.unit.crate_name.is_empty()
            || self.unit.crate_name.len() > 256
            || !self
                .unit
                .crate_name
                .chars()
                .all(|c| c == '_' || c.is_alphanumeric())
            || self.unit.analysis_target != self.build.analysis_target
            || !sorted_unique(&self.unit.features)
            || self
                .unit
                .features
                .iter()
                .any(|s| s.is_empty() || s.len() > 4096 || s.chars().any(char::is_control))
        {
            return Err(invalid(
                "invalid selected source unit or feature declaration",
            ));
        }
        if !self.exports.windows(2).all(|p| p[0].entry < p[1].entry)
            || !self.files.windows(2).all(|p| p[0].path < p[1].path)
        {
            return Err(invalid("source receipt rows must be sorted and unique"));
        }
        let mut ir_paths = BTreeSet::new();
        for row in &self.exports {
            if row.entry.is_empty()
                || row.entry.len() > 256
                || row.entry.contains('\0')
                || !normalized_label(&row.ir)
                || !ir_paths.insert(&row.ir)
            {
                return Err(invalid("invalid source receipt export identity or path"));
            }
            if row.bytes == 0 || row.bytes > limits.max_ir_bytes as u64 {
                return Err(resource("source receipt IR byte limit exceeded"));
            }
        }
        let mut bytes = 0;
        let mut reads = 0;
        charge(
            self.omitted.outside_roots_reads,
            &mut reads,
            limits.max_source_read_events,
            "source receipt read-event limit exceeded",
        )?;
        if self.omitted.outside_roots_reads == 0 && self.omitted.outside_roots_bytes != 0 {
            return Err(invalid(
                "source receipt omitted bytes require an omitted read",
            ));
        }
        let mut read_bytes = 0;
        let mut files = BTreeMap::new();
        let mut blob_lengths = BTreeMap::new();
        for file in &self.files {
            if !scoped_label(&file.path)
                || file.kinds.is_empty()
                || !sorted_unique(&file.kinds)
                || file.blob != format!("source-{}.bin", file.sha256)
            {
                return Err(invalid(
                    "invalid source receipt label, kinds or content filename",
                ));
            }
            if file.bytes > limits.max_source_bytes as u64 {
                return Err(resource("source receipt file byte limit exceeded"));
            }
            if blob_lengths
                .insert(file.sha256, file.bytes)
                .is_some_and(|bytes| bytes != file.bytes)
            {
                return Err(invalid(
                    "shared source blob declarations disagree on byte length",
                ));
            }
            charge(
                file.bytes,
                &mut bytes,
                limits.max_total_source_bytes,
                "source receipt expanded byte limit exceeded",
            )?;
            let compiler_kinds = file
                .kinds
                .iter()
                .filter(|k| {
                    matches!(
                        k,
                        ObservationKind::CompilerText | ObservationKind::CompilerBinary
                    )
                })
                .count();
            if (compiler_kinds == 0 && file.compiler_read_count != 0)
                || (compiler_kinds > 0 && file.compiler_read_count < compiler_kinds as u64)
            {
                return Err(invalid(
                    "source receipt compiler read count disagrees with kinds",
                ));
            }
            if compiler_kinds > 0
                && !(file.path.starts_with("package/")
                    || file.path.starts_with("generated/")
                    || file.path == "entry/library.rs")
            {
                return Err(invalid(
                    "compiler source label is outside declared capture scope",
                ));
            }
            charge(
                file.compiler_read_count,
                &mut reads,
                limits.max_source_read_events,
                "source receipt read-event limit exceeded",
            )?;
            let repeated = file
                .bytes
                .checked_mul(file.compiler_read_count)
                .ok_or_else(|| resource("source receipt repeated read accounting overflow"))?;
            charge(
                repeated,
                &mut read_bytes,
                limits.max_source_read_bytes,
                "source receipt read byte limit exceeded",
            )?;
            files.insert(file.path.as_str(), file);
        }
        let require_kind = |label: &str, kind: ObservationKind| -> Result<()> {
            if !files
                .get(label)
                .is_some_and(|file| file.kinds.contains(&kind))
            {
                return Err(invalid(
                    "source receipt snapshot/library reference is missing its declared kind",
                ));
            }
            Ok(())
        };
        require_kind(&self.unit.library_source, ObservationKind::CompilerText)?;
        require_kind(
            &self.snapshots.package_manifest,
            ObservationKind::LauncherManifest,
        )?;
        require_kind(
            &self.snapshots.workspace_manifest,
            ObservationKind::LauncherManifest,
        )?;
        if let Some(path) = &self.snapshots.lockfile {
            require_kind(path, ObservationKind::LauncherLockfilePostCargo)?;
        }
        if let Some(path) = &self.snapshots.build_script {
            require_kind(path, ObservationKind::LauncherBuildScriptSource)?;
        }
        for file in &self.files {
            for kind in &file.kinds {
                let referenced = match kind {
                    ObservationKind::LauncherManifest => {
                        file.path == self.snapshots.package_manifest
                            || file.path == self.snapshots.workspace_manifest
                    }
                    ObservationKind::LauncherBuildScriptSource => {
                        self.snapshots.build_script.as_ref() == Some(&file.path)
                    }
                    ObservationKind::LauncherLockfilePostCargo => {
                        self.snapshots.lockfile.as_ref() == Some(&file.path)
                    }
                    _ => true,
                };
                if !referenced {
                    return Err(invalid("unreferenced launcher source declaration"));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn check_ir(&self, entry: &str, path: Option<&str>, bytes: &[u8]) -> Result<()> {
        let row = self
            .exports
            .iter()
            .find(|row| row.entry == entry)
            .ok_or_else(|| invalid("retained IR absent from source receipt export set"))?;
        if path.is_some_and(|path| path != row.ir)
            || row.bytes != bytes.len() as u64
            || row.sha256 != digest(bytes)
        {
            return Err(Error::new(
                ErrorKind::DigestMismatch,
                "source receipt differs from exact retained IR bytes",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
