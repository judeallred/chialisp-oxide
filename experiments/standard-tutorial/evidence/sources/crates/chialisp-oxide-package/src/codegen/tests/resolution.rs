use super::*;

#[test]
fn exact_envelope_variants_and_aliases_preserve_the_complete_lock_commitment() {
    let original = package(&[0xff, 1, 2]);
    let mut bytes = b" \n".to_vec();
    bytes.extend_from_slice(original.bytes());
    let variant = crate::verify_package(&bytes, &Limits::default()).unwrap();
    assert_eq!(original.module_tree_hash(), variant.module_tree_hash());
    assert_ne!(original.object_id(), variant.object_id());
    let fixture = Fixture::new(Limits::default());
    fixture.publish(&original);
    fixture.publish(&variant);
    let checked = lock(&[("a", &original), ("b", &variant), ("c", &original)]);
    let a = generate_locked_rust(
        &checked,
        &fixture.registry,
        "a",
        &GenerationLimits::default(),
    )
    .unwrap();
    let b = generate_locked_rust(
        &checked,
        &fixture.registry,
        "b",
        &GenerationLimits::default(),
    )
    .unwrap();
    let c = generate_locked_rust(
        &checked,
        &fixture.registry,
        "c",
        &GenerationLimits::default(),
    )
    .unwrap();
    assert_eq!(a.record().object_id, original.object_id());
    assert_eq!(b.record().object_id, variant.object_id());
    assert_eq!(a.record().artifact, b.record().artifact);
    assert_eq!(a.source(), c.source());
    assert_ne!(a.source(), b.source());
    assert_eq!(a.record().lock_sha256, digest(checked.bytes()));
    assert_eq!(c.record().selected_alias, "c");
    let mut whitespace = b"\n".to_vec();
    whitespace.extend_from_slice(checked.bytes());
    let changed = verify_lock(&whitespace).unwrap();
    let changed = generate_locked_rust(
        &changed,
        &fixture.registry,
        "a",
        &GenerationLimits::default(),
    )
    .unwrap();
    assert_ne!(a.source(), changed.source());
    assert_ne!(a.record().lock_sha256, changed.record().lock_sha256);
    assert_kind(
        generate_locked_rust(
            &checked,
            &fixture.registry,
            "absent",
            &GenerationLimits::default(),
        ),
        ErrorKind::UnknownObject,
    );
}

#[test]
fn selected_and_unselected_missing_tampered_and_unrelated_objects_fail_admission() {
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
            generate_locked_rust(
                &checked,
                &fixture.registry,
                "a",
                &GenerationLimits::default(),
            ),
            ErrorKind::UnknownObject,
        );
        std::fs::write(path(package), package.bytes()).unwrap();
        std::fs::write(path(package), b"corrupt").unwrap();
        assert_kind(
            generate_locked_rust(
                &checked,
                &fixture.registry,
                "a",
                &GenerationLimits::default(),
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
    assert_kind(
        generate_locked_rust(
            &checked,
            &fixture.registry,
            "a",
            &GenerationLimits::default(),
        ),
        ErrorKind::CorruptObject,
    );
    // Even an unknown alias cannot turn generation into a selected-only lookup.
    assert_kind(
        generate_locked_rust(
            &checked,
            &fixture.registry,
            "absent",
            &GenerationLimits::default(),
        ),
        ErrorKind::CorruptObject,
    );
}

#[test]
fn unselected_pin_artifact_mismatch_is_not_hidden_by_selected_alias() {
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
        generate_locked_rust(
            &checked,
            &fixture.registry,
            "a",
            &GenerationLimits::default(),
        ),
        ErrorKind::DigestMismatch,
    );
}

#[test]
fn caller_registry_budgets_are_preserved_and_aliases_do_not_repeat_admission() {
    let original = package(&[1]);
    let limits = Limits {
        max_lookup_bytes: original.bytes().len(),
        ..Limits::default()
    };
    let fixture = Fixture::new(limits);
    fixture.publish(&original);
    let pins: Vec<_> = (0..128)
        .map(|i| (format!("alias{i:03}"), &original))
        .collect();
    let borrowed: Vec<_> = pins
        .iter()
        .map(|(name, package)| (name.as_str(), *package))
        .collect();
    let checked = lock(&borrowed);
    generate_locked_rust(
        &checked,
        &fixture.registry,
        "alias127",
        &GenerationLimits::default(),
    )
    .unwrap();
    let constrained = Registry::open(
        fixture.registry.root(),
        Limits {
            max_lookup_bytes: original.bytes().len() - 1,
            ..Limits::default()
        },
    )
    .unwrap();
    assert_kind(
        generate_locked_rust(
            &checked,
            &constrained,
            "alias000",
            &GenerationLimits::default(),
        ),
        ErrorKind::ResourceLimit,
    );
    let constrained = Registry::open(
        fixture.registry.root(),
        Limits {
            max_blob_bytes: 0,
            ..Limits::default()
        },
    )
    .unwrap();
    assert_kind(
        generate_locked_rust(
            &checked,
            &constrained,
            "alias000",
            &GenerationLimits::default(),
        ),
        ErrorKind::ResourceLimit,
    );
}

#[test]
fn caller_expanded_source_limits_are_not_replaced_by_defaults() {
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
        .generate(&expanded, &GenerationLimits::default())
        .unwrap();
    assert_eq!(generated.record().object_id, expanded.object_id());
}
