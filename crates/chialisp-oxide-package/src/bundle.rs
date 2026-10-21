//! Offline admission of one complete immutable Cargo export bundle.
//!
//! A bundle records declarations, not authenticated compiler/source provenance.
//! Import never follows `latest.json`, executes a tool or establishes IR/CLVM
//! equivalence. Every export is admitted before the requested name is selected.
use crate::{
    Digest32, Error, ErrorKind, Limits, Result,
    artifact::{ArtifactIdentity, Encoding, VerifiedArtifact, verify_artifact},
    digest,
    fs::read_regular_file,
};
use chialisp_oxide_ir::Program;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

const BUNDLE_STATUS: &str = "wip_prototype_not_ready_for_use";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BundleManifest {
    schema_version: u32,
    status: String,
    package: String,
    package_id: String,
    manifest_path: String,
    workspace_root: String,
    compiler: String,
    toolchain: String,
    analysis_target: String,
    #[serde(rename = "driver_sha256")]
    _driver_sha256: Digest32,
    rue_revision: String,
    cargo_options: Vec<String>,
    entries: Vec<CargoEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CargoEntry {
    entry: String,
    ir: String,
    clvm: String,
    artifact: CargoArtifact,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CargoArtifact {
    backend: String,
    optimize_lir: bool,
    bytes: u64,
    tree_hash: Digest32,
    clvm_hex: String,
    disassembly: String,
}

/// Complete-bundle admission retains exact bytes rather than reserializing IR.
/// These fields are internal inputs to the package-envelope constructor.
pub(crate) struct AdmittedBundle {
    pub(crate) manifest_json: String,
    pub(crate) entries: BTreeMap<String, AdmittedExport>,
}

pub(crate) struct AdmittedExport {
    pub(crate) ir_json: String,
    pub(crate) artifact: VerifiedArtifact,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidManifest, message)
}

fn resource(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::ResourceLimit, message)
}

fn nonempty(value: &str, label: &str) -> Result<()> {
    if value.is_empty() || value.contains('\0') {
        return Err(invalid(format!(
            "Cargo {label} must be nonempty and contain no NUL"
        )));
    }
    Ok(())
}

fn relative_file(root: &Path, raw: &str) -> Result<PathBuf> {
    let relative = Path::new(raw);
    if raw.is_empty()
        || raw.contains('\\')
        || raw.contains('\0')
        || raw
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(Error::new(
            ErrorKind::InvalidPath,
            "Cargo artifact path must be a normalized relative POSIX file path",
        ));
    }
    Ok(root.join(relative))
}

struct Reader<'a> {
    limits: &'a Limits,
    consumed: usize,
}
impl Reader<'_> {
    fn read(&mut self, path: &Path, maximum: usize) -> Result<Vec<u8>> {
        let remaining = self
            .limits
            .max_lookup_bytes
            .checked_sub(self.consumed)
            .ok_or_else(|| resource("Cargo bundle aggregate read limit exceeded"))?;
        let bytes = read_regular_file(path, maximum.min(remaining))?;
        self.consumed = self
            .consumed
            .checked_add(bytes.len())
            .ok_or_else(|| resource("Cargo bundle aggregate read accounting overflow"))?;
        Ok(bytes)
    }
}

fn declarations(manifest: &BundleManifest) -> Result<()> {
    if manifest.schema_version != 1 {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            "unsupported immutable Cargo bundle version",
        ));
    }
    if manifest.status != BUNDLE_STATUS {
        return Err(invalid("unsupported immutable Cargo bundle status"));
    }
    for (value, label) in [
        (&manifest.package, "package"),
        (&manifest.package_id, "package identity"),
        (&manifest.toolchain, "toolchain"),
        (&manifest.analysis_target, "analysis target"),
    ] {
        nonempty(value, label)?;
    }
    for path in [&manifest.manifest_path, &manifest.workspace_root] {
        if !Path::new(path).is_absolute() || path.contains('\0') {
            return Err(invalid(
                "declared Cargo workspace and manifest paths must be absolute",
            ));
        }
    }
    if !manifest.compiler.starts_with("rustc ") || manifest.compiler.contains('\0') {
        return Err(invalid(
            "Cargo compiler declaration requires full rustc -vV text",
        ));
    }
    let hosts: Vec<_> = manifest
        .compiler
        .lines()
        .filter_map(|line| line.strip_prefix("host: "))
        .collect();
    if hosts.as_slice() != [manifest.analysis_target.as_str()] {
        return Err(invalid(
            "declared compiler host differs from Cargo analysis target",
        ));
    }
    if manifest.rue_revision.len() != 40
        || !manifest
            .rue_revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(
            "Cargo Rue revision requires a full lowercase Git revision",
        ));
    }
    if manifest
        .cargo_options
        .iter()
        .any(|option| option.contains('\0'))
    {
        return Err(invalid("Cargo options cannot contain NUL"));
    }
    Ok(())
}

fn wire_version(value: &Value) -> Result<u32> {
    if !value.is_object() {
        return Err(invalid("Cargo IR must be an object"));
    }
    match value.get("ir_version") {
        None => Ok(1),
        Some(version) => match version.as_u64() {
            Some(version @ 2..=6) => Ok(version as u32),
            Some(_) => Err(Error::new(
                ErrorKind::UnsupportedVersion,
                "unsupported Cargo IR wire version",
            )),
            None => Err(invalid("Cargo IR wire version must be an integer")),
        },
    }
}

fn artifact_metadata(entry: &CargoArtifact, limits: &Limits) -> Result<()> {
    if entry.backend != "rust_mir" || entry.bytes == 0 || entry.disassembly.is_empty() {
        return Err(invalid(
            "Cargo entry requires nonempty Rust artifact metadata",
        ));
    }
    let bytes = usize::try_from(entry.bytes)
        .map_err(|_| resource("Cargo artifact length exceeds host bounds"))?;
    if bytes > limits.max_blob_bytes {
        return Err(resource("Cargo artifact exceeds blob byte limit"));
    }
    let hex_length = bytes
        .checked_mul(2)
        .ok_or_else(|| resource("Cargo artifact hex length overflow"))?;
    if entry.clvm_hex.len() != hex_length
        || !entry
            .clvm_hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(
            "Cargo artifact requires exact complete lowercase hexadecimal bytes",
        ));
    }
    Ok(())
}

fn artifact(entry: &CargoArtifact, limits: &Limits) -> Result<VerifiedArtifact> {
    artifact_metadata(entry, limits)?;
    let blob = hex::decode(&entry.clvm_hex)
        .map_err(|_| invalid("invalid Cargo artifact hexadecimal bytes"))?;
    let expected = ArtifactIdentity {
        encoding: Encoding::Plain,
        bytes: entry.bytes,
        blob_sha256: digest(&blob),
        module_tree_hash: entry.tree_hash,
    };
    verify_artifact(&blob, &expected, limits)
}

/// Decode a closed recorded manifest; this alone does not admit its files.
pub(crate) fn decode_manifest(bytes: &[u8], limits: &Limits) -> Result<BundleManifest> {
    let value = crate::json::decode_value(bytes, limits)?;
    let entries = value
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("Cargo manifest requires an export array"))?;
    if entries.is_empty() {
        return Err(invalid("Cargo manifest contains no exports"));
    }
    if entries.len() > limits.max_objects {
        return Err(resource(
            "Cargo bundle export count exceeds admission limit",
        ));
    }
    let manifest: BundleManifest = serde_json::from_value(value)
        .map_err(|error| invalid(format!("invalid closed Cargo manifest: {error}")))?;
    declarations(&manifest)?;
    let mut names = BTreeSet::new();
    let mut paths = BTreeSet::from([PathBuf::from("manifest.json")]);
    let mode = manifest.entries[0].artifact.optimize_lir;
    for entry in &manifest.entries {
        nonempty(&entry.entry, "export name")?;
        if entry.entry.len() > 256 || !names.insert(entry.entry.as_str()) {
            return Err(invalid(
                "Cargo bundle has duplicate or oversized export names",
            ));
        }
        if entry.artifact.optimize_lir != mode {
            return Err(invalid(
                "Cargo bundle entries disagree on recorded optimization mode",
            ));
        }
        artifact_metadata(&entry.artifact, limits)?;
        for relative in [&entry.ir, &entry.clvm] {
            if !paths.insert(relative_file(Path::new(""), relative)?) {
                return Err(invalid(
                    "Cargo bundle entries alias a retained artifact file",
                ));
            }
        }
    }
    Ok(manifest)
}

pub(crate) fn check_selected_metadata(
    manifest: &BundleManifest,
    entry: &str,
    artifact: &crate::package::ArtifactRecord,
) -> Result<()> {
    let selected = manifest
        .entries
        .iter()
        .find(|candidate| candidate.entry == entry)
        .ok_or_else(|| invalid("Rust package entry is absent from its recorded Cargo manifest"))?;
    let blob = hex::decode(&selected.artifact.clvm_hex)
        .map_err(|_| invalid("invalid selected Cargo artifact hexadecimal bytes"))?;
    if selected.artifact.backend != "rust_mir"
        || artifact.identity.encoding != Encoding::Plain
        || selected.artifact.bytes != artifact.identity.bytes
        || digest(&blob) != artifact.identity.blob_sha256
        || selected.artifact.tree_hash != artifact.identity.module_tree_hash
        || selected.artifact.clvm_hex != artifact.clvm_hex
    {
        return Err(Error::new(
            ErrorKind::DigestMismatch,
            "selected Cargo artifact receipt differs from the retained package artifact",
        ));
    }
    Ok(())
}

pub(crate) fn admit_bundle(manifest_path: &Path, limits: &Limits) -> Result<AdmittedBundle> {
    if manifest_path.file_name().and_then(|name| name.to_str()) != Some("manifest.json") {
        return Err(Error::new(
            ErrorKind::InvalidPath,
            "select an immutable Cargo manifest.json, not a latest pointer or output directory",
        ));
    }
    let root = manifest_path.parent().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidPath,
            "Cargo manifest has no bundle directory",
        )
    })?;
    let mut reader = Reader {
        limits,
        consumed: 0,
    };
    let manifest_bytes = reader.read(manifest_path, limits.max_envelope_bytes)?;
    let manifest = decode_manifest(&manifest_bytes, limits)?;
    // The complete metadata/path set was checked before any export is read or
    // selected. The bounded file reader additionally rejects symlinks/specials.
    let mut entries = BTreeMap::new();
    for entry in manifest.entries {
        let ir_bytes = reader.read(&relative_file(root, &entry.ir)?, limits.max_ir_bytes)?;
        let ir_value = crate::json::decode_value(&ir_bytes, limits)?;
        wire_version(&ir_value)?;
        let program: Program = serde_json::from_value(ir_value)
            .map_err(|error| invalid(format!("invalid Cargo IR for {}: {error}", entry.entry)))?;
        if program.name != entry.entry {
            return Err(invalid(
                "Cargo export name differs from its retained IR identity",
            ));
        }
        program.validate().map_err(|error| {
            invalid(format!(
                "invalid Cargo IR semantics for {}: {error}",
                entry.entry
            ))
        })?;
        let verified = artifact(&entry.artifact, limits)?;
        let file_limit = entry
            .artifact
            .clvm_hex
            .len()
            .checked_add(1)
            .ok_or_else(|| resource("Cargo CLVM text length overflow"))?;
        let clvm_file = reader.read(&relative_file(root, &entry.clvm)?, file_limit)?;
        if clvm_file.len() != file_limit
            || clvm_file.last() != Some(&b'\n')
            || &clvm_file[..clvm_file.len() - 1] != entry.artifact.clvm_hex.as_bytes()
        {
            return Err(Error::new(
                ErrorKind::DigestMismatch,
                "Cargo CLVM file differs from exact manifest artifact hex plus newline",
            ));
        }
        let ir_json =
            String::from_utf8(ir_bytes).map_err(|_| invalid("Cargo IR must be UTF-8 JSON"))?;
        entries.insert(
            entry.entry,
            AdmittedExport {
                ir_json,
                artifact: verified,
            },
        );
    }
    let manifest_json = String::from_utf8(manifest_bytes)
        .map_err(|_| invalid("Cargo manifest must be UTF-8 JSON"))?;
    Ok(AdmittedBundle {
        manifest_json,
        entries,
    })
}

/// Package a selected export only after admission of its complete recorded set.
/// Source attachments are explicitly absent in this artifact-only import lane.
pub fn import_bundle(
    manifest_path: &Path,
    entry: &str,
    limits: &Limits,
) -> Result<crate::VerifiedPackage> {
    let mut bundle = admit_bundle(manifest_path, limits)?;
    let selected = bundle.entries.remove(entry).ok_or_else(|| {
        Error::new(
            ErrorKind::UnknownObject,
            "requested Cargo export is absent from the fully admitted bundle",
        )
    })?;
    crate::package::from_rust_bundle(
        bundle.manifest_json,
        entry.to_owned(),
        selected.ir_json,
        selected.artifact,
        limits,
    )
}

#[cfg(test)]
mod tests;
