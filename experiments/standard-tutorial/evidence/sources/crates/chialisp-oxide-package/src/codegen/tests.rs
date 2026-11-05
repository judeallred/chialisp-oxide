//! Constructed generation and admission controls; no compiler/VM credit.
use super::*;
use crate::{
    Limits, VerifiedPackage,
    artifact::{Encoding, inspect_artifact},
    dependency::{DependencyPin, LOCK_PROTOCOL, LOCK_SCOPE, verify_lock},
    package::{
        ArtifactRecord, DeclaredProvenance, PackageKind, PackageManifest, SourceScope,
        encode_manifest,
    },
};
use serde_json::json;

mod bounds;
mod emitted;
mod resolution;

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
        limits: &GenerationLimits,
    ) -> Result<GeneratedRust> {
        generate_locked_rust(
            &lock(&[("module", package)]),
            &self.registry,
            "module",
            limits,
        )
    }
}
fn generated(bytes: &[u8], limits: &GenerationLimits) -> Result<GeneratedRust> {
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
    match payload {
        [] => vec![0x80],
        [byte] if *byte < 0x80 => vec![*byte],
        bytes if bytes.len() < 64 => {
            let mut out = vec![0x80 | bytes.len() as u8];
            out.extend_from_slice(bytes);
            out
        }
        bytes => {
            assert!(bytes.len() < 8192);
            let mut out = vec![0xc0 | (bytes.len() >> 8) as u8, bytes.len() as u8];
            out.extend_from_slice(bytes);
            out
        }
    }
}

#[test]
fn atom_and_nil_roots_are_applied_without_environment_reframing() {
    for (bytes, expected_constant) in [
        (vec![0x80], "const ATOM_0: Tree<'static> = Tree::Atom(&[]);"),
        (
            vec![0x01],
            "const ATOM_0: Tree<'static> = Tree::Atom(&[0x01]);",
        ),
        (
            vec![0x82, 0x00, 0x80],
            "const ATOM_0: Tree<'static> = Tree::Atom(&[0x00, 0x80]);",
        ),
        (
            vec![0x83, 0xff, 0xfe, 0x00],
            "const ATOM_0: Tree<'static> = Tree::Atom(&[0xff, 0xfe, 0x00]);",
        ),
    ] {
        let generated = generated(&bytes, &GenerationLimits::default()).unwrap();
        assert!(generated.source().contains(expected_constant));
        assert!(
            generated
                .source()
                .ends_with("    apply(&ATOM_0, environment)\n}\n")
        );
        assert!(!generated.source().contains("Tree::Pair"));
        assert_eq!(generated.record().metrics.construction_helpers, 1);
        assert_eq!(
            generated.record().artifact,
            package(&bytes).artifact().identity().clone()
        );
        assert_eq!(
            generated.record().source_sha256,
            digest(generated.source().as_bytes())
        );
        assert_eq!(
            generated.record().source_bytes,
            generated.source().len() as u64
        );
    }
}

#[test]
fn repeated_atoms_improper_tails_and_marker_bytes_have_exact_postorder_source() {
    // ((00 80 . ff fe 00) . 00 80): both atom payloads are opaque, including
    // zero padding and serialization marker bytes. The outer tail is improper.
    let first = [0x82, 0, 0x80];
    let second = [0x83, 0xff, 0xfe, 0];
    let bytes = pair(&pair(&first, &second), &first);
    let generated = generated(&bytes, &GenerationLimits::default()).unwrap();
    let source = generated.source();
    assert_eq!(source.matches("const ATOM_").count(), 2);
    assert!(source.contains("let node_2 = Tree::Pair(&ATOM_0, &ATOM_1);"));
    assert!(source.contains("let node_4 = Tree::Pair(&node_2, &ATOM_0);"));
    assert!(source.ends_with("    apply(&node_4, environment)\n}\n"));
    assert_eq!(
        generated.record().metrics.total_unique_atom_payload_bytes,
        5
    );
    assert_eq!(generated.record().metrics.pairs, 2);
}

#[test]
fn ordinary_chunks_forward_live_pairs_without_escaping_stack_values() {
    let mut bytes = vec![0x80];
    for _ in 0..17 {
        bytes = pair(&[0x80], &bytes);
    }
    let generated = generated(&bytes, &GenerationLimits::default()).unwrap();
    assert_eq!(generated.record().metrics.construction_helpers, 3);
    assert_eq!(generated.record().metrics.max_live_pair_references, 1);
    let source = generated.source();
    assert_eq!(source.matches("Tree::Pair(").count(), 17);
    // This right spine visits all eighteen nil leaves before completing its
    // pairs. The first eight completed pairs therefore end at postorder 25.
    assert!(source.contains("stage_1(environment, &node_25)"));
    assert!(source.contains(
        "fn stage_1<'a>(environment: &'a Tree<'a>, node_25: &'a Tree<'a>) -> Tree<'static>"
    ));
    assert!(source.contains("let node_26 = Tree::Pair(&ATOM_0, node_25);"));
    assert!(source.ends_with("    apply(&node_34, environment)\n}\n"));
}

#[test]
fn generic_curried_bytes_are_selected_whole_without_catalog_or_schema_rewrite() {
    // Independently spelled canonical curry of (* 2 input) with argument 2.
    let bytes = hex::decode("ff02ffff01ff12ff02ff0580ffff04ffff0102ff018080").unwrap();
    let generated = generated(&bytes, &GenerationLimits::default()).unwrap();
    assert_eq!(
        generated.record().artifact.module_tree_hash,
        package(&bytes).module_tree_hash()
    );
    assert_eq!(generated.source().matches("apply(").count(), 1);
    assert_eq!(generated.source().matches("pub(super) fn run").count(), 1);
    assert!(!generated.source().contains("#[chialisp"));
    assert_eq!(
        generated.source().matches("#[rustfmt::skip]").count(),
        generated.record().metrics.unique_atoms as usize
            + generated.record().metrics.construction_helpers as usize
    );
}

#[test]
fn retained_standard_bytes_are_eligible_with_the_measured_conservative_recipe() {
    let bytes = hex::decode(
        include_str!("../../tests/fixtures/mainnet-standard/pinned-standard-module-hex.body")
            .trim(),
    )
    .unwrap();
    let generated = generated(&bytes, &GenerationLimits::default()).unwrap();
    let metrics = &generated.record().metrics;
    assert_eq!(
        generated.record().artifact.module_tree_hash.to_hex(),
        "e9aaa49f45bad5c889b86ee3341550c155cfdd10c3a6757de618d20612fffd52"
    );
    assert_eq!(
        (
            metrics.expanded_nodes,
            metrics.pairs,
            metrics.unique_atoms,
            metrics.construction_helpers,
            metrics.max_live_pair_references
        ),
        (227, 113, 17, 15, 5)
    );
    assert_eq!(generated.source().matches("Tree::Pair(").count(), 113);
}

#[test]
fn recipe_corruption_is_detected_before_a_source_result_exists() {
    let artifact = package(&[0xff, 1, 2]);
    let limits = GenerationLimits::default();
    let mut work = Work::new(limits.max_generation_work_units);
    let mut recipe = recipe::Recipe::new(
        index::Index::new(artifact.artifact(), &limits, &mut work).unwrap(),
        &limits,
        &mut work,
    )
    .unwrap();
    recipe.nodes[2] = recipe::Node::Pair { first: 1, rest: 0 };
    assert_kind(
        recipe.verify(artifact.artifact().identity(), &limits, &mut work),
        ErrorKind::DigestMismatch,
    );
    let mut work = Work::new(limits.max_generation_work_units);
    let mut recipe = recipe::Recipe::new(
        index::Index::new(artifact.artifact(), &limits, &mut work).unwrap(),
        &limits,
        &mut work,
    )
    .unwrap();
    recipe.atoms[0] = recipe.atoms[1].clone();
    assert_kind(
        recipe.verify(artifact.artifact().identity(), &limits, &mut work),
        ErrorKind::DigestMismatch,
    );
}

#[test]
fn provenance_text_is_never_rust_syntax_and_identical_inputs_are_deterministic() {
    let original = package(&[0xff, 1, 2]);
    let mut manifest = original.manifest().clone();
    manifest.provenance = Some(DeclaredProvenance {
        label: "\"; compile_error!(\"injected\"); //".into(),
        source_url: None,
        revision: None,
        license: None,
    });
    let package = encode_manifest(&manifest, &Limits::default()).unwrap();
    let fixture = Fixture::new(Limits::default());
    fixture.publish(&package);
    let first = fixture
        .generate(&package, &GenerationLimits::default())
        .unwrap();
    let second = fixture
        .generate(&package, &GenerationLimits::default())
        .unwrap();
    assert_eq!(first.source(), second.source());
    assert_eq!(first.record(), second.record());
    assert!(!first.source().contains("injected"));
    assert!(!first.source().contains("compile_error"));
    assert_eq!(first.record().object_id, package.object_id());
}
