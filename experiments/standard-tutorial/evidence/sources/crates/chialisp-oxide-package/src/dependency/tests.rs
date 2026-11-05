//! Constructed host protocol/filesystem controls, never compiler/VM evidence.
use super::*;
use crate::{Error, ErrorKind, Limits, VerifiedPackage, digest, store::Registry, verify_package};
use serde_json::{Value, json};
use std::path::PathBuf;

mod operations;
mod protocol;
#[cfg(unix)]
mod writer;

fn package() -> VerifiedPackage {
    verify_package(br#"{"schema_version":1,"kind":"clvm_artifact","artifact":{"identity":{"encoding":"clvm-legacy-plain/v1","bytes":1,"blob_sha256":"76be8b528d0075f7aae98d6fa57a6d3c83ae480a8469e668d7b0af968995ac71","module_tree_hash":"4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"},"clvm_hex":"80"},"rust":null,"source_scope":"not_recorded","sources":[],"provenance":null}"#, &Limits::default()).unwrap()
}
fn variant() -> VerifiedPackage {
    let mut bytes = b" \n".to_vec();
    bytes.extend_from_slice(package().bytes());
    verify_package(&bytes, &Limits::default()).unwrap()
}
fn pin(name: &str, package: &VerifiedPackage) -> DependencyPin {
    DependencyPin {
        name: name.into(),
        object_id: package.object_id(),
        artifact: package.artifact().identity().clone(),
        binding: (),
    }
}
fn document(pins: Vec<DependencyPin>) -> Value {
    json!({"schema_version":1,"protocol":LOCK_PROTOCOL,"scope":LOCK_SCOPE,"dependencies":pins})
}
fn checked(pins: Vec<DependencyPin>) -> CheckedDependencyLock {
    verify_lock(&serde_json::to_vec(&document(pins)).unwrap()).unwrap()
}
struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    registry: Registry,
    lock: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        Self::with_limits(Limits::default())
    }
    fn with_limits(limits: Limits) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let registry = Registry::create(&root.join("registry"), limits).unwrap();
        Self {
            lock: root.join("chialisp-oxide.lock"),
            _temporary: temporary,
            root,
            registry,
        }
    }
    fn publish(&self, package: &VerifiedPackage) {
        self.registry.publish(package).unwrap();
    }
    fn object(&self, id: Digest32) -> PathBuf {
        self.registry
            .root()
            .join("objects")
            .join(format!("{id}.json"))
    }
    fn stage_files(&self) -> Vec<PathBuf> {
        std::fs::read_dir(&self.root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .as_encoded_bytes()
                    .windows(7)
                    .any(|s| s == b".stage-")
            })
            .collect()
    }
}
fn assert_kind<T>(result: crate::Result<T>, kind: ErrorKind) {
    match result {
        Ok(_) => panic!("expected {kind:?}"),
        Err(error) => assert_eq!(error.kind(), kind, "{error}"),
    }
}
