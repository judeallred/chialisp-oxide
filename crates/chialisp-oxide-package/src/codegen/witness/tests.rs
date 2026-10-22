//! Constructed source/admission controls; these execute no compiler or VM.
use super::*;
use crate::{
    Limits, VerifiedPackage,
    artifact::{Encoding, inspect_artifact},
    dependency::{DependencyPin, LOCK_PROTOCOL, LOCK_SCOPE, verify_lock},
    package::{ArtifactRecord, PackageKind, PackageManifest, SourceScope, encode_manifest},
};
use serde_json::json;

mod bounds;
mod resolution;
mod source;

fn package(bytes: &[u8]) -> VerifiedPackage {
    let artifact = inspect_artifact(bytes, Encoding::Plain, &Limits::default()).unwrap();
    encode_manifest(
        &PackageManifest {
            schema_version: 1,
            kind: PackageKind::ClvmArtifact,
            artifact: ArtifactRecord {
                identity: artifact.identity().clone(),
                clvm_hex: hex::encode(bytes),
            },
            rust: None,
            source_scope: SourceScope::NotRecorded,
            sources: Vec::new(),
            provenance: None,
        },
        &Limits::default(),
    )
    .unwrap()
}
fn lock(pins: &[(&str, &VerifiedPackage)]) -> CheckedDependencyLock {
    let pins: Vec<_> = pins
        .iter()
        .map(|(name, package)| DependencyPin {
            name: (*name).into(),
            object_id: package.object_id(),
            artifact: package.artifact().identity().clone(),
            binding: (),
        })
        .collect();
    verify_lock(&serde_json::to_vec(&json!({"schema_version":1,"protocol":LOCK_PROTOCOL,"scope":LOCK_SCOPE,"dependencies":pins})).unwrap()).unwrap()
}
struct Fixture {
    _root: tempfile::TempDir,
    registry: Registry,
}
impl Fixture {
    fn new(limits: Limits) -> Self {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().canonicalize().unwrap();
        let registry = Registry::create(&directory.join("registry"), limits).unwrap();
        Self {
            _root: root,
            registry,
        }
    }
    fn publish(&self, package: &VerifiedPackage) {
        self.registry.publish(package).unwrap();
    }
    fn generate(
        &self,
        package: &VerifiedPackage,
        limits: &WitnessGenerationLimits,
    ) -> Result<GeneratedWitnessRust> {
        generate_locked_witness_rust(
            &lock(&[("module", package)]),
            &self.registry,
            "module",
            limits,
        )
    }
}
fn generated(bytes: &[u8], limits: &WitnessGenerationLimits) -> Result<GeneratedWitnessRust> {
    let package = package(bytes);
    let fixture = Fixture::new(Limits::default());
    fixture.publish(&package);
    fixture.generate(&package, limits)
}
fn assert_kind<T>(result: Result<T>, expected: ErrorKind) {
    match result {
        Ok(_) => panic!("expected {expected:?}"),
        Err(error) => assert_eq!(error.kind(), expected, "{error}"),
    }
}
fn pair(first: &[u8], rest: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0xff];
    bytes.extend_from_slice(first);
    bytes.extend_from_slice(rest);
    bytes
}
fn atom(payload: &[u8]) -> Vec<u8> {
    let mut bytes = match payload {
        [] => return vec![0x80],
        [byte] if *byte < 0x80 => return vec![*byte],
        bytes if bytes.len() < 64 => vec![0x80 | bytes.len() as u8],
        bytes if bytes.len() < 8192 => vec![0xc0 | (bytes.len() >> 8) as u8, bytes.len() as u8],
        bytes => {
            assert!(bytes.len() < 1_048_576);
            vec![
                0xe0 | (bytes.len() >> 16) as u8,
                (bytes.len() >> 8) as u8,
                bytes.len() as u8,
            ]
        }
    };
    bytes.extend_from_slice(payload);
    bytes
}
