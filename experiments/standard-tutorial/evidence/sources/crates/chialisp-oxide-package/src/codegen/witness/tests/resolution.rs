use super::*;

#[test]
fn aliases_stay_out_of_syntax_and_exact_envelope_and_raw_lock_pins_are_retained() {
    let original = package(&[0xff, 1, 2]);
    let mut bytes = b" \n".to_vec();
    bytes.extend_from_slice(original.bytes());
    let variant = crate::verify_package(&bytes, &Limits::default()).unwrap();
    assert_eq!(original.module_tree_hash(), variant.module_tree_hash());
    assert_ne!(original.object_id(), variant.object_id());
    let fixture = Fixture::new(Limits::default());
    fixture.publish(&original);
    fixture.publish(&variant);
    let checked = lock(&[
        ("a", &original),
        ("b", &variant),
        ("opaque_alias_never_in_source", &original),
    ]);
    let generate = |name| {
        generate_locked_witness_rust(
            &checked,
            &fixture.registry,
            name,
            &WitnessGenerationLimits::default(),
        )
        .unwrap()
    };
    let a = generate("a");
    let b = generate("b");
    let c = generate("opaque_alias_never_in_source");
    assert_eq!(a.record().artifact, b.record().artifact);
    assert_eq!(a.record().object_id, original.object_id());
    assert_eq!(b.record().object_id, variant.object_id());
    assert_eq!(a.source(), c.source());
    assert!(!c.source().contains("opaque_alias_never_in_source"));
    assert_eq!(c.record().selected_alias, "opaque_alias_never_in_source");
    assert_ne!(a.source(), b.source());
    assert_eq!(a.record().lock_sha256, digest(checked.bytes()));
    let mut whitespace = b"\n".to_vec();
    whitespace.extend_from_slice(checked.bytes());
    let changed = verify_lock(&whitespace).unwrap();
    let changed = generate_locked_witness_rust(
        &changed,
        &fixture.registry,
        "a",
        &WitnessGenerationLimits::default(),
    )
    .unwrap();
    assert_ne!(a.source(), changed.source());
    assert_ne!(a.record().lock_sha256, changed.record().lock_sha256);
    assert_eq!(a.record().artifact, changed.record().artifact);
    assert_kind(
        generate_locked_witness_rust(
            &checked,
            &fixture.registry,
            "absent",
            &WitnessGenerationLimits::default(),
        ),
        ErrorKind::UnknownObject,
    );
}

#[test]
fn every_selected_unselected_and_unrelated_store_fault_is_admitted_first() {
    let selected = package(&[1]);
    let unselected = package(&[2]);
    let fixture = Fixture::new(Limits::default());
    fixture.publish(&selected);
    fixture.publish(&unselected);
    let checked = lock(&[("a", &selected), ("b", &unselected)]);
    let path = |package: &VerifiedPackage| {
        fixture
            .registry
            .root()
            .join("objects")
            .join(format!("{}.json", package.object_id()))
    };
    for package in [&selected, &unselected] {
        std::fs::remove_file(path(package)).unwrap();
        assert_kind(
            generate_locked_witness_rust(
                &checked,
                &fixture.registry,
                "a",
                &WitnessGenerationLimits::default(),
            ),
            ErrorKind::UnknownObject,
        );
        std::fs::write(path(package), b"corrupt").unwrap();
        assert_kind(
            generate_locked_witness_rust(
                &checked,
                &fixture.registry,
                "a",
                &WitnessGenerationLimits::default(),
            ),
            ErrorKind::CorruptObject,
        );
        std::fs::write(path(package), package.bytes()).unwrap();
    }
    let unrelated = fixture
        .registry
        .root()
        .join("objects")
        .join(format!("{}.json", Digest32::new([9; 32])));
    std::fs::write(unrelated, b"unrelated corrupt object").unwrap();
    for alias in ["a", "absent"] {
        assert_kind(
            generate_locked_witness_rust(
                &checked,
                &fixture.registry,
                alias,
                &WitnessGenerationLimits::default(),
            ),
            ErrorKind::CorruptObject,
        );
    }
}

#[test]
fn unselected_pin_identity_mismatch_is_not_hidden_by_selection() {
    let selected = package(&[1]);
    let unselected = package(&[2]);
    let fixture = Fixture::new(Limits::default());
    fixture.publish(&selected);
    fixture.publish(&unselected);
    let checked = lock(&[("a", &selected), ("b", &unselected)]);
    let mut value: serde_json::Value = serde_json::from_slice(checked.bytes()).unwrap();
    value["dependencies"][1]["artifact"]["blob_sha256"] = json!("11".repeat(32));
    let checked = verify_lock(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_kind(
        generate_locked_witness_rust(
            &checked,
            &fixture.registry,
            "a",
            &WitnessGenerationLimits::default(),
        ),
        ErrorKind::DigestMismatch,
    );
}

#[test]
fn caller_budgets_and_unique_store_ownership_are_preserved_across_128_aliases() {
    let original = package(&[1]);
    let fixture = Fixture::new(Limits {
        max_lookup_bytes: original.bytes().len(),
        ..Limits::default()
    });
    fixture.publish(&original);
    let pins: Vec<_> = (0..128)
        .map(|i| (format!("alias{i:03}"), &original))
        .collect();
    let borrowed: Vec<_> = pins
        .iter()
        .map(|(name, package)| (name.as_str(), *package))
        .collect();
    let checked = lock(&borrowed);
    generate_locked_witness_rust(
        &checked,
        &fixture.registry,
        "alias127",
        &WitnessGenerationLimits::default(),
    )
    .unwrap();
    for limits in [
        Limits {
            max_lookup_bytes: original.bytes().len() - 1,
            ..Limits::default()
        },
        Limits {
            max_blob_bytes: 0,
            ..Limits::default()
        },
        Limits {
            max_nodes: 0,
            ..Limits::default()
        },
    ] {
        let constrained = Registry::open(fixture.registry.root(), limits).unwrap();
        assert_kind(
            generate_locked_witness_rust(
                &checked,
                &constrained,
                "alias000",
                &WitnessGenerationLimits::default(),
            ),
            ErrorKind::ResourceLimit,
        );
    }
}

#[test]
fn caller_expanded_source_profile_is_not_replaced_by_default_re_admission() {
    let count = Limits::default().max_sources + 1;
    let original = package(&[1]);
    let mut value: serde_json::Value = serde_json::from_slice(original.bytes()).unwrap();
    value["source_scope"] = json!("declared_files_only");
    value["sources"] = json!((0..count).map(|i| json!({"path":format!("declared/f{i:04}.rs"),"sha256":digest(&[]),"content_hex":""})).collect::<Vec<_>>());
    let bytes = serde_json::to_vec(&value).unwrap();
    let limits = Limits {
        max_sources: count,
        ..Limits::default()
    };
    let expanded = crate::verify_package(&bytes, &limits).unwrap();
    let fixture = Fixture::new(limits);
    fixture.publish(&expanded);
    let generated = fixture
        .generate(&expanded, &WitnessGenerationLimits::default())
        .unwrap();
    assert_eq!(generated.record().object_id, expanded.object_id());
}
