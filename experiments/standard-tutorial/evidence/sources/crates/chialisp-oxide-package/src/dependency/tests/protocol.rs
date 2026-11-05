use super::*;

#[test]
fn exact_json_bytes_closed_fields_and_required_null_are_preserved() {
    let mut bytes = b" \n\t".to_vec();
    bytes.extend_from_slice(
        &serde_json::to_vec(&document(vec![pin("payment", &package())])).unwrap(),
    );
    bytes.extend_from_slice(b" \n");
    let lock = verify_lock(&bytes).unwrap();
    assert_eq!(lock.bytes(), bytes);
    assert_eq!(lock.dependencies()[0], pin("payment", &package()));
    let base = document(vec![pin("payment", &package())]);
    for mutation in [
        "root_extra",
        "pin_extra",
        "artifact_extra",
        "binding_missing",
        "binding_object",
        "binding_false",
        "encoding_map",
        "schema_bool",
        "schema_float",
        "schema_string",
        "protocol",
        "scope",
        "uppercase_hash",
        "zero_bytes",
    ] {
        let mut value = base.clone();
        match mutation {
            "root_extra" => value["extra"] = json!(null),
            "pin_extra" => value["dependencies"][0]["extra"] = json!(null),
            "artifact_extra" => value["dependencies"][0]["artifact"]["extra"] = json!(null),
            "binding_missing" => {
                value["dependencies"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("binding");
            }
            "binding_object" => {
                value["dependencies"][0]["binding"] = json!({"schema":"unreviewed"})
            }
            "binding_false" => value["dependencies"][0]["binding"] = json!(false),
            "encoding_map" => {
                value["dependencies"][0]["artifact"]["encoding"] =
                    json!({"clvm-legacy-plain/v1":null})
            }
            "schema_bool" => value["schema_version"] = json!(true),
            "schema_float" => value["schema_version"] = json!(1.0),
            "schema_string" => value["schema_version"] = json!("1"),
            "protocol" => value["protocol"] = json!("other"),
            "scope" => value["scope"] = json!("authenticated"),
            "uppercase_hash" => value["dependencies"][0]["object_id"] = json!("AA".repeat(32)),
            "zero_bytes" => value["dependencies"][0]["artifact"]["bytes"] = json!(0),
            _ => unreachable!(),
        }
        assert_kind(
            verify_lock(&serde_json::to_vec(&value).unwrap()),
            ErrorKind::InvalidManifest,
        );
    }
    let mut future = base;
    future["schema_version"] = json!(2);
    assert_kind(
        verify_lock(&serde_json::to_vec(&future).unwrap()),
        ErrorKind::UnsupportedVersion,
    );
    let duplicate=br#"{"schema_version":1,"schema_version":1,"protocol":"chialisp-oxide-dependencies/v1","scope":"local_exact_artifact_pins","dependencies":[]}"#;
    assert_kind(verify_lock(duplicate), ErrorKind::InvalidManifest);
}

#[test]
fn names_order_and_aliases_have_bounded_exact_semantics() {
    let package = package();
    for name in [
        "",
        "1payment",
        "Payment",
        "a-b",
        "a/b",
        "a.b",
        "a b",
        "é",
        "a\0b",
        &"a".repeat(65),
    ] {
        assert_kind(
            verify_lock(&serde_json::to_vec(&document(vec![pin(name, &package)])).unwrap()),
            ErrorKind::InvalidManifest,
        );
    }
    let longest = "a".repeat(64);
    checked(vec![pin(&longest, &package)]);
    for pins in [
        vec![pin("a", &package), pin("a", &package)],
        vec![pin("b", &package), pin("a", &package)],
    ] {
        assert_kind(
            verify_lock(&serde_json::to_vec(&document(pins)).unwrap()),
            ErrorKind::InvalidManifest,
        );
    }
    assert_eq!(
        checked(vec![pin("a", &package), pin("b", &package)])
            .dependencies()
            .len(),
        2
    );
}

#[test]
fn fixed_byte_dependency_depth_and_node_budgets_fail_closed() {
    let mut bytes = serde_json::to_vec(&document(vec![])).unwrap();
    bytes.resize(MAX_LOCK_BYTES, b' ');
    assert_eq!(verify_lock(&bytes).unwrap().bytes().len(), MAX_LOCK_BYTES);
    bytes.push(b' ');
    assert_kind(verify_lock(&bytes), ErrorKind::ResourceLimit);
    let package = package();
    let pins: Vec<_> = (0..MAX_DEPENDENCIES)
        .map(|i| pin(&format!("p{i:03}"), &package))
        .collect();
    checked(pins.clone());
    let mut excessive = pins;
    excessive.push(pin("p128", &package));
    assert_kind(
        verify_lock(&serde_json::to_vec(&document(excessive)).unwrap()),
        ErrorKind::ResourceLimit,
    );
    // The fixed JSON budgets apply before closed-field rejection can inspect
    // the unsupported payload, including unknown fields and escaped keys.
    let mut deep = document(vec![]);
    let mut nested = json!(null);
    for _ in 0..17 {
        nested = json!([nested]);
    }
    deep["extra"] = nested;
    assert_kind(
        verify_lock(&serde_json::to_vec(&deep).unwrap()),
        ErrorKind::ResourceLimit,
    );
    let mut nodes = document(vec![]);
    nodes["extra"] = json!(vec![Value::Null; 8192]);
    assert_kind(
        verify_lock(&serde_json::to_vec(&nodes).unwrap()),
        ErrorKind::ResourceLimit,
    );
}
