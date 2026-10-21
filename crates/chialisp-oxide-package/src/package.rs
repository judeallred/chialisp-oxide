//! A portable, exact-byte envelope. Attachments are checked declarations, not
//! proof that source, typed IR and CLVM have the same behavior.
use crate::{
    Digest32, Error, ErrorKind, Limits, Result,
    artifact::{ArtifactIdentity, VerifiedArtifact, verify_artifact},
    digest,
};
use chialisp_oxide_ir::{Environment, Program, Type};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

pub const PACKAGE_SCHEMA: u32 = 1;
pub const ABI_PROTOCOL: &str = "chialisp-oxide-structural-abi/v1";
pub const VERIFICATION_SCOPE: &str = "Offline bounded syntax, exact content identities and declared attachment consistency only; no source/IR/CLVM equivalence, compiler authentication, authorization, audit or suitability for real funds.";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRecord {
    pub identity: ArtifactIdentity,
    pub clvm_hex: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    ClvmArtifact,
    RustCargoExport,
}
impl<'de> Deserialize<'de> for PackageKind {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        crate::string_enum::deserialize(
            deserializer,
            &["clvm_artifact", "rust_cargo_export"],
            |value| match value {
                "clvm_artifact" => Some(Self::ClvmArtifact),
                "rust_cargo_export" => Some(Self::RustCargoExport),
                _ => None,
            },
        )
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceScope {
    NotRecorded,
    DeclaredFilesOnly,
}
impl<'de> Deserialize<'de> for SourceScope {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        crate::string_enum::deserialize(
            deserializer,
            &["not_recorded", "declared_files_only"],
            |value| match value {
                "not_recorded" => Some(Self::NotRecorded),
                "declared_files_only" => Some(Self::DeclaredFilesOnly),
                _ => None,
            },
        )
    }
}
/// Closed portable ABI vocabulary. Legacy IR readers retain their own wire
/// compatibility; unknown fields in a package ABI are never silently discarded.
/// Fieldless cases intentionally use empty structs: Serde internally tagged
/// unit variants ignore extra fields even with deny_unknown_fields.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AbiType {
    Atom {},
    Tree {},
    Bool {},
    Int {
        bits: u16,
        signed: bool,
    },
    Bytes {
        len: usize,
    },
    Tuple {
        items: Vec<AbiType>,
    },
    Pair {
        first: Box<AbiType>,
        rest: Box<AbiType>,
    },
    Optional {
        item: Box<AbiType>,
    },
    Unit {},
}
impl AbiType {
    // Called only after bounded JSON parsing and complete Program validation.
    // Consequently this conversion never traverses arbitrary host-created types.
    fn from_validated_ir(ty: &Type) -> Self {
        match ty {
            Type::Atom => Self::Atom {},
            Type::Tree => Self::Tree {},
            Type::Bool => Self::Bool {},
            Type::Int { bits, signed } => Self::Int {
                bits: *bits,
                signed: *signed,
            },
            Type::Bytes { len } => Self::Bytes { len: *len },
            Type::Tuple { items } => Self::Tuple {
                items: items.iter().map(Self::from_validated_ir).collect(),
            },
            Type::Pair { first, rest } => Self::Pair {
                first: Box::new(Self::from_validated_ir(first)),
                rest: Box::new(Self::from_validated_ir(rest)),
            },
            Type::Optional { item } => Self::Optional {
                item: Box::new(Self::from_validated_ir(item)),
            },
            Type::Unit => Self::Unit {},
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiParameter {
    pub name: String,
    pub ty: AbiType,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuralAbi {
    pub protocol: String,
    #[serde(deserialize_with = "deserialize_abi_environment")]
    pub environment: Environment,
    pub parameters: Vec<AbiParameter>,
    pub result: AbiType,
}
fn deserialize_abi_environment<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Environment, D::Error> {
    crate::string_enum::deserialize(
        deserializer,
        &["strict", "ignored", "raw"],
        |value| match value {
            "strict" => Some(Environment::Strict),
            "ignored" => Some(Environment::Ignored),
            "raw" => Some(Environment::Raw),
            _ => None,
        },
    )
}
impl StructuralAbi {
    fn from_program(program: &Program) -> Self {
        Self {
            protocol: ABI_PROTOCOL.into(),
            environment: program.environment,
            parameters: program
                .parameters
                .iter()
                .map(|p| AbiParameter {
                    name: p.name.clone(),
                    ty: AbiType::from_validated_ir(&p.ty),
                })
                .collect(),
            result: AbiType::from_validated_ir(&program.result),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RustExport {
    pub entry: String,
    pub ir_json: String,
    pub ir_sha256: Digest32,
    pub ir_wire_version: u32,
    pub abi: StructuralAbi,
    pub bundle_manifest_json: String,
    pub bundle_manifest_sha256: Digest32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAttachment {
    pub path: String,
    pub sha256: Digest32,
    pub content_hex: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclaredProvenance {
    pub label: String,
    pub source_url: Option<String>,
    pub revision: Option<String>,
    pub license: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub schema_version: u32,
    pub kind: PackageKind,
    pub artifact: ArtifactRecord,
    pub rust: Option<RustExport>,
    pub source_scope: SourceScope,
    pub sources: Vec<SourceAttachment>,
    pub provenance: Option<DeclaredProvenance>,
}
pub struct VerifiedPackage {
    bytes: Vec<u8>,
    object_id: Digest32,
    manifest: PackageManifest,
    artifact: VerifiedArtifact,
}
impl std::fmt::Debug for VerifiedPackage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VerifiedPackage")
            .field("summary", &self.summary())
            .field("envelope_bytes", &self.bytes.len())
            .field("artifact_stats", self.artifact.stats())
            .finish()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PackageSummary {
    pub object_id: Digest32,
    pub module_tree_hash: Digest32,
    pub blob_sha256: Digest32,
    pub kind: PackageKind,
    pub entry: Option<String>,
    pub source_scope: SourceScope,
    pub source_files: usize,
    pub scope: &'static str,
}
impl VerifiedPackage {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn object_id(&self) -> Digest32 {
        self.object_id
    }
    pub fn module_tree_hash(&self) -> Digest32 {
        self.artifact.identity().module_tree_hash
    }
    pub fn artifact(&self) -> &VerifiedArtifact {
        &self.artifact
    }
    pub fn manifest(&self) -> &PackageManifest {
        &self.manifest
    }
    pub fn summary(&self) -> PackageSummary {
        PackageSummary {
            object_id: self.object_id,
            module_tree_hash: self.module_tree_hash(),
            blob_sha256: self.artifact.identity().blob_sha256,
            kind: self.manifest.kind,
            entry: self.manifest.rust.as_ref().map(|r| r.entry.clone()),
            source_scope: self.manifest.source_scope,
            source_files: self.manifest.sources.len(),
            scope: VERIFICATION_SCOPE,
        }
    }
}
fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidManifest, message)
}
fn bounded_text(text: &str, max: usize, label: &str) -> Result<()> {
    if text.len() > max {
        return Err(Error::new(
            ErrorKind::ResourceLimit,
            format!("{label} exceeds local byte limit"),
        ));
    }
    if text.contains('\0') {
        return Err(invalid(format!("{label} contains NUL")));
    }
    Ok(())
}
fn hex_length(text: &str, max: usize, label: &str) -> Result<usize> {
    if text.len() / 2 > max {
        return Err(Error::new(
            ErrorKind::ResourceLimit,
            format!("{label} exceeds local byte limit"),
        ));
    }
    if !text.len().is_multiple_of(2)
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid(format!(
            "{label} requires an even number of lowercase hex digits"
        )));
    }
    Ok(text.len() / 2)
}
fn decode_hex(text: &str, max: usize, label: &str) -> Result<Vec<u8>> {
    hex_length(text, max, label)?;
    hex::decode(text).map_err(|_| invalid(format!("invalid {label}")))
}
fn valid_source_label(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.contains(['\\', '\0'])
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}
fn read_ir(text: &str, limits: &Limits) -> Result<(Program, u32)> {
    bounded_text(text, limits.max_ir_bytes, "IR attachment")?;
    let value = crate::json::decode_value(text.as_bytes(), limits)?;
    let version = match value.get("ir_version") {
        None => 1,
        Some(v) => v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| invalid("IR wire version must be an integer"))?,
    };
    if !(1..=6).contains(&version) {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            "unsupported IR wire version",
        ));
    }
    let program: Program = serde_json::from_value(value)
        .map_err(|e| invalid(format!("invalid typed IR attachment: {e}")))?;
    program
        .validate()
        .map_err(|e| invalid(format!("invalid typed IR attachment: {e}")))?;
    Ok((program, version))
}
fn check_rust(
    rust: &RustExport,
    artifact: &ArtifactRecord,
    limits: &Limits,
) -> Result<Option<crate::receipt::SourceReceipt>> {
    bounded_text(&rust.entry, 4096, "export identity")?;
    if rust.entry.is_empty() {
        return Err(invalid("empty Rust export identity"));
    }
    bounded_text(&rust.ir_json, limits.max_ir_bytes, "IR attachment")?;
    bounded_text(
        &rust.bundle_manifest_json,
        limits.max_envelope_bytes,
        "Cargo manifest attachment",
    )?;
    if digest(rust.ir_json.as_bytes()) != rust.ir_sha256
        || digest(rust.bundle_manifest_json.as_bytes()) != rust.bundle_manifest_sha256
    {
        return Err(Error::new(
            ErrorKind::DigestMismatch,
            "Rust attachment digest mismatch",
        ));
    }
    let (program, version) = read_ir(&rust.ir_json, limits)?;
    if program.name != rust.entry
        || version != rust.ir_wire_version
        || StructuralAbi::from_program(&program) != rust.abi
    {
        return Err(invalid(
            "Rust export/IR version/structural ABI attachment mismatch",
        ));
    }
    // These are recorded metadata relations. Offline verification never opens
    // the original host paths or authenticates which compiler produced them.
    let manifest = crate::bundle::decode_manifest(rust.bundle_manifest_json.as_bytes(), limits)?;
    crate::bundle::check_selected_metadata(&manifest, &rust.entry, artifact)?;
    if let Some(receipt) = &manifest.source_receipt {
        receipt.check_ir(&rust.entry, None, rust.ir_json.as_bytes())?;
    }
    Ok(manifest.source_receipt)
}
/// Admit an exact envelope without executing any attached artifact or tool.
pub fn verify_package(bytes: &[u8], limits: &Limits) -> Result<VerifiedPackage> {
    let value = crate::json::decode_value(bytes, limits)?;
    let version = value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| invalid("package schema_version must be an integer"))?;
    if version != u64::from(PACKAGE_SCHEMA) {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            "unsupported package schema version",
        ));
    }
    if let Some(encoding) = value
        .pointer("/artifact/identity/encoding")
        .and_then(|v| v.as_str())
        && encoding != "clvm-legacy-plain/v1"
    {
        return Err(Error::new(
            ErrorKind::UnsupportedEncoding,
            format!("unsupported artifact encoding: {encoding}"),
        ));
    }
    let manifest: PackageManifest = serde_json::from_value(value)
        .map_err(|e| invalid(format!("invalid package fields: {e}")))?;
    let blob = decode_hex(
        &manifest.artifact.clvm_hex,
        limits.max_blob_bytes,
        "CLVM artifact",
    )?;
    let artifact = verify_artifact(&blob, &manifest.artifact.identity, limits)?;
    let receipt = match (manifest.kind, &manifest.rust) {
        (PackageKind::ClvmArtifact, None) => None,
        (PackageKind::RustCargoExport, Some(rust)) => check_rust(rust, &manifest.artifact, limits)?,
        _ => return Err(invalid("package kind and Rust attachment disagree")),
    };
    if manifest.sources.len() > limits.max_sources {
        return Err(Error::new(
            ErrorKind::ResourceLimit,
            "too many declared source files",
        ));
    }
    if (manifest.sources.is_empty() && manifest.source_scope != SourceScope::NotRecorded)
        || (!manifest.sources.is_empty() && manifest.source_scope != SourceScope::DeclaredFilesOnly)
    {
        return Err(invalid(
            "source scope must state whether declared files were retained",
        ));
    }
    if let Some(receipt) = &receipt
        && (manifest.source_scope != SourceScope::DeclaredFilesOnly
            || manifest.sources.len() != receipt.files.len())
    {
        return Err(invalid(
            "schema-2 source receipt requires every declared source attachment",
        ));
    }
    let mut labels = BTreeSet::new();
    let mut total = 0usize;
    for source in &manifest.sources {
        if !valid_source_label(&source.path) || !labels.insert(&source.path) {
            return Err(Error::new(
                ErrorKind::InvalidPath,
                "source labels must be unique normalized relative POSIX paths",
            ));
        }
        let remaining = limits
            .max_total_source_bytes
            .checked_sub(total)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::ResourceLimit,
                    "aggregate source byte limit exceeded",
                )
            })?;
        // Check the aggregate remaining budget before allocating decoded bytes.
        let length = hex_length(
            &source.content_hex,
            limits.max_source_bytes.min(remaining),
            "source attachment",
        )?;
        total = total
            .checked_add(length)
            .filter(|n| *n <= limits.max_total_source_bytes)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::ResourceLimit,
                    "aggregate source byte limit exceeded",
                )
            })?;
        if let Some(receipt) = &receipt {
            let row = receipt
                .files
                .binary_search_by(|row| row.path.as_str().cmp(source.path.as_str()))
                .ok()
                .map(|index| &receipt.files[index])
                .ok_or_else(|| {
                    invalid("source attachment is absent from retained schema-2 receipt")
                })?;
            if row.bytes != length as u64 || row.sha256 != source.sha256 {
                return Err(Error::new(
                    ErrorKind::DigestMismatch,
                    "source attachment differs from retained source receipt",
                ));
            }
        }
        let data = decode_hex(&source.content_hex, length, "source attachment")?;
        if digest(&data) != source.sha256 {
            return Err(Error::new(
                ErrorKind::DigestMismatch,
                "declared source content digest mismatch",
            ));
        }
    }
    if let Some(p) = &manifest.provenance {
        bounded_text(&p.label, 4096, "provenance label")?;
        if p.label.is_empty() {
            return Err(invalid("empty provenance label"));
        }
        for (value, label) in [
            (&p.source_url, "source URL"),
            (&p.revision, "revision"),
            (&p.license, "license"),
        ] {
            if let Some(value) = value {
                bounded_text(value, 4096, label)?;
            }
        }
    }
    Ok(VerifiedPackage {
        bytes: bytes.to_vec(),
        object_id: digest(bytes),
        manifest,
        artifact,
    })
}
pub fn verify_package_file(path: &Path, limits: &Limits) -> Result<VerifiedPackage> {
    verify_package(
        &crate::fs::read_regular_file(path, limits.max_envelope_bytes)?,
        limits,
    )
}
/// Preflight the exact shape serde will write, including object-key nodes.
/// The input is a public host-created value, so byte-limited serialization alone
/// cannot protect its recursive ABI representation from exhausting the stack.
struct WriterShape<'a> {
    limits: &'a Limits,
    nodes: usize,
    text_bytes: usize,
}
impl WriterShape<'_> {
    fn text_length(&mut self, length: usize, maximum: usize, label: &str) -> Result<()> {
        if length > maximum {
            return Err(Error::new(
                ErrorKind::ResourceLimit,
                format!("{label} exceeds local byte limit"),
            ));
        }
        self.text_bytes = self
            .text_bytes
            .checked_add(length)
            .filter(|bytes| *bytes <= self.limits.max_envelope_bytes)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::ResourceLimit,
                    "package aggregate text byte limit exceeded",
                )
            })?;
        Ok(())
    }
    fn text(&mut self, value: &str, maximum: usize, label: &str) -> Result<()> {
        self.text_length(value.len(), maximum, label)?;
        bounded_text(value, maximum, label)
    }
    fn values(&mut self, depth: usize, count: usize) -> Result<()> {
        if count == 0 {
            return Ok(());
        }
        if depth > self.limits.max_json_depth.min(96) {
            return Err(Error::new(
                ErrorKind::ResourceLimit,
                "package JSON depth limit exceeded",
            ));
        }
        self.nodes = self
            .nodes
            .checked_add(count)
            .filter(|count| *count <= self.limits.max_json_nodes)
            .ok_or_else(|| {
                Error::new(ErrorKind::ResourceLimit, "package JSON node limit exceeded")
            })?;
        Ok(())
    }
    fn object(&mut self, depth: usize, fields: usize) -> Result<()> {
        self.values(depth, 1)?;
        self.values(depth + 1, fields)
    }
    fn length(&self, count: usize) -> Result<()> {
        if count > self.limits.max_json_nodes.saturating_sub(self.nodes) {
            return Err(Error::new(
                ErrorKind::ResourceLimit,
                "package JSON node limit exceeded",
            ));
        }
        Ok(())
    }
    fn abi_type(&mut self, root: &AbiType, root_depth: usize) -> Result<()> {
        self.length(1)?;
        let mut pending = Vec::new();
        pending.try_reserve(1).map_err(|_| {
            Error::new(
                ErrorKind::ResourceLimit,
                "ABI preflight stack allocation failed",
            )
        })?;
        pending.push((root, root_depth));
        while let Some((ty, depth)) = pending.pop() {
            let fields = match ty {
                AbiType::Int { .. } | AbiType::Pair { .. } => 3,
                AbiType::Bytes { .. } | AbiType::Tuple { .. } | AbiType::Optional { .. } => 2,
                _ => 1,
            };
            self.object(depth, fields)?;
            self.values(depth + 1, 1)?; // kind string
            let (children, child_depth) = match ty {
                AbiType::Int { .. } => {
                    self.values(depth + 1, 2)?;
                    (0, depth + 1)
                }
                AbiType::Bytes { .. } => {
                    self.values(depth + 1, 1)?;
                    (0, depth + 1)
                }
                AbiType::Tuple { items } => {
                    self.values(depth + 1, 1)?;
                    (items.len(), depth + 2)
                }
                AbiType::Pair { .. } => (2, depth + 1),
                AbiType::Optional { .. } => (1, depth + 1),
                _ => (0, depth + 1),
            };
            let queued = pending.len().checked_add(children).ok_or_else(|| {
                Error::new(
                    ErrorKind::ResourceLimit,
                    "ABI preflight stack size overflow",
                )
            })?;
            self.length(queued)?;
            if children != 0 && child_depth > self.limits.max_json_depth.min(96) {
                return Err(Error::new(
                    ErrorKind::ResourceLimit,
                    "package JSON depth limit exceeded",
                ));
            }
            pending.try_reserve(children).map_err(|_| {
                Error::new(
                    ErrorKind::ResourceLimit,
                    "ABI preflight stack allocation failed",
                )
            })?;
            match ty {
                AbiType::Tuple { items } => {
                    pending.extend(items.iter().rev().map(|item| (item, child_depth)))
                }
                AbiType::Pair { first, rest } => {
                    pending.push((rest, child_depth));
                    pending.push((first, child_depth));
                }
                AbiType::Optional { item } => pending.push((item, child_depth)),
                _ => (),
            }
        }
        Ok(())
    }
}
fn preflight_manifest(manifest: &PackageManifest, limits: &Limits) -> Result<()> {
    let mut shape = WriterShape {
        limits,
        nodes: 0,
        text_bytes: 0,
    };
    shape.object(1, 7)?;
    shape.values(2, 3)?; // schema_version, kind, source_scope
    shape.object(2, 2)?; // artifact
    shape.object(3, 4)?; // identity
    shape.values(4, 4)?;
    shape.values(3, 1)?; // clvm_hex
    shape.text_length(
        manifest.artifact.clvm_hex.len(),
        limits.max_envelope_bytes,
        "CLVM hexadecimal text",
    )?;
    hex_length(
        &manifest.artifact.clvm_hex,
        limits.max_blob_bytes,
        "CLVM artifact",
    )?;
    if let Some(rust) = &manifest.rust {
        shape.object(2, 7)?;
        shape.values(3, 6)?; // all Rust receipt fields other than ABI
        shape.object(3, 4)?;
        shape.values(4, 3)?; // protocol, environment, parameters array
        shape.length(rust.abi.parameters.len())?;
        // Check recursive types before serde or any IR/source hashing.
        shape.abi_type(&rust.abi.result, 4)?;
        for parameter in &rust.abi.parameters {
            shape.object(5, 2)?;
            shape.values(6, 1)?; // parameter name
            shape.abi_type(&parameter.ty, 6)?;
            shape.text(&parameter.name, limits.max_ir_bytes, "ABI parameter name")?;
        }
        shape.text(&rust.abi.protocol, 4096, "ABI protocol")?;
        shape.text(&rust.entry, 4096, "export identity")?;
        shape.text(&rust.ir_json, limits.max_ir_bytes, "IR attachment")?;
        shape.text(
            &rust.bundle_manifest_json,
            limits.max_envelope_bytes,
            "Cargo manifest attachment",
        )?;
    } else {
        shape.values(2, 1)?;
    }
    shape.values(2, 1)?; // sources array
    if manifest.sources.len() > limits.max_sources {
        return Err(Error::new(
            ErrorKind::ResourceLimit,
            "too many declared source files",
        ));
    }
    shape.length(manifest.sources.len())?;
    let mut remaining = limits.max_total_source_bytes;
    for source in &manifest.sources {
        shape.object(3, 3)?;
        shape.values(4, 3)?;
        shape.text_length(source.path.len(), 4096, "source label")?;
        if !valid_source_label(&source.path) {
            return Err(Error::new(
                ErrorKind::InvalidPath,
                "source label is not a normalized relative POSIX path",
            ));
        }
        shape.text_length(
            source.content_hex.len(),
            limits.max_envelope_bytes,
            "source hexadecimal text",
        )?;
        let length = hex_length(
            &source.content_hex,
            limits.max_source_bytes.min(remaining),
            "source attachment",
        )?;
        remaining = remaining.checked_sub(length).ok_or_else(|| {
            Error::new(
                ErrorKind::ResourceLimit,
                "aggregate source byte limit exceeded",
            )
        })?;
    }
    if let Some(provenance) = &manifest.provenance {
        shape.object(2, 4)?;
        shape.values(3, 4)?;
        shape.text(&provenance.label, 4096, "provenance label")?;
        for (value, label) in [
            (&provenance.source_url, "source URL"),
            (&provenance.revision, "revision"),
            (&provenance.license, "license"),
        ] {
            if let Some(value) = value {
                shape.text(value, 4096, label)?;
            }
        }
    } else {
        shape.values(2, 1)?;
    }
    Ok(())
}
pub fn encode_manifest(manifest: &PackageManifest, limits: &Limits) -> Result<VerifiedPackage> {
    preflight_manifest(manifest, limits)?;
    // Serialize into a bounded writer so a host-created oversized manifest is
    // rejected before materializing an unbounded envelope.
    struct Writer {
        bytes: Vec<u8>,
        max: usize,
        exceeded: bool,
    }
    impl std::io::Write for Writer {
        fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
            if input.len() > self.max.saturating_sub(self.bytes.len()) {
                self.exceeded = true;
                return Err(std::io::Error::other("envelope byte limit exceeded"));
            }
            self.bytes.extend_from_slice(input);
            Ok(input.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Writer {
        bytes: Vec::new(),
        max: limits.max_envelope_bytes,
        exceeded: false,
    };
    if let Err(error) = serde_json::to_writer(&mut writer, manifest) {
        return Err(Error::new(
            if writer.exceeded {
                ErrorKind::ResourceLimit
            } else {
                ErrorKind::InvalidManifest
            },
            format!("encode package: {error}"),
        ));
    }
    verify_package(&writer.bytes, limits)
}
pub(crate) fn from_rust_bundle(
    bundle_manifest_json: String,
    entry: String,
    ir_json: String,
    artifact: VerifiedArtifact,
    sources: Vec<SourceAttachment>,
    limits: &Limits,
) -> Result<VerifiedPackage> {
    bounded_text(
        &bundle_manifest_json,
        limits.max_envelope_bytes,
        "Cargo manifest attachment",
    )?;
    let (program, version) = read_ir(&ir_json, limits)?;
    let rust = RustExport {
        entry,
        ir_sha256: digest(ir_json.as_bytes()),
        ir_wire_version: version,
        abi: StructuralAbi::from_program(&program),
        ir_json,
        bundle_manifest_sha256: digest(bundle_manifest_json.as_bytes()),
        bundle_manifest_json,
    };
    encode_manifest(
        &PackageManifest {
            schema_version: PACKAGE_SCHEMA,
            kind: PackageKind::RustCargoExport,
            artifact: ArtifactRecord {
                identity: artifact.identity().clone(),
                clvm_hex: hex::encode(artifact.bytes()),
            },
            rust: Some(rust),
            source_scope: if sources.is_empty() {
                SourceScope::NotRecorded
            } else {
                SourceScope::DeclaredFilesOnly
            },
            sources,
            provenance: None,
        },
        limits,
    )
}
#[cfg(test)]
#[path = "package_tests.rs"]
mod tests;
