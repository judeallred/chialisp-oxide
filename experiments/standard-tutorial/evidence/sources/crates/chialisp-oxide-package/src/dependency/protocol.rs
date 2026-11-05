use super::{LOCK_PROTOCOL, LOCK_SCOPE, MAX_DEPENDENCIES, MAX_LOCK_BYTES};
use crate::{Digest32, Error, ErrorKind, Limits, Result, artifact::ArtifactIdentity, fs};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{io::Write, path::Path};

/// A raw declaration. Only CheckedDependencyLock and checked resolution attest
/// admission; directly deserializing this type does not validate a dependency.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyPin {
    pub name: String,
    pub object_id: Digest32,
    pub artifact: ArtifactIdentity,
    pub binding: (),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LockDocument {
    schema_version: u32,
    protocol: String,
    scope: String,
    dependencies: Vec<DependencyPin>,
}

pub struct CheckedDependencyLock {
    bytes: Vec<u8>,
    dependencies: Vec<DependencyPin>,
}
impl std::fmt::Debug for CheckedDependencyLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckedDependencyLock")
            .field("bytes", &self.bytes.len())
            .field("dependencies", &self.dependencies.len())
            .finish()
    }
}
impl CheckedDependencyLock {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn dependencies(&self) -> &[DependencyPin] {
        &self.dependencies
    }
}

pub(super) fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidManifest, message)
}
pub(super) fn resource(message: &str) -> Error {
    Error::new(ErrorKind::ResourceLimit, message)
}
pub(super) fn check_name(name: &str) -> Result<()> {
    let bytes = name.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 64
        || !bytes[0].is_ascii_lowercase()
        || !bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
    {
        return Err(invalid("dependency name must match [a-z][a-z0-9_]{0,63}"));
    }
    Ok(())
}
fn check_pins(pins: &[DependencyPin]) -> Result<()> {
    if pins.len() > MAX_DEPENDENCIES {
        return Err(resource("dependency count exceeds lock limit"));
    }
    for pin in pins {
        check_name(&pin.name)?;
        if pin.artifact.bytes == 0 {
            return Err(invalid("dependency artifact cannot be empty"));
        }
    }
    if !pins.windows(2).all(|p| p[0].name < p[1].name) {
        return Err(invalid("dependency names must be sorted and unique"));
    }
    Ok(())
}

/// Admit a closed, bounded v1 declaration while preserving its exact JSON bytes.
pub fn verify_lock(bytes: &[u8]) -> Result<CheckedDependencyLock> {
    let profile = Limits {
        max_envelope_bytes: MAX_LOCK_BYTES,
        max_json_depth: 16,
        max_json_nodes: 8192,
        ..Limits::default()
    };
    let value = crate::json::decode_value(bytes, &profile)?;
    let object = value
        .as_object()
        .ok_or_else(|| invalid("dependency lock must be an object"))?;
    let version = object
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid("dependency schema version must be an integer"))?;
    if version != 1 {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            "unsupported dependency lock version",
        ));
    }
    let pins = object
        .get("dependencies")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("dependency lock requires a dependency array"))?;
    if pins.len() > MAX_DEPENDENCIES {
        return Err(resource("dependency count exceeds lock limit"));
    }
    // Serde's unit type can accept a missing field. Check required explicit
    // null before typed decoding; no v1 binding declaration may be discarded.
    for pin in pins {
        if pin.as_object().and_then(|p| p.get("binding")) != Some(&Value::Null) {
            return Err(invalid("dependency binding must be present and null in v1"));
        }
    }
    let document: LockDocument = serde_json::from_value(value).map_err(|error| {
        Error::new(
            ErrorKind::InvalidManifest,
            format!("invalid closed dependency lock: {error}"),
        )
    })?;
    if document.protocol != LOCK_PROTOCOL || document.scope != LOCK_SCOPE {
        return Err(invalid("unsupported dependency protocol or scope"));
    }
    check_pins(&document.dependencies)?;
    Ok(CheckedDependencyLock {
        bytes: bytes.to_vec(),
        dependencies: document.dependencies,
    })
}

pub fn read_lock(path: &Path) -> Result<CheckedDependencyLock> {
    verify_lock(&fs::read_regular_file(path, MAX_LOCK_BYTES)?)
}

struct BoundedWriter {
    bytes: Vec<u8>,
    exceeded: bool,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_LOCK_BYTES.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(std::io::Error::other("dependency lock encoding limit"));
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|_| std::io::Error::other("allocate dependency lock"))?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(super) fn encode(pins: &[DependencyPin]) -> Result<CheckedDependencyLock> {
    // All strings/collections are closed and bounded before cloning or writing.
    check_pins(pins)?;
    let document = LockDocument {
        schema_version: 1,
        protocol: LOCK_PROTOCOL.into(),
        scope: LOCK_SCOPE.into(),
        dependencies: pins.to_vec(),
    };
    let mut writer = BoundedWriter {
        bytes: Vec::new(),
        exceeded: false,
    };
    if let Err(error) = serde_json::to_writer_pretty(&mut writer, &document) {
        return Err(if writer.exceeded {
            resource("dependency lock encoding byte limit exceeded")
        } else {
            Error::new(
                ErrorKind::ResourceLimit,
                format!("cannot encode bounded dependency lock: {error}"),
            )
        });
    }
    writer
        .write_all(b"\n")
        .map_err(|_| resource("dependency lock encoding byte limit exceeded"))?;
    verify_lock(&writer.bytes)
}
