use super::*;
use serde_json::{Value, json};
fn fixture() -> Value {
    json!({"schema_version":1,"kind":"clvm_artifact","artifact":{"identity":{"encoding":"clvm-legacy-plain/v1","bytes":1,
        "blob_sha256":"76be8b528d0075f7aae98d6fa57a6d3c83ae480a8469e668d7b0af968995ac71",
        "module_tree_hash":"4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"},"clvm_hex":"80"},
        "rust":null,"source_scope":"not_recorded","sources":[],"provenance":null})
}
fn admit(v: &Value) -> Result<VerifiedPackage> {
    verify_package(&serde_json::to_vec(v).unwrap(), &Limits::default())
}
#[test]
fn exact_envelope_identity_differs_from_blob_and_tree() {
    let value = fixture();
    let bytes = serde_json::to_vec(&value).unwrap();
    let p = verify_package(&bytes, &Limits::default()).unwrap();
    assert_eq!(p.object_id(), digest(&bytes));
    assert_ne!(p.object_id(), p.module_tree_hash());
    assert_ne!(p.object_id(), p.artifact().identity().blob_sha256);
    let spaced = serde_json::to_vec_pretty(&value).unwrap();
    let q = verify_package(&spaced, &Limits::default()).unwrap();
    assert_ne!(p.object_id(), q.object_id());
    assert_eq!(p.module_tree_hash(), q.module_tree_hash());
    assert_eq!(p.summary().scope, VERIFICATION_SCOPE);
    assert_eq!(p.summary().source_scope, SourceScope::NotRecorded);
}
#[test]
fn scope_and_all_declared_artifact_identities_are_checked() {
    for (pointer, replacement, expected) in [
        ("/schema_version", json!(2), ErrorKind::UnsupportedVersion),
        ("/schema_version", json!(true), ErrorKind::InvalidManifest),
        (
            "/artifact/identity/encoding",
            json!("clvm-legacy-backrefs/v1"),
            ErrorKind::UnsupportedEncoding,
        ),
        (
            "/artifact/identity/bytes",
            json!(2),
            ErrorKind::DigestMismatch,
        ),
        (
            "/artifact/identity/blob_sha256",
            json!("00".repeat(32)),
            ErrorKind::DigestMismatch,
        ),
        (
            "/artifact/identity/module_tree_hash",
            json!("00".repeat(32)),
            ErrorKind::DigestMismatch,
        ),
        (
            "/kind",
            json!("rust_cargo_export"),
            ErrorKind::InvalidManifest,
        ),
        (
            "/source_scope",
            json!("declared_files_only"),
            ErrorKind::InvalidManifest,
        ),
    ] {
        let mut bad = fixture();
        *bad.pointer_mut(pointer).unwrap() = replacement;
        assert_eq!(admit(&bad).unwrap_err().kind(), expected, "{pointer}");
    }
    let mut trailing = fixture();
    trailing["artifact"]["clvm_hex"] = json!("8080");
    trailing["artifact"]["identity"]["bytes"] = json!(2);
    trailing["artifact"]["identity"]["blob_sha256"] = json!(digest(&[0x80, 0x80]));
    assert_eq!(
        admit(&trailing).unwrap_err().kind(),
        ErrorKind::MalformedSerialization
    );
    for key in ["audit_passed", "security_proof"] {
        let mut bad = fixture();
        bad[key] = json!(true);
        assert_eq!(admit(&bad).unwrap_err().kind(), ErrorKind::InvalidManifest);
    }
}
#[test]
fn source_bytes_are_declarations_with_exact_digests_and_confined_labels() {
    let data = b"pub fn puzzle() {}";
    let mut value = fixture();
    value["source_scope"] = json!("declared_files_only");
    value["sources"] =
        json!([{"path":"src/lib.rs","sha256":digest(data),"content_hex":hex::encode(data)}]);
    let p = admit(&value).unwrap();
    assert_eq!(p.summary().source_files, 1);
    let mut bad = value.clone();
    bad["sources"][0]["content_hex"] = json!(hex::encode(b"pub fn puzzle() { panic!() }"));
    assert_eq!(admit(&bad).unwrap_err().kind(), ErrorKind::DigestMismatch);
    for path in ["../outside", "/absolute", "a//b", "a/./b", "a\\b", ""] {
        let mut bad = value.clone();
        bad["sources"][0]["path"] = json!(path);
        assert_eq!(admit(&bad).unwrap_err().kind(), ErrorKind::InvalidPath);
    }
    let mut bad = value.clone();
    bad["sources"]
        .as_array_mut()
        .unwrap()
        .push(value["sources"][0].clone());
    assert_eq!(admit(&bad).unwrap_err().kind(), ErrorKind::InvalidPath);
    let limits = Limits {
        max_total_source_bytes: data.len() - 1,
        ..Limits::default()
    };
    assert_eq!(
        verify_package(&serde_json::to_vec(&value).unwrap(), &limits)
            .unwrap_err()
            .kind(),
        ErrorKind::ResourceLimit
    );
}
#[test]
fn recursive_duplicates_and_byte_budgets_fail_before_admission() {
    let text = serde_json::to_string(&fixture()).unwrap();
    let duplicate = text.replacen(
        "\"kind\":\"clvm_artifact\"",
        "\"kind\":\"clvm_artifact\",\"kind\":\"rust_cargo_export\"",
        1,
    );
    assert_ne!(duplicate, text);
    assert_eq!(
        verify_package(duplicate.as_bytes(), &Limits::default())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidManifest
    );
    let p = admit(&fixture()).unwrap();
    let limits = Limits {
        max_envelope_bytes: p.bytes().len() - 1,
        ..Limits::default()
    };
    assert_eq!(
        verify_package(p.bytes(), &limits).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    assert_eq!(
        encode_manifest(p.manifest(), &limits).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
}

fn rust_manifest(program: Program) -> PackageManifest {
    // Constructed transport: a separately valid nil tree is deliberately not
    // asserted to be the compiled implementation of this typed IR declaration.
    let ir = serde_json::to_string_pretty(&program).unwrap() + " \n";
    let manifest = json!({
        "schema_version":1,"status":"wip_prototype_not_ready_for_use",
        "package":"fixture","package_id":"path+file:///declared/fixture#0.1.0",
        "manifest_path":"/declared/fixture/Cargo.toml","workspace_root":"/declared/fixture",
        "compiler":"rustc constructed-fixture\nhost: fixture-host\n",
        "toolchain":"nightly-fixture","analysis_target":"fixture-host",
        "driver_sha256":"00".repeat(32),"rue_revision":"a".repeat(40),
        "cargo_options":["--locked"],"entries":[{
            "entry":program.name,"ir":"puzzle.ir.json","clvm":"puzzle.clvm.hex",
            "artifact":{"backend":"rust_mir","optimize_lir":true,"bytes":1,
                "tree_hash":"4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a",
                "clvm_hex":"80","disassembly":"()"}
        }]
    });
    let artifact = crate::artifact::inspect_artifact(
        &[0x80],
        crate::artifact::Encoding::Plain,
        &Limits::default(),
    )
    .unwrap();
    from_rust_bundle(
        serde_json::to_string_pretty(&manifest).unwrap(),
        program.name,
        ir,
        artifact,
        &Limits::default(),
    )
    .unwrap()
    .manifest()
    .clone()
}
fn unit_program() -> Program {
    Program {
        name: "fixture::puzzle".into(),
        environment: Environment::Strict,
        parameters: vec![],
        result: Type::Unit,
        body: chialisp_oxide_ir::Expr::Unit,
        functions: vec![],
    }
}

#[test]
fn portable_rust_package_keeps_exact_receipts_and_only_structural_abi_credit() {
    let manifest = rust_manifest(unit_program());
    let package = encode_manifest(&manifest, &Limits::default()).unwrap();
    let retained = package.manifest().rust.as_ref().unwrap();
    assert_eq!(retained.entry, "fixture::puzzle");
    assert_eq!(retained.abi.protocol, ABI_PROTOCOL);
    assert_eq!(retained.abi.result, AbiType::Unit {});
    assert_eq!(retained.abi.environment, Environment::Strict);
    assert_eq!(retained.ir_wire_version, 1);
    assert!(retained.ir_json.ends_with(" \n"));
    assert_eq!(retained.ir_sha256, digest(retained.ir_json.as_bytes()));
    assert_eq!(
        retained.bundle_manifest_sha256,
        digest(retained.bundle_manifest_json.as_bytes())
    );
    assert_eq!(package.summary().source_scope, SourceScope::NotRecorded);
    assert_eq!(package.summary().source_files, 0);
    assert!(
        package
            .summary()
            .scope
            .contains("no source/IR/CLVM equivalence")
    );
    assert_eq!(
        verify_package(package.bytes(), &Limits::default())
            .unwrap()
            .object_id(),
        package.object_id()
    );
}

#[test]
fn rust_receipt_digests_identity_and_abi_corruptions_fail_closed() {
    let manifest = rust_manifest(unit_program());
    for mutation in [
        "ir-bytes",
        "bundle-bytes",
        "entry",
        "version",
        "protocol",
        "environment",
        "type",
        "parameter",
    ] {
        let mut bad = manifest.clone();
        let rust = bad.rust.as_mut().unwrap();
        let expected = match mutation {
            "ir-bytes" => {
                rust.ir_json.push(' ');
                ErrorKind::DigestMismatch
            }
            "bundle-bytes" => {
                rust.bundle_manifest_json.push(' ');
                ErrorKind::DigestMismatch
            }
            "entry" => {
                rust.entry = "other::puzzle".into();
                ErrorKind::InvalidManifest
            }
            "version" => {
                rust.ir_wire_version = 6;
                ErrorKind::InvalidManifest
            }
            "protocol" => {
                rust.abi.protocol = "inferred-security/v1".into();
                ErrorKind::InvalidManifest
            }
            "environment" => {
                rust.abi.environment = Environment::Ignored;
                ErrorKind::InvalidManifest
            }
            "type" => {
                rust.abi.result = AbiType::Atom {};
                ErrorKind::InvalidManifest
            }
            "parameter" => {
                rust.abi.parameters.push(AbiParameter {
                    name: "input".into(),
                    ty: AbiType::Unit {},
                });
                ErrorKind::InvalidManifest
            }
            _ => unreachable!(),
        };
        assert_eq!(
            verify_package(&serde_json::to_vec(&bad).unwrap(), &Limits::default())
                .unwrap_err()
                .kind(),
            expected,
            "{mutation}"
        );
    }
}

#[test]
fn optional_capability_and_exported_optional_input_remain_rejected_on_corruption() {
    let mut program = unit_program();
    program.result = Type::Optional {
        item: Box::new(Type::Unit),
    };
    program.body = chialisp_oxide_ir::Expr::OptionalNone { item: Type::Unit };
    let manifest = rust_manifest(program);
    assert_eq!(manifest.rust.as_ref().unwrap().ir_wire_version, 6);
    for input in [false, true] {
        let mut bad = manifest.clone();
        let rust = bad.rust.as_mut().unwrap();
        let mut ir: Value = serde_json::from_str(&rust.ir_json).unwrap();
        if input {
            ir["program"]["parameters"] =
                json!([{"name":"input","ty":{"kind":"optional","item":{"kind":"unit"}}}]);
        } else {
            ir["ir_version"] = json!(5);
        }
        rust.ir_json = serde_json::to_string(&ir).unwrap();
        rust.ir_sha256 = digest(rust.ir_json.as_bytes());
        assert_eq!(
            verify_package(&serde_json::to_vec(&bad).unwrap(), &Limits::default())
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidManifest
        );
    }
}

#[test]
fn nested_portable_abi_types_reject_unknown_keys_without_widening_legacy_ir() {
    let mut program = unit_program();
    let ty = Type::Tuple {
        items: vec![
            Type::Int {
                bits: 8,
                signed: false,
            },
            Type::Pair {
                first: Box::new(Type::Unit),
                rest: Box::new(Type::Unit),
            },
        ],
    };
    program.parameters.push(chialisp_oxide_ir::Parameter {
        name: "input".into(),
        ty: ty.clone(),
    });
    program.result = ty;
    program.body = chialisp_oxide_ir::Expr::Arg { index: 0 };
    let value = serde_json::to_value(rust_manifest(program)).unwrap();
    for path in [
        "/rust/abi/result",
        "/rust/abi/result/items/0",
        "/rust/abi/result/items/1",
        "/rust/abi/result/items/1/first",
        "/rust/abi/parameters/0/ty/items/0",
    ] {
        let mut bad = value.clone();
        bad.pointer_mut(path).unwrap()["security_proof"] = json!(true);
        assert_eq!(
            admit(&bad).unwrap_err().kind(),
            ErrorKind::InvalidManifest,
            "{path}"
        );
    }
    let mut program = unit_program();
    program.result = Type::Optional {
        item: Box::new(Type::Unit),
    };
    program.body = chialisp_oxide_ir::Expr::OptionalNone { item: Type::Unit };
    let mut bad = serde_json::to_value(rust_manifest(program)).unwrap();
    bad["rust"]["abi"]["result"]["item"]["trusted"] = json!(true);
    assert_eq!(admit(&bad).unwrap_err().kind(), ErrorKind::InvalidManifest);
}

#[test]
fn ir_and_bundle_text_bounds_precede_digest_work_and_digest_failure() {
    let manifest = rust_manifest(unit_program());
    let mut bad = manifest.clone();
    bad.rust.as_mut().unwrap().ir_sha256 = Digest32::new([0; 32]);
    let limits = Limits {
        max_ir_bytes: 1,
        ..Limits::default()
    };
    assert_eq!(
        verify_package(&serde_json::to_vec(&bad).unwrap(), &limits)
            .unwrap_err()
            .kind(),
        ErrorKind::ResourceLimit
    );
    assert_eq!(
        encode_manifest(&bad, &limits).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    // Exercise the receipt checker directly: a portable envelope necessarily
    // adds bytes around its contained receipt, so reducing its outer cap would
    // otherwise test only the outer JSON bound.
    let rust = manifest.rust.as_ref().unwrap();
    let limits = Limits {
        max_envelope_bytes: rust.bundle_manifest_json.len() - 1,
        ..Limits::default()
    };
    let mut rust = rust.clone();
    rust.bundle_manifest_sha256 = Digest32::new([0; 32]);
    assert_eq!(
        check_rust(&rust, &manifest.artifact, &limits)
            .unwrap_err()
            .kind(),
        ErrorKind::ResourceLimit
    );
}

#[test]
fn second_source_is_charged_against_remaining_budget_before_hex_decoding() {
    let mut value = fixture();
    value["source_scope"] = json!("declared_files_only");
    value["sources"] = json!([
        {"path":"src/one.rs","sha256":digest(b"abc"),"content_hex":"616263"},
        {"path":"src/two.rs","sha256":digest(b"de"),"content_hex":"6465"}
    ]);
    let exact = Limits {
        max_total_source_bytes: 5,
        ..Limits::default()
    };
    let encoded = serde_json::to_vec(&value).unwrap();
    assert!(verify_package(&encoded, &exact).is_ok());
    let short = Limits {
        max_total_source_bytes: 4,
        ..Limits::default()
    };
    assert_eq!(
        verify_package(&encoded, &short).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    value["sources"][1]["content_hex"] = json!("zzzz");
    // Over-budget decoded length fails before the invalid hexadecimal syntax.
    let encoded = serde_json::to_vec(&value).unwrap();
    assert_eq!(
        verify_package(&encoded, &short).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    assert_eq!(
        verify_package(&encoded, &exact).unwrap_err().kind(),
        ErrorKind::InvalidManifest
    );
    let typed: PackageManifest = serde_json::from_value(value).unwrap();
    assert_eq!(
        encode_manifest(&typed, &short).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
}

fn json_measurement(value: &Value) -> (usize, usize) {
    let mut nodes = 0;
    let mut depth = 0;
    let mut pending = vec![(value, 1)];
    while let Some((value, current)) = pending.pop() {
        nodes += 1;
        depth = depth.max(current);
        match value {
            Value::Object(fields) => {
                for value in fields.values() {
                    nodes += 1;
                    depth = depth.max(current + 1); // key
                    pending.push((value, current + 1));
                }
            }
            Value::Array(items) => pending.extend(items.iter().map(|item| (item, current + 1))),
            _ => (),
        }
    }
    (nodes, depth)
}

#[test]
fn iterative_writer_preflight_matches_actual_json_type_depth_and_node_boundaries() {
    // Constructed serialization profiles, not assertions that optional inputs
    // are a supported Rust ABI. General JSON measurement is independent of the
    // package-specific preflight and checks serde's actual emitted shape.
    let original = rust_manifest(unit_program());
    for (ty, result_depth, parameter_depth) in [
        (AbiType::Unit {}, 5, 7),
        (
            AbiType::Tuple {
                items: vec![AbiType::Unit {}],
            },
            7,
            9,
        ),
        (
            AbiType::Pair {
                first: Box::new(AbiType::Unit {}),
                rest: Box::new(AbiType::Unit {}),
            },
            6,
            8,
        ),
        (
            AbiType::Optional {
                item: Box::new(AbiType::Unit {}),
            },
            6,
            8,
        ),
    ] {
        for parameter in [false, true] {
            let mut manifest = original.clone();
            let abi = &mut manifest.rust.as_mut().unwrap().abi;
            if parameter {
                abi.parameters.push(AbiParameter {
                    name: "input".into(),
                    ty: ty.clone(),
                });
            } else {
                abi.result = ty.clone();
            }
            let value = serde_json::to_value(&manifest).unwrap();
            let (nodes, depth) = json_measurement(&value);
            assert_eq!(
                depth,
                if parameter {
                    parameter_depth
                } else {
                    result_depth
                }
            );
            let exact = Limits {
                max_json_nodes: nodes,
                max_json_depth: depth,
                ..Limits::default()
            };
            preflight_manifest(&manifest, &exact).unwrap();
            crate::json::decode_value(&serde_json::to_vec(&value).unwrap(), &exact).unwrap();
            let shallow = Limits {
                max_json_depth: depth - 1,
                ..exact.clone()
            };
            assert_eq!(
                preflight_manifest(&manifest, &shallow).unwrap_err().kind(),
                ErrorKind::ResourceLimit
            );
            let fewer = Limits {
                max_json_nodes: nodes - 1,
                ..exact
            };
            assert_eq!(
                preflight_manifest(&manifest, &fewer).unwrap_err().kind(),
                ErrorKind::ResourceLimit
            );
        }
    }
}

#[test]
fn public_typed_writer_bounds_recursive_abi_before_recursive_serialization() {
    let mut manifest = rust_manifest(unit_program());
    let mut ty = AbiType::Unit {};
    for _ in 0..200 {
        ty = AbiType::Optional { item: Box::new(ty) };
    }
    manifest.rust.as_mut().unwrap().abi.result = ty;
    for limit in [8, 96, usize::MAX] {
        let limits = Limits {
            max_json_depth: limit,
            ..Limits::default()
        };
        let error = encode_manifest(&manifest, &limits).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ResourceLimit);
        assert!(error.to_string().contains("depth"));
    }
    let mut manifest = rust_manifest(unit_program());
    manifest.rust.as_mut().unwrap().abi.result = AbiType::Tuple {
        items: vec![AbiType::Unit {}; 128],
    };
    let limits = Limits {
        max_json_nodes: 64,
        ..Limits::default()
    };
    assert_eq!(
        encode_manifest(&manifest, &limits).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
}

#[test]
fn typed_writer_charges_text_bytes_before_scanning_invalid_or_excessive_strings() {
    let mut manifest = rust_manifest(unit_program());
    manifest
        .rust
        .as_mut()
        .unwrap()
        .abi
        .parameters
        .push(AbiParameter {
            name: "\0".repeat(2048),
            ty: AbiType::Unit {},
        });
    let limits = Limits {
        max_envelope_bytes: 1024,
        ..Limits::default()
    };
    // Aggregate text exceeds this limit before scanning for NUL or serializing
    // the public host value; with space available NUL has its own error kind.
    assert_eq!(
        encode_manifest(&manifest, &limits).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    assert_eq!(
        encode_manifest(&manifest, &Limits::default())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidManifest
    );
}

#[test]
fn all_fieldless_abi_variants_reject_unknown_fields_directly() {
    for kind in ["atom", "tree", "bool", "unit"] {
        let valid = json!({"kind":kind});
        let parsed: AbiType = serde_json::from_value(valid.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), valid);
        for key in ["security_proof", "future_flag"] {
            let mut invalid = valid.clone();
            invalid[key] = json!(true);
            assert!(
                serde_json::from_value::<AbiType>(invalid).is_err(),
                "{kind}: {key}"
            );
        }
        let escaped = format!(r#"{{"kind":"{kind}","\u0074rusted":true}}"#);
        assert!(
            serde_json::from_str::<AbiType>(&escaped).is_err(),
            "{kind}: escaped unknown key"
        );
    }
}

#[test]
fn verified_package_debug_is_compact_and_omits_raw_envelopes_and_attachments() {
    let mut manifest = rust_manifest(unit_program());
    let source = "distinctive_source_payload_for_debug_control".repeat(200);
    manifest.source_scope = SourceScope::DeclaredFilesOnly;
    manifest.sources.push(SourceAttachment {
        path: "src/lib.rs".into(),
        sha256: digest(source.as_bytes()),
        content_hex: hex::encode(source.as_bytes()),
    });
    let package = encode_manifest(&manifest, &Limits::default()).unwrap();
    let debug = format!("{package:?}");
    assert!(
        debug.len() < 4096,
        "debug output must stay useful for assertion failures"
    );
    assert!(debug.contains("VerifiedPackage"));
    assert!(debug.contains("artifact_stats"));
    for omitted in [
        "ir_json",
        "bundle_manifest_json",
        "manifest_path",
        "clvm_hex",
        "constructed-fixture",
        source.as_str(),
    ] {
        assert!(
            !debug.contains(omitted),
            "unexpected raw attachment in Debug"
        );
    }
    assert!(package.bytes().len() > debug.len());
}
