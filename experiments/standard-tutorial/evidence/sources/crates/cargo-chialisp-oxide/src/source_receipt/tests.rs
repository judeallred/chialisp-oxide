use super::*;
use serde_json::{Value, json};

struct Fixture {
    _temp: tempfile::TempDir,
    package: PathBuf,
    work: PathBuf,
    exports: Vec<ExportDeclaration>,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let package = root.join("consumer");
        let work = root.join("work");
        std::fs::create_dir_all(package.join("src")).unwrap();
        std::fs::create_dir(&work).unwrap();
        std::fs::write(
            package.join("Cargo.toml"),
            b"[package]\nname='consumer'\n[workspace]\n",
        )
        .unwrap();
        std::fs::write(
            package.join("src/lib.rs"),
            "\u{feff}// raw\r\npub fn value() {}\r\n",
        )
        .unwrap();
        let ir = b"{\"constructed_transport_control\":true}";
        let export = ExportDeclaration {
            entry: "consumer::puzzle".into(),
            ir: format!("{}.ir.json", digest(b"consumer::puzzle")),
            bytes: ir.len() as u64,
            sha256: digest(ir),
        };
        std::fs::write(work.join(&export.ir), ir).unwrap();
        Self {
            _temp: temp,
            package,
            work,
            exports: vec![export],
        }
    }

    fn plan(&self) -> CapturePlan {
        CapturePlan::begin(
            &self.package.join("Cargo.toml"),
            &self.package,
            &self.package.join("src/lib.rs"),
            None,
            "consumer",
        )
        .unwrap()
    }

    fn row(&self, path: &str, bytes: &[u8], kinds: &[&str], count: u64) -> Value {
        let sha256 = digest(bytes);
        let blob = format!("source-{sha256}.bin");
        std::fs::write(self.work.join(&blob), bytes).unwrap();
        json!({"path":path,"read_kinds":kinds,"read_count":count,"bytes":bytes.len(),"sha256":sha256,"blob":blob})
    }

    fn unit(&self) -> Value {
        let library = std::fs::read(self.package.join("src/lib.rs")).unwrap();
        json!({
            "schema_version":1,"protocol":"chialisp-oxide-driver-source-unit/v1",
            "scope":"selected_unit_file_loader_reads_only",
            "unit":{"crate_name":"consumer","analysis_target":"test-host","features":["strict"],"library_source":"package/src/lib.rs"},
            "exports":self.exports,
            "files":[self.row("package/src/lib.rs", &library, &["text"], 1)],
            "omitted":{"outside_roots_reads":0,"outside_roots_bytes":0},
        })
    }

    fn write_unit(&self, unit: &Value) {
        std::fs::write(self.work.join(UNIT_NAME), serde_json::to_vec(unit).unwrap()).unwrap();
    }

    fn finish(&self, plan: CapturePlan) -> Result<SourceReceipt> {
        plan.finish(
            &self.work,
            &BuildContext {
                package: "consumer",
                package_id: "path+test#consumer@0.1.0",
                compiler: "constructed test compiler\n",
                toolchain: "test-nightly",
                analysis_target: "test-host",
                driver_sha256: "1111111111111111111111111111111111111111111111111111111111111111",
                rue_revision: "2222222222222222222222222222222222222222",
                cargo_options: &[],
                optimize_lir: true,
            },
            &self.exports,
        )
    }
}

#[test]
fn retains_raw_bytes_separate_observation_origins_and_post_cargo_lockfile() {
    let fixture = Fixture::new();
    let plan = fixture.plan();
    let mut unit = fixture.unit();
    unit["files"].as_array_mut().unwrap().insert(
        0,
        fixture.row("package/input.bin", &[0xff, 0, 0x80], &["binary"], 1),
    );
    // Outside data is not iterated/copied or charged as attached read work.
    unit["omitted"]["outside_roots_reads"] = 1.into();
    unit["omitted"]["outside_roots_bytes"] = u64::MAX.into();
    fixture.write_unit(&unit);
    std::fs::write(
        fixture.package.join("Cargo.lock"),
        b"# created by successful Cargo\r\n",
    )
    .unwrap();
    let receipt = fixture.finish(plan).unwrap();
    assert_eq!(
        receipt.snapshots.package_manifest,
        receipt.snapshots.workspace_manifest
    );
    assert_eq!(
        receipt.snapshots.lockfile.as_deref(),
        Some("package/Cargo.lock")
    );
    assert_eq!(receipt.unit.features, ["strict"]);
    assert_eq!(receipt.omitted.outside_roots_bytes, u64::MAX);
    assert!(!fixture.work.join(UNIT_NAME).exists());
    let binary = receipt
        .files
        .iter()
        .find(|row| row.path == "package/input.bin")
        .unwrap();
    assert_eq!(
        std::fs::read(fixture.work.join(&binary.blob)).unwrap(),
        [0xff, 0, 0x80]
    );
    let library = receipt
        .files
        .iter()
        .find(|row| row.path == "package/src/lib.rs")
        .unwrap();
    assert_eq!(
        std::fs::read(fixture.work.join(&library.blob)).unwrap(),
        std::fs::read(fixture.package.join("src/lib.rs")).unwrap()
    );
    assert_eq!(library.kinds, [ObservationKind::CompilerText]);
    let lock = receipt
        .files
        .iter()
        .find(|row| row.path == "package/Cargo.lock")
        .unwrap();
    assert_eq!(lock.kinds, [ObservationKind::LauncherLockfilePostCargo]);
    assert_eq!(lock.compiler_read_count, 0);
}

#[test]
fn existing_unlocked_lockfile_may_change_while_manifests_must_stay_stable() {
    let fixture = Fixture::new();
    std::fs::write(fixture.package.join("Cargo.lock"), "old").unwrap();
    let plan = fixture.plan();
    fixture.write_unit(&fixture.unit());
    std::fs::write(fixture.package.join("Cargo.lock"), "updated").unwrap();
    let receipt = fixture.finish(plan).unwrap();
    let lock = receipt
        .files
        .iter()
        .find(|row| Some(&row.path) == receipt.snapshots.lockfile.as_ref())
        .unwrap();
    assert_eq!(
        std::fs::read(fixture.work.join(&lock.blob)).unwrap(),
        b"updated"
    );

    let fixture = Fixture::new();
    let plan = fixture.plan();
    fixture.write_unit(&fixture.unit());
    std::fs::write(fixture.package.join("Cargo.toml"), "changed").unwrap();
    assert!(
        fixture
            .finish(plan)
            .unwrap_err()
            .to_string()
            .contains("changed during Cargo")
    );
}

#[test]
fn exact_external_roots_get_named_labels_without_parent_capture() {
    let fixture = Fixture::new();
    let external = fixture.work.parent().unwrap().join("external");
    std::fs::create_dir(&external).unwrap();
    std::fs::write(external.join("custom.rs"), "pub fn custom() {}\n").unwrap();
    std::fs::write(external.join("build.rs"), "fn main() {}\n").unwrap();
    let plan = CapturePlan::begin(
        &fixture.package.join("Cargo.toml"),
        &fixture.package,
        &external.join("custom.rs"),
        Some(&external.join("build.rs")),
        "consumer",
    )
    .unwrap();
    let mut unit = fixture.unit();
    // Remove the old fixture-only blob as the actual selected unit never read it.
    let old_blob = unit["files"][0]["blob"].as_str().unwrap().to_owned();
    std::fs::remove_file(fixture.work.join(old_blob)).unwrap();
    unit["unit"]["library_source"] = "entry/library.rs".into();
    unit["files"] = json!([fixture.row("entry/library.rs", b"pub fn custom() {}\n", &["text"], 1)]);
    fixture.write_unit(&unit);
    let receipt = fixture.finish(plan).unwrap();
    assert_eq!(receipt.unit.library_source, "entry/library.rs");
    assert_eq!(
        receipt.snapshots.build_script.as_deref(),
        Some("entry/build-script.rs")
    );
    assert!(
        receipt
            .files
            .iter()
            .all(|file| !file.path.contains("external"))
    );
}

#[test]
fn missing_unit_or_changed_build_script_cannot_become_artifact_only() {
    let fixture = Fixture::new();
    assert!(fixture.finish(fixture.plan()).is_err());

    let script = fixture.package.join("custom-build.rs");
    std::fs::write(&script, "fn main() {}\n").unwrap();
    let plan = CapturePlan::begin(
        &fixture.package.join("Cargo.toml"),
        &fixture.package,
        &fixture.package.join("src/lib.rs"),
        Some(&script),
        "consumer",
    )
    .unwrap();
    fixture.write_unit(&fixture.unit());
    std::fs::write(script, "fn main() { panic!() }\n").unwrap();
    assert!(
        fixture
            .finish(plan)
            .unwrap_err()
            .to_string()
            .contains("changed during Cargo")
    );
}

#[test]
fn rejects_incomplete_or_conflicting_unit_without_downgrade() {
    for mutation in [
        "version",
        "unit",
        "export",
        "ir",
        "library",
        "count",
        "kind",
        "blob",
        "extra-blob",
        "unknown",
        "compiler-manifest-conflict",
    ] {
        let fixture = Fixture::new();
        let plan = fixture.plan();
        let mut unit = fixture.unit();
        match mutation {
            "version" => unit["schema_version"] = 2.into(),
            "unit" => unit["unit"]["crate_name"] = "wrong".into(),
            "export" => unit["exports"].as_array_mut().unwrap().clear(),
            "ir" => unit["exports"][0]["sha256"] = digest(b"different").to_hex().into(),
            "library" => unit["unit"]["library_source"] = "package/missing.rs".into(),
            "count" => unit["files"][0]["read_count"] = 0.into(),
            "kind" => unit["files"][0]["read_kinds"] = json!(["launcher_manifest"]),
            "blob" => {
                let blob = unit["files"][0]["blob"].as_str().unwrap();
                std::fs::write(fixture.work.join(blob), "changed").unwrap();
            }
            "extra-blob" => {
                std::fs::write(
                    fixture
                        .work
                        .join(format!("source-{}.bin", digest(b"extra"))),
                    "extra",
                )
                .unwrap();
            }
            "unknown" => unit["untrusted"] = true.into(),
            "compiler-manifest-conflict" => {
                unit["files"].as_array_mut().unwrap().insert(
                    0,
                    fixture.row("package/Cargo.toml", b"different manifest", &["text"], 1),
                );
            }
            _ => unreachable!(),
        }
        fixture.write_unit(&unit);
        assert!(fixture.finish(plan).is_err(), "accepted {mutation}");
    }
}

#[test]
fn expanded_logical_sources_are_charged_even_with_shared_blobs() {
    let fixture = Fixture::new();
    let mut plan = fixture.plan();
    plan.limits.max_total_source_bytes = 100;
    let mut unit = fixture.unit();
    for path in ["package/a.bin", "package/b.bin"] {
        unit["files"]
            .as_array_mut()
            .unwrap()
            .insert(0, fixture.row(path, &[42; 60], &["binary"], 1));
    }
    unit["files"]
        .as_array_mut()
        .unwrap()
        .sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    fixture.write_unit(&unit);
    assert!(
        fixture
            .finish(plan)
            .unwrap_err()
            .to_string()
            .contains("aggregate")
    );
}

#[test]
fn preflight_rejects_duplicate_keys_deep_or_wide_json_before_typed_parse() {
    assert!(preflight_unit(br#"{"a":1,"a":2}"#).is_err());
    assert!(preflight_unit(b"[[[[[[[[[[0]]]]]]]]]]").is_err());
    assert!(preflight_unit(format!("[{}]", vec!["0"; 4097].join(",")).as_bytes()).is_err());
    assert!(preflight_unit(format!("\"{}\"", "x".repeat(MAX_PATH_BYTES + 1)).as_bytes()).is_err());
}

#[test]
fn publication_uses_only_exact_checked_inventory() {
    let fixture = Fixture::new();
    let plan = fixture.plan();
    fixture.write_unit(&fixture.unit());
    let receipt = fixture.finish(plan).unwrap();
    let expected = publication_files(&fixture.exports, Some(&receipt)).unwrap();
    for name in &expected {
        if !fixture.work.join(name).exists() {
            std::fs::write(fixture.work.join(name), "presentation fixture").unwrap();
        }
    }
    check_publication_files(&fixture.work, &expected).unwrap();
    std::fs::write(fixture.work.join("undeclared.txt"), "not publishable").unwrap();
    assert!(check_publication_files(&fixture.work, &expected).is_err());
    std::fs::remove_file(fixture.work.join("undeclared.txt")).unwrap();
    std::fs::remove_file(fixture.work.join("manifest.json")).unwrap();
    assert!(check_publication_files(&fixture.work, &expected).is_err());
}

#[test]
fn ir_aggregate_remaining_budget_caps_each_read_and_preserves_failed_accounting() {
    let fixture = Fixture::new();
    let first = fixture.work.join("first.ir.json");
    let second = fixture.work.join("second.ir.json");
    std::fs::write(&first, b"1234").unwrap();
    // A sparse regular file has no input allocation proportional to its size.
    // Its metadata must reject it against the remaining one-byte budget.
    std::fs::File::create(&second)
        .unwrap()
        .set_len(1024 * 1024)
        .unwrap();
    let limits = Limits {
        max_lookup_bytes: 5,
        ..Limits::default()
    };
    let mut total = 0;
    assert_eq!(
        read_ir_bounded(&first, &mut total, &limits).unwrap(),
        b"1234"
    );
    assert_eq!(total, 4);
    let error = read_ir_bounded(&second, &mut total, &limits).unwrap_err();
    assert_eq!(
        error.downcast_ref::<Error>().unwrap().kind(),
        ErrorKind::ResourceLimit
    );
    assert_eq!(total, 4);
    total = 6;
    let error = read_ir_bounded(
        &fixture.work.join("does-not-exist.ir.json"),
        &mut total,
        &limits,
    )
    .unwrap_err();
    assert_eq!(
        error.downcast_ref::<Error>().unwrap().kind(),
        ErrorKind::ResourceLimit
    );
    assert_eq!(total, 6);
}

#[test]
fn source_blob_declared_length_caps_sparse_actual_file_before_read_or_hash() {
    let fixture = Fixture::new();
    let plan = fixture.plan();
    let mut unit = fixture.unit();
    unit["files"][0]["bytes"] = 1.into();
    let blob = unit["files"][0]["blob"].as_str().unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(fixture.work.join(blob))
        .unwrap()
        .set_len(1024 * 1024)
        .unwrap();
    fixture.write_unit(&unit);
    let error = fixture.finish(plan).unwrap_err();
    assert_eq!(
        error.downcast_ref::<Error>().unwrap().kind(),
        ErrorKind::ResourceLimit
    );
}

#[test]
fn distinct_logical_labels_share_an_exact_checked_source_blob() {
    let fixture = Fixture::new();
    let plan = fixture.plan();
    let mut unit = fixture.unit();
    for label in ["package/a.bin", "package/b.bin"] {
        unit["files"].as_array_mut().unwrap().push(fixture.row(
            label,
            &[0, 0xff, 0x80],
            &["binary"],
            1,
        ));
    }
    unit["files"]
        .as_array_mut()
        .unwrap()
        .sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    fixture.write_unit(&unit);
    let receipt = fixture.finish(plan).unwrap();
    let rows: Vec<_> = receipt
        .files
        .iter()
        .filter(|row| matches!(row.path.as_str(), "package/a.bin" | "package/b.bin"))
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].blob, rows[1].blob);
    assert_eq!(rows[0].bytes, 3);
    assert_eq!(rows[1].bytes, 3);
    assert_eq!(
        std::fs::read(fixture.work.join(&rows[0].blob)).unwrap(),
        [0, 0xff, 0x80]
    );
}

#[test]
fn declared_ir_snapshot_and_lockfile_caps_precede_sparse_rereads() {
    for mutation in ["ir", "snapshot", "lockfile"] {
        let fixture = Fixture::new();
        let mut plan = fixture.plan();
        fixture.write_unit(&fixture.unit());
        let oversized = match mutation {
            "ir" => fixture.work.join(&fixture.exports[0].ir),
            "snapshot" => fixture.package.join("Cargo.toml"),
            "lockfile" => {
                plan.limits.max_total_source_bytes =
                    std::fs::metadata(fixture.package.join("src/lib.rs"))
                        .unwrap()
                        .len() as usize
                        + std::fs::metadata(fixture.package.join("Cargo.toml"))
                            .unwrap()
                            .len() as usize
                        + 1;
                let lock = fixture.package.join("Cargo.lock");
                std::fs::File::create(&lock).unwrap();
                lock
            }
            _ => unreachable!(),
        };
        std::fs::OpenOptions::new()
            .write(true)
            .open(oversized)
            .unwrap()
            .set_len(1024 * 1024)
            .unwrap();
        let error = fixture.finish(plan).unwrap_err();
        assert_eq!(
            error.downcast_ref::<Error>().unwrap().kind(),
            ErrorKind::ResourceLimit,
            "{mutation}: {error:#}"
        );
    }
}
