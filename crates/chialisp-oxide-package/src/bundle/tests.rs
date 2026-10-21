//! Constructed offline transport controls, not compiler or VM observations.
use super::*;
use chialisp_oxide_ir::{Environment, Expr, Parameter, Type};
use serde_json::json;
use tempfile::TempDir;

struct Fixture {
    _temporary: TempDir,
    root: PathBuf,
    manifest: Value,
}

fn program(name: &str, version: u32) -> Program {
    let mut result = Program {
        name: name.into(),
        parameters: vec![],
        result: Type::Unit,
        body: Expr::Unit,
        environment: Environment::Strict,
        functions: vec![],
    };
    match version {
        1 => (),
        2 => result.environment = Environment::Ignored,
        3 => {
            result.environment = Environment::Raw;
            result.parameters.push(Parameter {
                name: "environment".into(),
                ty: Type::Tree,
            });
            result.result = Type::Tree;
            result.body = Expr::Arg { index: 0 };
        }
        4 => {
            result.body = Expr::Let {
                local_id: 0,
                ty: Type::Unit,
                value: Box::new(Expr::Unit),
                body: Box::new(Expr::Local { local_id: 0 }),
            }
        }
        5 | 6 => {
            let rest = if version == 6 {
                Type::Optional {
                    item: Box::new(Type::Unit),
                }
            } else {
                Type::Unit
            };
            result.result = Type::Pair {
                first: Box::new(Type::Unit),
                rest: Box::new(rest),
            };
            result.body = Expr::Pair {
                first: Box::new(Expr::Unit),
                rest: Box::new(if version == 6 {
                    Expr::OptionalNone { item: Type::Unit }
                } else {
                    Expr::Unit
                }),
            };
        }
        _ => unreachable!(),
    }
    result
}

impl Fixture {
    fn new(version: u32, optimize: bool) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        // macOS may expose its temporary parent through /var -> /private/var.
        // Admission requires actual canonical parents, not a symlink exception.
        let root = temporary.path().canonicalize().unwrap();
        let entries: Vec<_> = ["one", "two"]
            .into_iter()
            .map(|stem| {
                let entry = format!("fixture::{stem}");
                let ir = format!("{stem}.ir.json");
                let payload =
                    serde_json::to_string_pretty(&program(&entry, version)).unwrap() + " \n";
                std::fs::write(root.join(&ir), payload).unwrap();
                let clvm = format!("{stem}.clvm.hex");
                // A canonical CLVM nil data artifact; no execution/equivalence claim.
                std::fs::write(root.join(&clvm), "80\n").unwrap();
                json!({"entry":entry,"ir":ir,"clvm":clvm,"artifact":{
                    "backend":"rust_mir","optimize_lir":optimize,"bytes":1,
                    "tree_hash":digest(&[1]),"clvm_hex":"80","disassembly":"()"
                }})
            })
            .collect();
        let manifest = json!({
            "schema_version":1,"status":BUNDLE_STATUS,"package":"fixture",
            "package_id":"path+file:///declared/fixture#0.1.0",
            "manifest_path":"/declared/fixture/Cargo.toml","workspace_root":"/declared/fixture",
            "compiler":"rustc 1.93.0-nightly (constructed fixture)\nhost: fixture-host\nrelease: fixture\n",
            "toolchain":"nightly-fixture","analysis_target":"fixture-host",
            "driver_sha256":"00".repeat(32),"rue_revision":"a".repeat(40),
            "cargo_options":["--locked","--features","fixture"],"entries":entries
        });
        let fixture = Self {
            _temporary: temporary,
            root,
            manifest,
        };
        fixture.save();
        fixture
    }
    fn path(&self) -> PathBuf {
        self.root.join("manifest.json")
    }
    fn save(&self) {
        std::fs::write(
            self.path(),
            serde_json::to_vec_pretty(&self.manifest).unwrap(),
        )
        .unwrap();
    }
    fn ir(&self, stem: &str) -> Value {
        serde_json::from_slice(&std::fs::read(self.root.join(format!("{stem}.ir.json"))).unwrap())
            .unwrap()
    }
    fn write_ir(&self, stem: &str, value: &Value) {
        std::fs::write(
            self.root.join(format!("{stem}.ir.json")),
            serde_json::to_vec(value).unwrap(),
        )
        .unwrap();
    }
    fn assert_error(&self, kind: ErrorKind) {
        let error = match import_bundle(&self.path(), "fixture::one", &Limits::default()) {
            Ok(_) => panic!("malformed bundle accepted"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), kind, "{error}");
    }
}

#[test]
fn admits_all_current_ir_profiles_and_keeps_exact_import_bytes() {
    for version in 1..=6 {
        for optimize in [false, true] {
            let fixture = Fixture::new(version, optimize);
            let ir = std::fs::read(fixture.root.join("one.ir.json")).unwrap();
            let manifest = std::fs::read(fixture.path()).unwrap();
            let package =
                import_bundle(&fixture.path(), "fixture::one", &Limits::default()).unwrap();
            let retained = package.manifest().rust.as_ref().unwrap();
            assert_eq!(retained.ir_json.as_bytes(), ir);
            assert_eq!(retained.bundle_manifest_json.as_bytes(), manifest);
            assert_eq!(retained.ir_wire_version, version);
            assert_eq!(retained.ir_sha256, digest(&ir));
            assert_eq!(retained.bundle_manifest_sha256, digest(&manifest));
            assert_eq!(package.module_tree_hash(), digest(&[1]));
            assert_eq!(
                package.manifest().source_scope,
                crate::package::SourceScope::NotRecorded
            );
            assert!(package.manifest().sources.is_empty());
            assert_eq!(
                serde_json::from_str::<Value>(&retained.bundle_manifest_json).unwrap()["entries"]
                    [0]["artifact"]["optimize_lir"],
                optimize
            );
        }
    }
}

#[test]
fn unselected_ir_and_artifacts_are_admitted_before_name_selection() {
    for mutation in [
        "ir-semantics",
        "ir-name",
        "tree-hash",
        "clvm-file",
        "missing-ir",
    ] {
        let mut fixture = Fixture::new(1, true);
        let expected = match mutation {
            "ir-semantics" => {
                let mut ir = fixture.ir("two");
                ir["body"] = json!({"op":"arg","index":9});
                fixture.write_ir("two", &ir);
                ErrorKind::InvalidManifest
            }
            "ir-name" => {
                let mut ir = fixture.ir("two");
                ir["name"] = "fixture::other".into();
                fixture.write_ir("two", &ir);
                ErrorKind::InvalidManifest
            }
            "tree-hash" => {
                fixture.manifest["entries"][1]["artifact"]["tree_hash"] = "00".repeat(32).into();
                fixture.save();
                ErrorKind::DigestMismatch
            }
            "clvm-file" => {
                std::fs::write(fixture.root.join("two.clvm.hex"), "01\n").unwrap();
                ErrorKind::DigestMismatch
            }
            "missing-ir" => {
                std::fs::remove_file(fixture.root.join("two.ir.json")).unwrap();
                ErrorKind::Io
            }
            _ => unreachable!(),
        };
        fixture.assert_error(expected);
        let error = import_bundle(&fixture.path(), "absent", &Limits::default())
            .err()
            .unwrap();
        assert_eq!(
            error.kind(),
            expected,
            "selection bypassed invalid unselected export: {mutation}"
        );
    }
    let fixture = Fixture::new(1, true);
    assert_eq!(
        import_bundle(&fixture.path(), "absent", &Limits::default())
            .err()
            .unwrap()
            .kind(),
        ErrorKind::UnknownObject
    );
}

#[test]
fn rejects_closed_manifest_metadata_identity_and_mode_corruptions() {
    for mutation in [
        "schema",
        "boolean-schema",
        "status",
        "host",
        "duplicate-host",
        "digest",
        "revision",
        "mode",
        "backend",
        "extra-top",
        "extra-entry",
        "extra-artifact",
        "duplicate-entry",
        "shared-ir",
        "shared-clvm",
        "manifest-alias",
    ] {
        let mut fixture = Fixture::new(1, true);
        let expected = match mutation {
            "schema" => {
                fixture.manifest["schema_version"] = 2.into();
                ErrorKind::UnsupportedVersion
            }
            "boolean-schema" => {
                fixture.manifest["schema_version"] = true.into();
                ErrorKind::InvalidManifest
            }
            "status" => {
                fixture.manifest["status"] = "production".into();
                ErrorKind::InvalidManifest
            }
            "host" => {
                fixture.manifest["analysis_target"] = "other-host".into();
                ErrorKind::InvalidManifest
            }
            "duplicate-host" => {
                fixture.manifest["compiler"] =
                    "rustc fixture\nhost: fixture-host\nhost: fixture-host\n".into();
                ErrorKind::InvalidManifest
            }
            "digest" => {
                fixture.manifest["driver_sha256"] = "AA".repeat(32).into();
                ErrorKind::InvalidManifest
            }
            "revision" => {
                fixture.manifest["rue_revision"] = "unpinned".into();
                ErrorKind::InvalidManifest
            }
            "mode" => {
                fixture.manifest["entries"][1]["artifact"]["optimize_lir"] = false.into();
                ErrorKind::InvalidManifest
            }
            "backend" => {
                fixture.manifest["entries"][1]["artifact"]["backend"] = "rue_source".into();
                ErrorKind::InvalidManifest
            }
            "extra-top" => {
                fixture.manifest["verified_source"] = true.into();
                ErrorKind::InvalidManifest
            }
            "extra-entry" => {
                fixture.manifest["entries"][1]["source_authentic"] = true.into();
                ErrorKind::InvalidManifest
            }
            "extra-artifact" => {
                fixture.manifest["entries"][1]["artifact"]["audited"] = true.into();
                ErrorKind::InvalidManifest
            }
            "duplicate-entry" => {
                fixture.manifest["entries"][1]["entry"] = "fixture::one".into();
                ErrorKind::InvalidManifest
            }
            "shared-ir" => {
                fixture.manifest["entries"][1]["ir"] = "one.ir.json".into();
                ErrorKind::InvalidManifest
            }
            "shared-clvm" => {
                fixture.manifest["entries"][1]["clvm"] = "one.clvm.hex".into();
                ErrorKind::InvalidManifest
            }
            "manifest-alias" => {
                fixture.manifest["entries"][1]["ir"] = "manifest.json".into();
                ErrorKind::InvalidManifest
            }
            _ => unreachable!(),
        };
        fixture.save();
        fixture.assert_error(expected);
    }
}

#[test]
fn rejects_duplicate_keys_in_manifest_and_unselected_ir() {
    let fixture = Fixture::new(1, true);
    let bytes = std::fs::read_to_string(fixture.path()).unwrap();
    std::fs::write(
        fixture.path(),
        bytes.replacen("{", "{\"schema_version\":1,", 1),
    )
    .unwrap();
    fixture.assert_error(ErrorKind::InvalidManifest);
    let fixture = Fixture::new(1, true);
    let bytes = std::fs::read_to_string(fixture.root.join("two.ir.json")).unwrap();
    std::fs::write(
        fixture.root.join("two.ir.json"),
        bytes.replacen("{", "{\"name\":\"fixture::two\",", 1),
    )
    .unwrap();
    fixture.assert_error(ErrorKind::InvalidManifest);
    let fixture = Fixture::new(1, true);
    let bytes = std::fs::read_to_string(fixture.path()).unwrap();
    std::fs::write(
        fixture.path(),
        bytes.replacen(
            "\"backend\": \"rust_mir\"",
            "\"backend\":\"rust_mir\",\"\\u0062ackend\":\"rust_mir\"",
            1,
        ),
    )
    .unwrap();
    fixture.assert_error(ErrorKind::InvalidManifest);
}

#[test]
fn rejects_ir_capability_and_environment_corruptions() {
    for mutation in [
        "unknown-version",
        "boolean-version",
        "missing-optional-capability",
        "optional-input",
        "raw-policy",
    ] {
        let fixture = Fixture::new(if mutation == "raw-policy" { 3 } else { 6 }, true);
        let mut ir = fixture.ir("two");
        let expected = match mutation {
            "unknown-version" => {
                ir["ir_version"] = 7.into();
                ErrorKind::UnsupportedVersion
            }
            "boolean-version" => {
                ir["ir_version"] = true.into();
                ErrorKind::InvalidManifest
            }
            "missing-optional-capability" => {
                ir["ir_version"] = 5.into();
                ErrorKind::InvalidManifest
            }
            "optional-input" => {
                ir["program"]["parameters"] =
                    json!([{"name":"input","ty":{"kind":"optional","item":{"kind":"unit"}}}]);
                ErrorKind::InvalidManifest
            }
            "raw-policy" => {
                ir["program"]["parameters"] = json!([]);
                ErrorKind::InvalidManifest
            }
            _ => unreachable!(),
        };
        fixture.write_ir("two", &ir);
        fixture.assert_error(expected);
    }
}

#[test]
fn rejects_traversal_nonregular_inputs_and_moving_pointer() {
    for raw in [
        "",
        "../outside",
        "./two.ir.json",
        "dir//two.ir.json",
        "/absolute",
        "dir\\two.ir.json",
        "two.ir.json/",
    ] {
        let mut fixture = Fixture::new(1, true);
        fixture.manifest["entries"][1]["ir"] = raw.into();
        fixture.save();
        fixture.assert_error(ErrorKind::InvalidPath);
    }
    let fixture = Fixture::new(1, true);
    std::fs::remove_file(fixture.root.join("two.ir.json")).unwrap();
    std::fs::create_dir(fixture.root.join("two.ir.json")).unwrap();
    fixture.assert_error(ErrorKind::InvalidPath);
    std::fs::write(
        fixture.root.join("latest.json"),
        "{\"manifest\":\"manifest.json\"}",
    )
    .unwrap();
    assert_eq!(
        import_bundle(
            &fixture.root.join("latest.json"),
            "fixture::one",
            &Limits::default()
        )
        .err()
        .unwrap()
        .kind(),
        ErrorKind::InvalidPath
    );
}

#[cfg(unix)]
#[test]
fn rejects_confined_symlinks_and_symlinked_bundle_parents() {
    let fixture = Fixture::new(1, true);
    std::fs::rename(
        fixture.root.join("two.ir.json"),
        fixture.root.join("actual.ir.json"),
    )
    .unwrap();
    std::os::unix::fs::symlink("actual.ir.json", fixture.root.join("two.ir.json")).unwrap();
    fixture.assert_error(ErrorKind::InvalidPath);
    let parent = tempfile::tempdir().unwrap();
    let link = parent.path().canonicalize().unwrap().join("bundle");
    std::os::unix::fs::symlink(&fixture.root, &link).unwrap();
    assert_eq!(
        import_bundle(
            &link.join("manifest.json"),
            "fixture::one",
            &Limits::default()
        )
        .err()
        .unwrap()
        .kind(),
        ErrorKind::InvalidPath
    );
}

#[test]
fn charges_export_file_json_blob_and_complete_bundle_budgets() {
    let fixture = Fixture::new(6, true);
    for limits in [
        Limits {
            max_objects: 1,
            ..Limits::default()
        },
        Limits {
            max_envelope_bytes: 1,
            ..Limits::default()
        },
        Limits {
            max_ir_bytes: 1,
            ..Limits::default()
        },
        Limits {
            max_blob_bytes: 0,
            ..Limits::default()
        },
        Limits {
            max_json_nodes: 1,
            ..Limits::default()
        },
        Limits {
            max_json_depth: 1,
            ..Limits::default()
        },
    ] {
        assert_eq!(
            import_bundle(&fixture.path(), "fixture::one", &limits)
                .err()
                .unwrap()
                .kind(),
            ErrorKind::ResourceLimit
        );
    }
    let total: usize = [
        "manifest.json",
        "one.ir.json",
        "two.ir.json",
        "one.clvm.hex",
        "two.clvm.hex",
    ]
    .into_iter()
    .map(|name| std::fs::read(fixture.root.join(name)).unwrap().len())
    .sum();
    let limits = Limits {
        max_lookup_bytes: total,
        ..Limits::default()
    };
    assert!(import_bundle(&fixture.path(), "fixture::one", &limits).is_ok());
    let limits = Limits {
        max_lookup_bytes: total - 1,
        ..limits
    };
    assert_eq!(
        import_bundle(&fixture.path(), "fixture::one", &limits)
            .err()
            .unwrap()
            .kind(),
        ErrorKind::ResourceLimit
    );
}

#[test]
fn offline_selected_receipt_rejects_unrelated_artifact_metadata() {
    let fixture = Fixture::new(1, true);
    let package = import_bundle(&fixture.path(), "fixture::one", &Limits::default()).unwrap();
    let manifest =
        decode_manifest(&std::fs::read(fixture.path()).unwrap(), &Limits::default()).unwrap();
    let mut receipt = package.manifest().artifact.clone();
    check_selected_metadata(&manifest, "fixture::one", &receipt).unwrap();
    receipt.clvm_hex = "01".into();
    assert_eq!(
        check_selected_metadata(&manifest, "fixture::one", &receipt)
            .unwrap_err()
            .kind(),
        ErrorKind::DigestMismatch
    );
    let mut receipt = package.manifest().artifact.clone();
    receipt.identity.blob_sha256 = Digest32::new([0; 32]);
    assert_eq!(
        check_selected_metadata(&manifest, "fixture::one", &receipt)
            .unwrap_err()
            .kind(),
        ErrorKind::DigestMismatch
    );
    assert_eq!(
        check_selected_metadata(&manifest, "absent", &package.manifest().artifact)
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidManifest
    );
}
