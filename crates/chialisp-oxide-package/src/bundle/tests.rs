//! Constructed offline transport controls, not compiler or VM observations.
use super::*;
use crate::receipt::{
    BuildDeclaration, ExportDeclaration, FileDeclaration, ObservationKind, OmittedReads,
    RECEIPT_PROTOCOL, RECEIPT_SCOPE, SnapshotDeclaration, SourceReceipt, UnitDeclaration,
};
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
    fn with_receipt(mut self) -> Self {
        let data: &[(&str, &[u8], ObservationKind)] = &[
            (
                "package/Cargo.toml",
                b"[package]\r\n",
                ObservationKind::LauncherManifest,
            ),
            (
                "package/src/lib.rs",
                b"\xef\xbb\xbfpub fn puzzle() {}\r\n",
                ObservationKind::CompilerText,
            ),
            (
                "generated/binary.bin",
                b"\x00\xff",
                ObservationKind::CompilerBinary,
            ),
            ("generated/empty.bin", b"", ObservationKind::CompilerBinary),
            (
                "workspace/Cargo.lock",
                b"# post-Cargo declaration\n",
                ObservationKind::LauncherLockfilePostCargo,
            ),
        ];
        let mut files: Vec<_> = data
            .iter()
            .map(|(path, bytes, kind)| {
                let sha256 = digest(bytes);
                let blob = format!("source-{sha256}.bin");
                std::fs::write(self.root.join(&blob), bytes).unwrap();
                FileDeclaration {
                    path: (*path).into(),
                    kinds: vec![*kind],
                    compiler_read_count: u64::from(matches!(
                        kind,
                        ObservationKind::CompilerText | ObservationKind::CompilerBinary
                    )),
                    bytes: bytes.len() as u64,
                    sha256,
                    blob,
                }
            })
            .collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let exports = self.manifest["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let ir = row["ir"].as_str().unwrap();
                let bytes = std::fs::read(self.root.join(ir)).unwrap();
                ExportDeclaration {
                    entry: row["entry"].as_str().unwrap().into(),
                    ir: ir.into(),
                    bytes: bytes.len() as u64,
                    sha256: digest(&bytes),
                }
            })
            .collect();
        let build: BuildDeclaration = serde_json::from_value(json!({
            "package":self.manifest["package"],"package_id":self.manifest["package_id"],
            "compiler_sha256":digest(self.manifest["compiler"].as_str().unwrap().as_bytes()),
            "toolchain":self.manifest["toolchain"],"analysis_target":self.manifest["analysis_target"],
            "driver_sha256":self.manifest["driver_sha256"],"rue_revision":self.manifest["rue_revision"],
            "cargo_options":self.manifest["cargo_options"],"optimize_lir":self.manifest["entries"][0]["artifact"]["optimize_lir"]
        })).unwrap();
        let receipt = SourceReceipt {
            protocol: RECEIPT_PROTOCOL.into(),
            scope: RECEIPT_SCOPE.into(),
            build,
            unit: UnitDeclaration {
                crate_name: "fixture".into(),
                analysis_target: "fixture-host".into(),
                features: vec!["fixture".into()],
                library_source: "package/src/lib.rs".into(),
            },
            exports,
            files,
            snapshots: SnapshotDeclaration {
                package_manifest: "package/Cargo.toml".into(),
                workspace_manifest: "package/Cargo.toml".into(),
                lockfile: Some("workspace/Cargo.lock".into()),
                build_script: None,
            },
            omitted: OmittedReads {
                outside_roots_reads: 0,
                outside_roots_bytes: 0,
            },
        };
        self.manifest["schema_version"] = json!(2);
        self.manifest["source_receipt"] = serde_json::to_value(receipt).unwrap();
        self.save();
        self
    }
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
fn v2_admits_complete_sources_and_preserves_exact_raw_bytes_offline() {
    for optimize in [false, true] {
        let fixture = Fixture::new(6, optimize).with_receipt();
        let admitted = admit_bundle(&fixture.path(), &Limits::default()).unwrap();
        assert_eq!(admitted.entries().len(), 2);
        assert_eq!(
            admitted.entries()["fixture::one"].program().name,
            "fixture::one"
        );
        assert_eq!(
            admitted.source_receipt().unwrap().build.optimize_lir,
            optimize
        );
        assert_eq!(admitted.sources().len(), 5);
        let package = import_bundle(&fixture.path(), "fixture::one", &Limits::default()).unwrap();
        assert_eq!(
            package.manifest().source_scope,
            crate::package::SourceScope::DeclaredFilesOnly
        );
        let expected = [
            (
                "package/src/lib.rs",
                b"\xef\xbb\xbfpub fn puzzle() {}\r\n".as_slice(),
            ),
            ("generated/binary.bin", b"\x00\xff".as_slice()),
            ("generated/empty.bin", b"".as_slice()),
        ];
        for (label, bytes) in expected {
            let source = package
                .manifest()
                .sources
                .iter()
                .find(|source| source.path == label)
                .unwrap();
            assert_eq!(source.content_hex, hex::encode(bytes));
            assert_eq!(source.sha256, digest(bytes));
        }
        // The declared host paths do not exist. Envelope replay needs only its
        // retained bytes even after the immutable bundle itself disappears.
        let bytes = package.bytes().to_vec();
        std::fs::remove_dir_all(&fixture.root).unwrap();
        let replay = crate::verify_package(&bytes, &Limits::default()).unwrap();
        assert_eq!(replay.object_id(), package.object_id());
        assert_eq!(replay.manifest().sources.len(), 5);
    }
}

#[test]
fn bare_relative_manifests_use_child_cwd_without_global_state_changes() {
    const CHILD_SCHEMA: &str = "OXIDE_BUNDLE_BARE_MANIFEST_TEST_SCHEMA";
    if let Some(schema) = std::env::var_os(CHILD_SCHEMA) {
        let source_aware = match schema.to_str() {
            Some("1") => false,
            Some("2") => true,
            other => panic!("unexpected bare-manifest test schema: {other:?}"),
        };
        let bare = admit_bundle(Path::new("manifest.json"), &Limits::default()).unwrap();
        let dotted = admit_bundle(Path::new("./manifest.json"), &Limits::default()).unwrap();
        assert_eq!(bare.entries().len(), 2);
        assert_eq!(bare.manifest_json(), dotted.manifest_json());
        assert_eq!(bare.source_receipt().is_some(), source_aware);
        let bare = import_bundle(
            Path::new("manifest.json"),
            "fixture::one",
            &Limits::default(),
        )
        .unwrap();
        let dotted = import_bundle(
            Path::new("./manifest.json"),
            "fixture::one",
            &Limits::default(),
        )
        .unwrap();
        assert_eq!(bare.object_id(), dotted.object_id());
        println!("BARE_MANIFEST_OFFLINE_ADMITTED");
        return;
    }
    for source_aware in [false, true] {
        let fixture = Fixture::new(1, true);
        let fixture = if source_aware {
            fixture.with_receipt()
        } else {
            fixture
        };
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "bundle::tests::bare_relative_manifests_use_child_cwd_without_global_state_changes",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD_SCHEMA, if source_aware { "2" } else { "1" })
            .current_dir(&fixture.root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "schema {} child failed:\n{}\n{}",
            if source_aware { 2 } else { 1 },
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("BARE_MANIFEST_OFFLINE_ADMITTED"),
            "the exact child regression did not run"
        );
    }
}

#[test]
fn v2_declared_ir_length_caps_the_file_before_content_admission() {
    let mut fixture = Fixture::new(1, true).with_receipt();
    fixture.manifest["source_receipt"]["exports"][0]["bytes"] = json!(1);
    fixture.save();
    // A sparse file reaches the ordinary per-IR ceiling without allocating its
    // content in the fixture. The receipt's smaller cap must fail at file
    // metadata, rather than copy all bytes and report a later digest mismatch.
    std::fs::OpenOptions::new()
        .write(true)
        .open(fixture.root.join("one.ir.json"))
        .unwrap()
        .set_len(Limits::default().max_ir_bytes as u64)
        .unwrap();
    let error = admit_bundle(&fixture.path(), &Limits::default())
        .err()
        .unwrap();
    assert_eq!(error.kind(), ErrorKind::ResourceLimit);
    assert!(error.to_string().contains("local file exceeds byte limit"));

    // Schema 1 has no declared IR length: its existing per-file limit remains
    // the only per-IR read cap, even with source sidecars beside the manifest.
    let mut legacy = Fixture::new(1, true).with_receipt();
    legacy.manifest["source_receipt"]["exports"][0]["bytes"] = json!(1);
    legacy.manifest["schema_version"] = json!(1);
    legacy
        .manifest
        .as_object_mut()
        .unwrap()
        .remove("source_receipt");
    legacy.save();
    admit_bundle(&legacy.path(), &Limits::default()).unwrap();
}

#[test]
fn v2_precharges_all_declared_ir_before_any_export_file_is_opened() {
    let fixture = Fixture::new(1, true).with_receipt();
    let declared: usize = fixture.manifest["source_receipt"]["exports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["bytes"].as_u64().unwrap() as usize)
        .sum();
    let manifest_bytes = std::fs::metadata(fixture.path()).unwrap().len() as usize;
    let limits = Limits {
        max_lookup_bytes: manifest_bytes + declared - 1,
        ..Limits::default()
    };
    std::fs::remove_file(fixture.root.join("one.ir.json")).unwrap();
    let error = admit_bundle(&fixture.path(), &limits).err().unwrap();
    assert_eq!(error.kind(), ErrorKind::ResourceLimit);
    assert!(error.to_string().contains("declared IR aggregate"));
    // If the declaration fits, the same missing file remains an IO failure;
    // the resource error above cannot come from reading the first export.
    assert_eq!(
        admit_bundle(&fixture.path(), &Limits::default())
            .err()
            .unwrap()
            .kind(),
        ErrorKind::Io
    );

    let mut invalid = Fixture::new(1, true).with_receipt();
    invalid.manifest["source_receipt"]["protocol"] = json!("unsupported");
    invalid.save();
    std::fs::remove_file(invalid.root.join("one.ir.json")).unwrap();
    invalid.assert_error(ErrorKind::InvalidManifest);
}

#[test]
fn v2_complete_read_budget_accepts_exact_declared_files_and_rejects_one_less() {
    let fixture = Fixture::new(1, true).with_receipt();
    let total: usize = std::fs::read_dir(&fixture.root)
        .unwrap()
        .map(|entry| entry.unwrap().metadata().unwrap().len() as usize)
        .sum();
    let limits = Limits {
        max_lookup_bytes: total,
        ..Limits::default()
    };
    admit_bundle(&fixture.path(), &limits).unwrap();
    let limits = Limits {
        max_lookup_bytes: total - 1,
        ..limits
    };
    assert_eq!(
        admit_bundle(&fixture.path(), &limits).err().unwrap().kind(),
        ErrorKind::ResourceLimit
    );
}

#[test]
fn v2_checks_build_export_and_unselected_source_before_selection() {
    for (pointer, value) in [
        ("/build/package", json!("other")),
        ("/build/package_id", json!("other")),
        ("/build/compiler_sha256", json!("00".repeat(32))),
        ("/build/toolchain", json!("other")),
        ("/build/analysis_target", json!("other")),
        ("/build/driver_sha256", json!("11".repeat(32))),
        ("/build/rue_revision", json!("b".repeat(40))),
        ("/build/cargo_options", json!([])),
        ("/build/optimize_lir", json!(false)),
        ("/exports/1/ir", json!("wrong.ir.json")),
    ] {
        let mut fixture = Fixture::new(1, true).with_receipt();
        *fixture.manifest["source_receipt"]
            .pointer_mut(pointer)
            .unwrap() = value;
        fixture.save();
        fixture.assert_error(ErrorKind::InvalidManifest);
    }
    let fixture = Fixture::new(1, true).with_receipt();
    let file = fixture.root.join("two.ir.json");
    let mut bytes = std::fs::read(&file).unwrap();
    assert_eq!(bytes.last(), Some(&b'\n'));
    *bytes.last_mut().unwrap() = b' ';
    std::fs::write(file, bytes).unwrap();
    fixture.assert_error(ErrorKind::DigestMismatch);
    let fixture = Fixture::new(1, true).with_receipt();
    let blob = fixture.manifest["source_receipt"]["files"][0]["blob"]
        .as_str()
        .unwrap();
    std::fs::write(fixture.root.join(blob), [1, 2]).unwrap();
    let error = admit_bundle(&fixture.path(), &Limits::default())
        .err()
        .unwrap();
    assert_eq!(error.kind(), ErrorKind::DigestMismatch);
    assert_eq!(
        import_bundle(&fixture.path(), "absent", &Limits::default())
            .err()
            .unwrap()
            .kind(),
        ErrorKind::DigestMismatch
    );
}

#[test]
fn v2_rejects_missing_extra_and_aliased_source_inventory() {
    for mutation in [
        "missing",
        "extra",
        "alias",
        "receipt-null",
        "receipt-missing",
        "v1-with-receipt",
    ] {
        let mut fixture = Fixture::new(1, true).with_receipt();
        match mutation {
            "missing" => {
                let blob = fixture.manifest["source_receipt"]["files"][0]["blob"]
                    .as_str()
                    .unwrap();
                std::fs::remove_file(fixture.root.join(blob)).unwrap();
            }
            "extra" => {
                std::fs::write(
                    fixture.root.join(format!("source-{}.bin", "ff".repeat(32))),
                    [],
                )
                .unwrap();
            }
            "alias" => {
                let blob = fixture.manifest["source_receipt"]["files"][0]["blob"].clone();
                fixture.manifest["entries"][0]["ir"] = blob.clone();
                fixture.manifest["source_receipt"]["exports"][0]["ir"] = blob;
            }
            "receipt-null" => fixture.manifest["source_receipt"] = Value::Null,
            "receipt-missing" => {
                fixture
                    .manifest
                    .as_object_mut()
                    .unwrap()
                    .remove("source_receipt");
            }
            "v1-with-receipt" => fixture.manifest["schema_version"] = json!(1),
            _ => unreachable!(),
        }
        fixture.save();
        fixture.assert_error(ErrorKind::InvalidManifest);
    }
}

#[test]
fn every_source_kind_map_null_fails_complete_bundle_and_retained_envelope_admission() {
    let mut fixture = Fixture::new(1, true).with_receipt();
    let rows = fixture.manifest["source_receipt"]["files"]
        .as_array_mut()
        .unwrap();
    let mut build_script = rows
        .iter()
        .find(|row| row["path"] == "package/src/lib.rs")
        .unwrap()
        .clone();
    build_script["path"] = json!("entry/build-script.rs");
    build_script["kinds"] = json!(["launcher_build_script_source"]);
    build_script["compiler_read_count"] = json!(0);
    rows.push(build_script);
    rows.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    fixture.manifest["source_receipt"]["snapshots"]["build_script"] =
        json!("entry/build-script.rs");
    fixture.save();
    let original = fixture.manifest.clone();
    let package = import_bundle(&fixture.path(), "fixture::one", &Limits::default()).unwrap();
    crate::verify_package(package.bytes(), &Limits::default()).unwrap();
    for kind in [
        "compiler_binary",
        "compiler_text",
        "launcher_build_script_source",
        "launcher_lockfile_post_cargo",
        "launcher_manifest",
    ] {
        fixture.manifest = original.clone();
        let rows = fixture.manifest["source_receipt"]["files"]
            .as_array_mut()
            .unwrap();
        let row = rows
            .iter_mut()
            .find(|row| {
                row["kinds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|value| value == kind)
            })
            .unwrap();
        let kinds = row["kinds"].as_array_mut().unwrap();
        let index = kinds.iter().position(|value| value == kind).unwrap();
        kinds[index] = json!({kind:null});
        fixture.save();
        fixture.assert_error(ErrorKind::InvalidManifest);
        let mut envelope = package.manifest().clone();
        let rust = envelope.rust.as_mut().unwrap();
        rust.bundle_manifest_json = serde_json::to_string(&fixture.manifest).unwrap();
        rust.bundle_manifest_sha256 = digest(rust.bundle_manifest_json.as_bytes());
        assert_eq!(
            crate::verify_package(&serde_json::to_vec(&envelope).unwrap(), &Limits::default())
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidManifest,
            "{kind}"
        );
    }
}

#[test]
fn v2_reserves_private_unit_and_blob_names_without_reserving_ordinary_sidecars() {
    let mut fixture = Fixture::new(1, true).with_receipt();
    std::fs::write(
        fixture.root.join("source-notes.txt"),
        "ordinary unimported sidecar",
    )
    .unwrap();
    let contents = b"{\"policy\":\"user file\"}";
    let blob = format!("source-{}.bin", digest(contents));
    std::fs::write(fixture.root.join(&blob), contents).unwrap();
    let rows = fixture.manifest["source_receipt"]["files"]
        .as_array_mut()
        .unwrap();
    rows.push(
        json!({"path":"package/source-unit.json","kinds":["compiler_binary"],
        "compiler_read_count":1,"bytes":contents.len(),"sha256":digest(contents),"blob":blob}),
    );
    rows.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    fixture.save();
    let admitted = admit_bundle(&fixture.path(), &Limits::default()).unwrap();
    assert_eq!(admitted.sources().len(), 6);
    let original = admitted
        .sources()
        .iter()
        .find(|file| file.path == "package/source-unit.json")
        .unwrap();
    assert_eq!(original.content_hex, hex::encode(contents));
    assert!(
        !admitted
            .sources()
            .iter()
            .any(|file| file.path.contains("source-notes.txt"))
    );
    std::fs::write(fixture.root.join("source-unit.json"), "{}").unwrap();
    fixture.assert_error(ErrorKind::InvalidManifest);

    // The new publication namespace applies only to source-aware schema 2.
    let legacy = Fixture::new(1, true);
    std::fs::write(legacy.root.join("source-unit.json"), "{}").unwrap();
    let admitted = admit_bundle(&legacy.path(), &Limits::default()).unwrap();
    assert!(admitted.source_receipt().is_none());
    assert!(admitted.sources().is_empty());
}

#[test]
fn v2_package_attachment_correspondence_is_exact_but_v1_manual_sources_remain_valid() {
    let fixture = Fixture::new(1, true).with_receipt();
    let package = import_bundle(&fixture.path(), "fixture::one", &Limits::default()).unwrap();
    for mutation in [
        "missing",
        "extra",
        "renamed",
        "rehash-source",
        "not-recorded",
        "selected-ir-receipt",
    ] {
        let mut manifest = package.manifest().clone();
        match mutation {
            "missing" => {
                manifest.sources.pop();
            }
            "extra" => {
                let mut source = manifest.sources[0].clone();
                source.path = "package/extra.rs".into();
                manifest.sources.push(source);
            }
            "renamed" => manifest.sources[0].path = "package/wrong.rs".into(),
            "rehash-source" => {
                manifest.sources[0].content_hex = "01".into();
                manifest.sources[0].sha256 = digest(&[1]);
            }
            "not-recorded" => {
                manifest.sources.clear();
                manifest.source_scope = crate::package::SourceScope::NotRecorded;
            }
            "selected-ir-receipt" => {
                let rust = manifest.rust.as_mut().unwrap();
                let mut retained: Value = serde_json::from_str(&rust.bundle_manifest_json).unwrap();
                retained["source_receipt"]["exports"][0]["sha256"] = json!("00".repeat(32));
                rust.bundle_manifest_json = serde_json::to_string(&retained).unwrap();
                rust.bundle_manifest_sha256 = digest(rust.bundle_manifest_json.as_bytes());
            }
            _ => unreachable!(),
        }
        assert!(
            crate::package::encode_manifest(&manifest, &Limits::default()).is_err(),
            "{mutation}"
        );
    }
    let fixture = Fixture::new(1, true);
    let package = import_bundle(&fixture.path(), "fixture::one", &Limits::default()).unwrap();
    let mut manual = package.manifest().clone();
    manual.source_scope = crate::package::SourceScope::DeclaredFilesOnly;
    manual.sources.push(SourceAttachment {
        path: "handwritten/example.rs".into(),
        sha256: digest(&[0, 255]),
        content_hex: "00ff".into(),
    });
    crate::package::encode_manifest(&manual, &Limits::default()).unwrap();
}

#[test]
fn v2_directory_and_expanded_source_limits_are_independent() {
    let fixture = Fixture::new(1, true).with_receipt();
    let entries = std::fs::read_dir(&fixture.root).unwrap().count();
    let exact = Limits {
        max_bundle_files: entries,
        ..Limits::default()
    };
    admit_bundle(&fixture.path(), &exact).unwrap();
    let fewer = Limits {
        max_bundle_files: entries - 1,
        ..Limits::default()
    };
    assert_eq!(
        admit_bundle(&fixture.path(), &fewer).err().unwrap().kind(),
        ErrorKind::ResourceLimit
    );
    let total: u64 = fixture.manifest["source_receipt"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["bytes"].as_u64().unwrap())
        .sum();
    let exact = Limits {
        max_total_source_bytes: total as usize,
        ..Limits::default()
    };
    admit_bundle(&fixture.path(), &exact).unwrap();
    let fewer = Limits {
        max_total_source_bytes: total as usize - 1,
        ..Limits::default()
    };
    assert_eq!(
        admit_bundle(&fixture.path(), &fewer).err().unwrap().kind(),
        ErrorKind::ResourceLimit
    );
}

#[test]
fn v2_deduplicates_content_blobs_without_merging_logical_files_or_budgets() {
    let mut fixture = Fixture::new(1, true).with_receipt();
    let mut shared = fixture.manifest["source_receipt"]["files"][0].clone();
    shared["path"] = json!("package/shared.bin");
    let rows = fixture.manifest["source_receipt"]["files"]
        .as_array_mut()
        .unwrap();
    rows.push(shared);
    rows.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    fixture.save();
    let admitted = admit_bundle(&fixture.path(), &Limits::default()).unwrap();
    assert_eq!(admitted.sources().len(), 6);
    let binary = admitted
        .sources()
        .iter()
        .find(|row| row.path == "generated/binary.bin")
        .unwrap();
    let shared = admitted
        .sources()
        .iter()
        .find(|row| row.path == "package/shared.bin")
        .unwrap();
    assert_eq!(binary.content_hex, "00ff");
    assert_eq!(shared.content_hex, binary.content_hex);
    assert_eq!(binary.sha256, shared.sha256);
    let total = admitted
        .sources()
        .iter()
        .map(|row| row.content_hex.len() / 2)
        .sum::<usize>();
    let limits = Limits {
        max_total_source_bytes: total - 1,
        ..Limits::default()
    };
    assert_eq!(
        admit_bundle(&fixture.path(), &limits).err().unwrap().kind(),
        ErrorKind::ResourceLimit
    );
}

#[test]
fn v2_exact_external_entry_labels_and_post_cargo_lockfile_are_declarations() {
    let mut fixture = Fixture::new(1, true).with_receipt();
    fixture.manifest["source_receipt"]["unit"]["library_source"] = json!("entry/library.rs");
    let rows = fixture.manifest["source_receipt"]["files"]
        .as_array_mut()
        .unwrap();
    let row = rows
        .iter_mut()
        .find(|row| row["path"] == "package/src/lib.rs")
        .unwrap();
    row["path"] = json!("entry/library.rs");
    let bytes = b"fn main() {}\n";
    let blob = format!("source-{}.bin", digest(bytes));
    std::fs::write(fixture.root.join(&blob), bytes).unwrap();
    rows.push(json!({"path":"entry/build-script.rs","kinds":["launcher_build_script_source"],"compiler_read_count":0,"bytes":bytes.len(),"sha256":digest(bytes),"blob":blob}));
    rows.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    fixture.manifest["source_receipt"]["snapshots"]["build_script"] =
        json!("entry/build-script.rs");
    fixture.save();
    let admitted = admit_bundle(&fixture.path(), &Limits::default()).unwrap();
    let receipt = admitted.source_receipt().unwrap();
    assert_eq!(receipt.unit.library_source, "entry/library.rs");
    assert_eq!(
        receipt.snapshots.lockfile.as_deref(),
        Some("workspace/Cargo.lock")
    );
    assert!(
        receipt
            .files
            .iter()
            .find(|row| row.path == "workspace/Cargo.lock")
            .unwrap()
            .kinds
            .contains(&ObservationKind::LauncherLockfilePostCargo)
    );
    // A coherent new feature declaration changes exact identity; offline
    // admission makes no claim that it independently observed Cargo's cfgs.
    let original = import_bundle(&fixture.path(), "fixture::one", &Limits::default()).unwrap();
    fixture.manifest["source_receipt"]["unit"]["features"] = json!(["other"]);
    fixture.save();
    let other = import_bundle(&fixture.path(), "fixture::one", &Limits::default()).unwrap();
    assert_ne!(original.object_id(), other.object_id());
    assert_eq!(original.artifact().identity(), other.artifact().identity());
}

#[cfg(unix)]
#[test]
fn v2_rejects_source_symlinks_without_opening_their_targets() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new(1, true).with_receipt();
    let blob = fixture.manifest["source_receipt"]["files"][0]["blob"]
        .as_str()
        .unwrap();
    let source = fixture.root.join(blob);
    let target = fixture.root.join("target.bin");
    std::fs::rename(&source, &target).unwrap();
    symlink(&target, &source).unwrap();
    fixture.assert_error(ErrorKind::InvalidPath);
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
                fixture.manifest["schema_version"] = 3.into();
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
    for source_aware in [false, true] {
        let fixture = Fixture::new(1, true);
        let fixture = if source_aware {
            fixture.with_receipt()
        } else {
            fixture
        };
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
