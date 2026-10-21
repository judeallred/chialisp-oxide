use super::*;

#[test]
#[cfg(unix)]
fn unique_add_exact_alias_idempotence_and_readonly_resolution() {
    let fixture = Fixture::new();
    let package = package();
    fixture.publish(&package);
    let result = add_local(
        &fixture.registry,
        &fixture.lock,
        "payment",
        package.module_tree_hash(),
        None,
    )
    .unwrap();
    assert_eq!(result.status, AddStatus::Added);
    assert_eq!(result.pin, pin("payment", &package));
    assert_eq!(result.dependency_count, 1);
    let bytes = std::fs::read(&fixture.lock).unwrap();
    assert_eq!(result.lock_sha256, digest(&bytes));
    let mut whitespace = b" \n".to_vec();
    whitespace.extend_from_slice(&bytes);
    whitespace.extend_from_slice(b"\t ");
    std::fs::write(&fixture.lock, &whitespace).unwrap();
    let result = add_local(
        &fixture.registry,
        &fixture.lock,
        "payment",
        package.module_tree_hash(),
        Some(package.object_id()),
    )
    .unwrap();
    assert_eq!(result.status, AddStatus::AlreadyPresent);
    assert_eq!(result.lock_sha256, digest(&whitespace));
    assert_eq!(std::fs::read(&fixture.lock).unwrap(), whitespace);
    let lock = read_lock(&fixture.lock).unwrap();
    let resolved = lock.resolve(&fixture.registry).unwrap();
    assert_eq!(resolved.get("payment").unwrap().bytes(), package.bytes());
    assert!(resolved.get("missing").is_none());
    assert_eq!(resolved.entries().len(), 1);
    let sidecar = fixture.root.join(".chialisp-oxide.lock.write-lock");
    std::fs::remove_file(sidecar).unwrap();
    lock.resolve(&fixture.registry).unwrap();
    assert!(
        !fixture
            .root
            .join(".chialisp-oxide.lock.write-lock")
            .exists()
    );
}

#[test]
#[cfg(unix)]
fn ambiguity_explicit_selection_and_alias_rebinding_are_distinct() {
    let fixture = Fixture::new();
    let first = package();
    let other = variant();
    fixture.publish(&first);
    fixture.publish(&other);
    assert_kind(
        add_local(
            &fixture.registry,
            &fixture.lock,
            "payment",
            first.module_tree_hash(),
            None,
        ),
        ErrorKind::Conflict,
    );
    assert!(!fixture.lock.exists());
    add_local(
        &fixture.registry,
        &fixture.lock,
        "payment",
        first.module_tree_hash(),
        Some(first.object_id()),
    )
    .unwrap();
    let before = std::fs::read(&fixture.lock).unwrap();
    assert_kind(
        add_local(
            &fixture.registry,
            &fixture.lock,
            "payment",
            first.module_tree_hash(),
            Some(other.object_id()),
        ),
        ErrorKind::Conflict,
    );
    assert_kind(
        add_local(
            &fixture.registry,
            &fixture.lock,
            "unknown",
            Digest32::new([8; 32]),
            None,
        ),
        ErrorKind::UnknownObject,
    );
    assert_kind(
        add_local(
            &fixture.registry,
            &fixture.lock,
            "wrong",
            Digest32::new([8; 32]),
            Some(first.object_id()),
        ),
        ErrorKind::Conflict,
    );
    assert_kind(
        add_local(
            &fixture.registry,
            &fixture.lock,
            "missing",
            first.module_tree_hash(),
            Some(Digest32::new([9; 32])),
        ),
        ErrorKind::UnknownObject,
    );
    assert_eq!(std::fs::read(&fixture.lock).unwrap(), before);
}

#[test]
#[cfg(unix)]
fn every_artifact_identity_field_is_checked_against_actual_retrieval() {
    let fixture = Fixture::new();
    let package = package();
    fixture.publish(&package);
    for field in ["bytes", "blob_sha256", "module_tree_hash"] {
        let mut value = document(vec![pin("payment", &package)]);
        value["dependencies"][0]["artifact"][field] = if field == "bytes" {
            json!(2)
        } else {
            json!("22".repeat(32))
        };
        let lock = verify_lock(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_kind(lock.resolve(&fixture.registry), ErrorKind::DigestMismatch);
        std::fs::write(&fixture.lock, lock.bytes()).unwrap();
        assert_kind(
            add_local(
                &fixture.registry,
                &fixture.lock,
                "another",
                package.module_tree_hash(),
                Some(package.object_id()),
            ),
            ErrorKind::DigestMismatch,
        );
        assert_eq!(std::fs::read(&fixture.lock).unwrap(), lock.bytes());
    }
}

#[test]
#[cfg(unix)]
fn missing_locked_and_unrelated_corrupt_objects_cannot_be_bypassed() {
    let fixture = Fixture::new();
    let package = package();
    fixture.publish(&package);
    let lock = checked(vec![pin("payment", &package)]);
    std::fs::write(&fixture.lock, lock.bytes()).unwrap();
    std::fs::remove_file(fixture.object(package.object_id())).unwrap();
    assert_kind(lock.resolve(&fixture.registry), ErrorKind::UnknownObject);
    assert_kind(
        add_local(
            &fixture.registry,
            &fixture.lock,
            "another",
            package.module_tree_hash(),
            None,
        ),
        ErrorKind::UnknownObject,
    );
    fixture.publish(&package);
    std::fs::write(
        fixture.object(Digest32::new([7; 32])),
        b"corrupt unrelated bytes",
    )
    .unwrap();
    assert_kind(lock.resolve(&fixture.registry), ErrorKind::CorruptObject);
    assert_kind(
        add_local(
            &fixture.registry,
            &fixture.lock,
            "another",
            package.module_tree_hash(),
            Some(package.object_id()),
        ),
        ErrorKind::CorruptObject,
    );
    assert_eq!(std::fs::read(&fixture.lock).unwrap(), lock.bytes());
}

#[test]
#[cfg(unix)]
fn registry_total_budget_is_not_multiplied_by_aliases_or_requested_objects() {
    let first = package();
    let second = variant();
    let limits = Limits {
        max_lookup_bytes: first.bytes().len() + second.bytes().len() - 1,
        ..Limits::default()
    };
    let fixture = Fixture::with_limits(limits);
    fixture.publish(&first);
    fixture.publish(&second);
    assert_kind(
        checked(vec![pin("a", &first), pin("b", &second)]).resolve(&fixture.registry),
        ErrorKind::ResourceLimit,
    );
    assert_kind(
        add_local(
            &fixture.registry,
            &fixture.lock,
            "a",
            first.module_tree_hash(),
            Some(first.object_id()),
        ),
        ErrorKind::ResourceLimit,
    );
    assert!(!fixture.lock.exists());
}

#[test]
fn aliases_share_one_package_and_preserve_nondefault_registry_admission() {
    let count = Limits::default().max_sources + 1;
    let mut value: Value = serde_json::from_slice(package().bytes()).unwrap();
    value["source_scope"] = json!("declared_files_only");
    value["sources"]=json!((0..count).map(|i|json!({"path":format!("declared/f{i:04}.rs"),"sha256":digest(&[]),"content_hex":""})).collect::<Vec<_>>());
    let bytes = serde_json::to_vec(&value).unwrap();
    assert_kind(
        verify_package(&bytes, &Limits::default()),
        ErrorKind::ResourceLimit,
    );
    let limits = Limits {
        max_sources: count,
        ..Limits::default()
    };
    let package = verify_package(&bytes, &limits).unwrap();
    let fixture = Fixture::with_limits(limits);
    fixture.publish(&package);
    let lock = checked(
        (0..128)
            .map(|i| pin(&format!("alias{i:03}"), &package))
            .collect(),
    );
    let resolved = lock.resolve(&fixture.registry).unwrap();
    assert_eq!(resolved.entries().len(), 128);
    let first = resolved.get("alias000").unwrap();
    for (_, package) in resolved.entries() {
        assert!(std::ptr::eq(first, package));
    }
    assert_eq!(first.bytes(), bytes);
}

#[test]
#[cfg(unix)]
fn copied_lock_and_store_resolve_without_original_source_paths() {
    let fixture = Fixture::new();
    let package = package();
    fixture.publish(&package);
    add_local(
        &fixture.registry,
        &fixture.lock,
        "payment",
        package.module_tree_hash(),
        None,
    )
    .unwrap();
    let copy = Fixture::new();
    std::fs::copy(&fixture.lock, &copy.lock).unwrap();
    std::fs::copy(
        fixture.object(package.object_id()),
        copy.object(package.object_id()),
    )
    .unwrap();
    drop(fixture);
    let resolved = read_lock(&copy.lock)
        .unwrap()
        .resolve(&copy.registry)
        .unwrap();
    assert_eq!(resolved.get("payment").unwrap().artifact().bytes(), [0x80]);
}

#[cfg(not(unix))]
#[test]
fn unsupported_mutating_platform_fails_before_creating_coordination_or_content() {
    let fixture = Fixture::new();
    let package = package();
    fixture.publish(&package);
    assert_kind(
        add_local(
            &fixture.registry,
            &fixture.lock,
            "payment",
            package.module_tree_hash(),
            None,
        ),
        ErrorKind::Io,
    );
    assert!(!fixture.lock.exists());
    assert!(
        !fixture
            .root
            .join(".chialisp-oxide.lock.write-lock")
            .exists()
    );
    assert!(fixture.stage_files().is_empty());
}
