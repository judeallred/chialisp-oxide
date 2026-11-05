use super::*;

#[test]
fn fixed_source_hashes_before_apply_and_preserves_both_complete_trees() {
    let bytes = hex::decode("ff12ff02ff0580").unwrap();
    let generated = generated(&bytes, &WitnessGenerationLimits::default()).unwrap();
    let source = generated.source();
    assert!(source.contains(
        "pub(super) fn run<'a>(program: &'a Tree<'a>, environment: &'a Tree<'a>) -> Tree<'static>"
    ));
    assert!(source.contains("    let program_hash = sha256tree(program);\n    assert!(program_hash == EXPECTED_PROGRAM_TREE_HASH);\n    apply(program, environment)\n}"));
    assert!(source.ends_with(super::super::recipe::HASH_HELPER));
    assert_eq!(source.matches("#[rustfmt::skip]").count(), 5);
    assert_eq!(source.matches("fn run<").count(), 1);
    assert_eq!(source.matches("fn sha256tree<").count(), 1);
    assert_eq!(source.matches("apply(program, environment)").count(), 1);
    assert!(!source.contains("#[chialisp"));
    assert!(!source.contains(".first("));
    assert!(!source.contains(".rest("));

    let expected = package(&bytes);
    let record = generated.record();
    assert_eq!(record.schema_version, 1);
    assert_eq!(record.recipe, RECIPE);
    assert_eq!(record.artifact, *expected.artifact().identity());
    assert_eq!(record.object_id, expected.object_id());
    assert_eq!(record.source_bytes, source.len() as u64);
    assert_eq!(record.source_sha256, digest(source.as_bytes()));
    assert_eq!(record.metrics.generated_source_bytes, record.source_bytes);
    assert_eq!(record.metrics.artifact_bytes, bytes.len() as u64);
    assert_eq!(record.metrics.source_helpers, 2);
    assert_eq!(record.metrics.commitment_bytes, 32);
    assert_eq!(record.metrics.embedded_program_bytes, 0);
    assert!(record.scope.contains("second-preimage"));
    assert!(
        record
            .scope
            .contains("does not commit original wire encoding")
    );
    assert!(record.scope.contains("not a runtime resource bound"));
}

#[test]
fn exact_commitment_is_the_only_selected_program_data_in_rust_literals() {
    let bytes = pair(&[1], &atom(&vec![0xa7; 8192]));
    let expected = package(&bytes);
    let generated = generated(&bytes, &WitnessGenerationLimits::default()).unwrap();
    let constant = generated
        .source()
        .split("const EXPECTED_PROGRAM_TREE_HASH: [u8; 32] = [")
        .nth(1)
        .unwrap()
        .split("];\n")
        .next()
        .unwrap();
    let actual: Vec<u8> = constant
        .split(", ")
        .map(|value| u8::from_str_radix(value.strip_prefix("0x").unwrap(), 16).unwrap())
        .collect();
    assert_eq!(actual.as_slice(), expected.module_tree_hash().as_bytes());
    assert_eq!(actual.len(), 32);
    assert!(!generated.source().contains(&"0xa7, ".repeat(33)));
    assert_eq!(generated.source().matches("const ").count(), 3);
    assert!(!generated.source().contains("Tree::Atom(&["));
    assert_eq!(generated.record().metrics.embedded_program_bytes, 0);
    assert!(generated.source().len() <= 8192);
}

#[test]
fn local_hash_body_and_fixed_upstream_span_provenance_remain_exact() {
    let source = generated(&[0x80], &WitnessGenerationLimits::default()).unwrap();
    for original in [
        include_str!("../../../../../../puzzles/sha256tree/puzzle.rs"),
        include_str!("../../../../../../puzzles/inner-puzzle/puzzle.rs"),
    ] {
        let function = original.find("fn sha256tree<'a>").unwrap();
        assert_eq!(&original[function..], super::super::recipe::HASH_HELPER);
    }
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../puzzles/sha256tree/puzzle.json"
    ))
    .unwrap();
    for field in ["revision", "source_path"] {
        assert!(
            source
                .source()
                .contains(manifest["provenance"]["upstream"][field].as_str().unwrap())
        );
    }
    assert_eq!(manifest["provenance"]["source_span"]["start_line"], 4);
    assert_eq!(manifest["provenance"]["source_span"]["end_line"], 10);
    assert!(
        source.source().contains(
            manifest["provenance"]["source_span"]["sha256"]
                .as_str()
                .unwrap()
        )
    );
    assert!(source.source().contains("Apache-2.0"));
    assert!(
        source
            .source()
            .contains("const ATOM_DOMAIN: [u8; 1] = [1];")
    );
    assert!(
        source
            .source()
            .contains("const PAIR_DOMAIN: [u8; 1] = [2];")
    );
}

#[test]
fn repeat_generation_is_exact_and_static_generation_has_no_witness_fallback() {
    let original = package(&[0xff, 1, 2]);
    let fixture = Fixture::new(Limits::default());
    fixture.publish(&original);
    let checked = lock(&[("module", &original)]);
    let static_before = crate::codegen::generate_locked_rust(
        &checked,
        &fixture.registry,
        "module",
        &crate::codegen::GenerationLimits::default(),
    )
    .unwrap();
    let a = generate_locked_witness_rust(
        &checked,
        &fixture.registry,
        "module",
        &WitnessGenerationLimits::default(),
    )
    .unwrap();
    let b = generate_locked_witness_rust(
        &checked,
        &fixture.registry,
        "module",
        &WitnessGenerationLimits::default(),
    )
    .unwrap();
    let static_after = crate::codegen::generate_locked_rust(
        &checked,
        &fixture.registry,
        "module",
        &crate::codegen::GenerationLimits::default(),
    )
    .unwrap();
    assert_eq!(a.source(), b.source());
    assert_eq!(a.record(), b.record());
    assert_eq!(static_before.source(), static_after.source());
    assert_eq!(static_before.record(), static_after.record());
    assert_eq!(
        static_before.record().recipe,
        "chialisp-oxide-generated-tree/v1"
    );
    assert!(
        static_before
            .source()
            .contains("fn run<'a>(environment: &'a Tree<'a>)")
    );
    assert!(!static_before.source().contains("sha256"));
    assert_ne!(static_before.source(), a.source());
    let profile = crate::codegen::GenerationLimits::default();
    assert_eq!(profile.max_atom_payload_bytes, 256);
    assert_eq!(profile.max_expanded_nodes, 257);
    assert_eq!(profile.max_data_depth_root_one, 64);
}
