//! Constructed declaration controls; no compiler or execution credit.
use super::*;
use serde_json::{Value, json};

fn receipt() -> SourceReceipt {
    SourceReceipt {
        protocol: RECEIPT_PROTOCOL.into(),
        scope: RECEIPT_SCOPE.into(),
        build: BuildDeclaration {
            package: "fixture".into(),
            package_id: "fixture@0.1.0".into(),
            compiler_sha256: digest(b"compiler"),
            toolchain: "nightly-fixture".into(),
            analysis_target: "fixture-host".into(),
            driver_sha256: digest(b"driver"),
            rue_revision: "a".repeat(40),
            cargo_options: vec!["--locked".into()],
            optimize_lir: true,
        },
        unit: UnitDeclaration {
            crate_name: "fixture".into(),
            analysis_target: "fixture-host".into(),
            features: vec!["strict-cap".into()],
            library_source: "package/src/lib.rs".into(),
        },
        exports: vec![ExportDeclaration {
            entry: "fixture::puzzle".into(),
            ir: "puzzle.ir.json".into(),
            bytes: 2,
            sha256: digest(b"{}"),
        }],
        files: vec![
            FileDeclaration {
                path: "package/Cargo.toml".into(),
                kinds: vec![ObservationKind::LauncherManifest],
                compiler_read_count: 0,
                bytes: 0,
                sha256: digest(b""),
                blob: format!("source-{}.bin", digest(b"")),
            },
            FileDeclaration {
                path: "package/src/lib.rs".into(),
                kinds: vec![ObservationKind::CompilerText],
                compiler_read_count: 1,
                bytes: 3,
                sha256: digest(b"abc"),
                blob: format!("source-{}.bin", digest(b"abc")),
            },
        ],
        snapshots: SnapshotDeclaration {
            package_manifest: "package/Cargo.toml".into(),
            workspace_manifest: "package/Cargo.toml".into(),
            lockfile: None,
            build_script: None,
        },
        omitted: OmittedReads {
            outside_roots_reads: 0,
            outside_roots_bytes: 0,
        },
    }
}

#[test]
fn closed_wire_requires_every_nested_field_and_rejects_unknown_kinds() {
    let good = serde_json::to_value(receipt()).unwrap();
    let decoded: SourceReceipt = serde_json::from_value(good.clone()).unwrap();
    decoded.validate(&Limits::default()).unwrap();
    for pointer in [
        "",
        "/build",
        "/unit",
        "/exports/0",
        "/files/0",
        "/snapshots",
        "/omitted",
    ] {
        let mut bad = good.clone();
        bad.pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("authenticated".into(), json!(true));
        assert!(
            serde_json::from_value::<SourceReceipt>(bad).is_err(),
            "{pointer}"
        );
    }
    for field in ["lockfile", "build_script"] {
        let mut bad = good.clone();
        bad["snapshots"].as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<SourceReceipt>(bad).is_err(),
            "missing nullable {field}"
        );
    }
    for value in [
        json!("authenticated_source"),
        json!({"compiler_text":{}}),
        json!(true),
    ] {
        let mut bad = good.clone();
        bad["files"][1]["kinds"][0] = value;
        assert!(serde_json::from_value::<SourceReceipt>(bad).is_err());
    }
    for pointer in [
        "/exports/0/bytes",
        "/files/1/bytes",
        "/files/1/compiler_read_count",
        "/omitted/outside_roots_reads",
    ] {
        let mut bad = good.clone();
        *bad.pointer_mut(pointer).unwrap() = json!(true);
        assert!(serde_json::from_value::<SourceReceipt>(bad).is_err());
    }
}

#[test]
fn every_observation_kind_requires_a_json_string_in_direct_and_nested_reads() {
    for (name, kind) in [
        ("compiler_binary", ObservationKind::CompilerBinary),
        ("compiler_text", ObservationKind::CompilerText),
        (
            "launcher_build_script_source",
            ObservationKind::LauncherBuildScriptSource,
        ),
        (
            "launcher_lockfile_post_cargo",
            ObservationKind::LauncherLockfilePostCargo,
        ),
        ("launcher_manifest", ObservationKind::LauncherManifest),
    ] {
        let bytes = serde_json::to_vec(name).unwrap();
        assert_eq!(
            serde_json::from_slice::<ObservationKind>(&bytes).unwrap(),
            kind
        );
        assert_eq!(serde_json::to_vec(&kind).unwrap(), bytes);
        for alternate in [
            json!({name:null}),
            json!({name:{}}),
            json!([name]),
            Value::Null,
            json!(true),
            json!(1),
        ] {
            assert!(
                serde_json::from_value::<ObservationKind>(alternate.clone()).is_err(),
                "{name}: {alternate}"
            );
            assert!(
                serde_json::from_slice::<ObservationKind>(&serde_json::to_vec(&alternate).unwrap())
                    .is_err()
            );
            let mut nested = serde_json::to_value(receipt()).unwrap();
            nested["files"][1]["kinds"][0] = alternate;
            assert!(
                serde_json::from_value::<SourceReceipt>(nested).is_err(),
                "nested {name}"
            );
        }
    }
}

#[test]
fn scope_paths_sorting_snapshot_references_and_read_counts_are_closed() {
    let good = serde_json::to_value(receipt()).unwrap();
    let changes: [(&str, Value); 15] = [
        ("/protocol", json!("other")),
        ("/scope", json!("complete_build")),
        ("/unit/features", json!(["z", "a"])),
        ("/unit/features", json!(["a", "a"])),
        ("/unit/library_source", json!("missing")),
        ("/unit/analysis_target", json!("other")),
        ("/files/1/path", json!("../outside")),
        ("/files/1/path", json!("workspace/private-key")),
        ("/files/1/path", json!("entry/build-script.rs")),
        ("/files/1/blob", json!("puzzle.ir.json")),
        ("/files/1/compiler_read_count", json!(0)),
        ("/files/0/compiler_read_count", json!(1)),
        ("/files/1/kinds", json!(["compiler_text", "compiler_text"])),
        ("/snapshots/build_script", json!("package/src/lib.rs")),
        ("/snapshots/package_manifest", json!("package/src/lib.rs")),
    ];
    for (pointer, value) in changes {
        let mut bad = good.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        let bad: SourceReceipt = serde_json::from_value(bad).unwrap();
        assert!(bad.validate(&Limits::default()).is_err(), "{pointer}");
    }
    let mut reordered = receipt();
    reordered.files.reverse();
    assert!(reordered.validate(&Limits::default()).is_err());
    let mut unreferenced = receipt();
    unreferenced.files[1]
        .kinds
        .push(ObservationKind::LauncherManifest);
    assert!(unreferenced.validate(&Limits::default()).is_err());
}

#[test]
fn expanded_source_and_repeated_read_budgets_charge_before_contents() {
    let mut value = receipt();
    let mut limits = Limits {
        max_total_source_bytes: 3,
        max_source_read_bytes: 3,
        max_source_read_events: 1,
        ..Limits::default()
    };
    value.validate(&limits).unwrap();
    for changed in ["bytes", "events", "file", "files", "work"] {
        let mut limits = limits.clone();
        match changed {
            "bytes" => limits.max_total_source_bytes = 2,
            "events" => limits.max_source_read_events = 0,
            "file" => limits.max_source_bytes = 2,
            "files" => limits.max_sources = 1,
            "work" => limits.max_source_read_bytes = 2,
            _ => unreachable!(),
        }
        assert_eq!(
            value.validate(&limits).unwrap_err().kind(),
            ErrorKind::ResourceLimit
        );
    }
    value.files[1].compiler_read_count = 2;
    limits.max_source_read_events = 2;
    assert_eq!(
        value.validate(&limits).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    limits.max_source_read_bytes = 6;
    value.validate(&limits).unwrap();
    value.files[1].compiler_read_count = u64::MAX;
    assert_eq!(
        value.validate(&limits).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    let mut duplicate_content = receipt();
    let mut file = duplicate_content.files[1].clone();
    file.path = "package/src/second.rs".into();
    duplicate_content.files.push(file);
    assert_eq!(
        duplicate_content
            .validate(&Limits {
                max_total_source_bytes: 5,
                ..Limits::default()
            })
            .unwrap_err()
            .kind(),
        ErrorKind::ResourceLimit
    );
    let mut omitted = receipt();
    omitted.omitted = OmittedReads {
        outside_roots_reads: 1,
        outside_roots_bytes: u64::MAX,
    };
    // Omitted byte totals do not cause allocation, hashing or iteration. Only
    // retained/repeated contents are charged to that separate work budget.
    omitted.validate(&Limits::default()).unwrap();
    omitted.omitted.outside_roots_reads = 0;
    assert_eq!(
        omitted.validate(&Limits::default()).unwrap_err().kind(),
        ErrorKind::InvalidManifest
    );
}

#[test]
fn feature_declarations_have_independent_count_and_aggregate_byte_bounds() {
    let mut value = receipt();
    value.unit.features = (0..128)
        .map(|index| format!("{index:03}{}", "a".repeat(509)))
        .collect();
    value.validate(&Limits::default()).unwrap();
    value.unit.features.last_mut().unwrap().push('a');
    assert_eq!(
        value.validate(&Limits::default()).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    value.unit.features = (0..129)
        .map(|index| format!("feature-{index:03}"))
        .collect();
    assert_eq!(
        value.validate(&Limits::default()).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    value.unit.features = vec!["x".into()];
    value
        .validate(&Limits {
            max_objects: 1,
            ..Limits::default()
        })
        .unwrap();
}
