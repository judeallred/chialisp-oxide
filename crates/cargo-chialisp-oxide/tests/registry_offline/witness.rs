//! Actual prebuilt offline witness-source generation, never compilation or VM.
use super::*;
#[cfg(unix)]
use chialisp_oxide_package::digest;

#[cfg(unix)]
fn arguments(output: &Path, check: bool) -> Vec<std::ffi::OsString> {
    let mut result = vec![
        "registry".into(),
        "generate-witness-rust".into(),
        "--lock".into(),
        "pins.json".into(),
        "--registry".into(),
        "local".into(),
        "--name".into(),
        "program".into(),
        "--output".into(),
        output.as_os_str().to_owned(),
        "--json".into(),
    ];
    if check {
        result.push("--check".into());
    }
    result
}

#[cfg(unix)]
fn stages(root: &Path) -> Vec<std::path::PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .as_encoded_bytes()
                .windows(7)
                .any(|bytes| bytes == b".stage-")
        })
        .collect()
}

#[cfg(unix)]
fn prepare(root: &Path, source: &Value) -> String {
    let id = seed(root, "input.json", source, false);
    let module = source["artifact"]["identity"]["module_tree_hash"]
        .as_str()
        .unwrap();
    success(&add(root, "program", module, Some(&id), "pins.json"));
    id
}

#[cfg(unix)]
fn quoted_atom(length: usize) -> Value {
    let payload = vec![0xa5; length];
    let mut bytes = vec![0xff, 1];
    bytes.extend_from_slice(match length {
        257 => &[0xc1, 1],
        8192 => &[0xe0, 0x20, 0],
        _ => unreachable!(),
    });
    bytes.extend_from_slice(&payload);
    let mut atom = vec![1];
    atom.extend_from_slice(&payload);
    let mut pair = vec![2];
    pair.extend_from_slice(digest(&[1, 1]).as_bytes());
    pair.extend_from_slice(digest(&atom).as_bytes());
    let mut source = package();
    source["artifact"] = json!({"identity":{"encoding":"clvm-legacy-plain/v1","bytes":bytes.len(),
        "blob_sha256":digest(&bytes),"module_tree_hash":digest(&pair)},"clvm_hex":hex::encode(bytes)});
    source
}

#[cfg(unix)]
#[test]
fn actual_large_witness_generation_is_explicit_exact_and_never_a_static_fallback() {
    for (length, prefix) in [(257, false), (8192, true)] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let source = quoted_atom(length);
        let object = prepare(root, &source);
        let lock = fs::read(root.join("pins.json")).unwrap();
        let object_path = root.join("local/objects").join(format!("{object}.json"));
        let object_bytes = fs::read(&object_path).unwrap();
        fs::write(root.join("Cargo.toml"), "not a Cargo manifest").unwrap();
        fs::write(root.join("Cargo.lock"), "protected Cargo bytes").unwrap();
        let mut args = arguments(Path::new("witness.rs"), false);
        if prefix {
            args.insert(0, "chialisp-oxide".into());
        }
        let first = success(&command_os(&args, root));
        assert_eq!(first.as_object().unwrap().len(), 3);
        assert_eq!(first["status"], "created");
        assert_eq!(first["output"]["status"], "created");
        let generated = &first["generation"];
        assert_eq!(generated["schema_version"], 1);
        assert_eq!(generated["recipe"], "chialisp-oxide-witness-rust/v1");
        assert_eq!(generated["lock_sha256"], digest(&lock).to_string());
        assert_eq!(generated["selected_alias"], "program");
        assert_eq!(generated["object_id"], object);
        assert_eq!(generated["artifact"], source["artifact"]["identity"]);
        assert_eq!(generated["limits"]["max_source_bytes"], 8192);
        assert_eq!(generated["metrics"]["commitment_bytes"], 32);
        assert_eq!(generated["metrics"]["embedded_program_bytes"], 0);
        assert_eq!(generated["metrics"]["source_helpers"], 2);
        let bytes = fs::read(root.join("witness.rs")).unwrap();
        assert!(bytes.len() <= 8192);
        assert_eq!(generated["source_bytes"], bytes.len());
        assert_eq!(generated["metrics"]["generated_source_bytes"], bytes.len());
        assert_eq!(generated["source_sha256"], digest(&bytes).to_string());
        assert_eq!(first["output"]["source_sha256"], generated["source_sha256"]);
        assert_eq!(first["output"]["source_bytes"], generated["source_bytes"]);
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(
            text.contains(
                "pub(super) fn run<'a>(program: &'a Tree<'a>, environment: &'a Tree<'a>)"
            )
        );
        assert!(text.contains("assert!(program_hash == EXPECTED_PROGRAM_TREE_HASH)"));
        assert!(!text.contains(&"0xa5, ".repeat(length)));
        assert!(!text.contains("#[chialisp_oxide::puzzle"));
        let repeated = success(&command_os(&args, root));
        assert_eq!(repeated["status"], "already_present");
        assert_eq!(repeated["generation"], *generated);
        args.push("--check".into());
        let checked = success(&command_os(&args, root));
        assert_eq!(checked["status"], "checked");
        assert_eq!(checked["generation"], *generated);
        failure(
            &command(
                &[
                    "registry",
                    "generate-rust",
                    "--lock",
                    "pins.json",
                    "--registry",
                    "local",
                    "--name",
                    "program",
                    "--output",
                    "static.rs",
                    "--json",
                ],
                root,
            ),
            "resource_limit",
        );
        assert!(!root.join("static.rs").exists());
        assert_eq!(fs::read(root.join("witness.rs")).unwrap(), bytes);
        assert_eq!(fs::read(root.join("pins.json")).unwrap(), lock);
        assert_eq!(fs::read(object_path).unwrap(), object_bytes);
        assert_eq!(
            fs::read_to_string(root.join("Cargo.toml")).unwrap(),
            "not a Cargo manifest"
        );
        assert_eq!(
            fs::read_to_string(root.join("Cargo.lock")).unwrap(),
            "protected Cargo bytes"
        );
        assert!(stages(root).is_empty());
    }
}

#[cfg(unix)]
#[test]
fn witness_source_edits_and_changed_raw_lock_never_overwrite_existing_output() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    prepare(root, &package());
    failure(
        &command_os(&arguments(Path::new("witness.rs"), true), root),
        "conflict",
    );
    success(&command_os(
        &arguments(Path::new("witness.rs"), false),
        root,
    ));
    let original = fs::read(root.join("witness.rs")).unwrap();
    let mut edited = original.clone();
    edited.extend_from_slice(b"\n// manual source edit\n");
    fs::write(root.join("witness.rs"), &edited).unwrap();
    for check in [false, true] {
        failure(
            &command_os(&arguments(Path::new("witness.rs"), check), root),
            "conflict",
        );
        assert_eq!(fs::read(root.join("witness.rs")).unwrap(), edited);
    }
    fs::write(root.join("witness.rs"), &original).unwrap();
    let mut changed_lock = fs::read(root.join("pins.json")).unwrap();
    changed_lock.push(b' ');
    fs::write(root.join("pins.json"), &changed_lock).unwrap();
    for check in [false, true] {
        failure(
            &command_os(&arguments(Path::new("witness.rs"), check), root),
            "conflict",
        );
        assert_eq!(fs::read(root.join("witness.rs")).unwrap(), original);
        assert_eq!(fs::read(root.join("pins.json")).unwrap(), changed_lock);
    }
    let fresh = success(&command_os(&arguments(Path::new("fresh.rs"), false), root));
    assert_eq!(
        fresh["generation"]["lock_sha256"],
        digest(&changed_lock).to_string()
    );
    assert_ne!(
        fresh["generation"]["source_sha256"],
        digest(&original).to_string()
    );
    assert!(stages(root).is_empty());
}

#[cfg(unix)]
#[test]
fn witness_generation_resolves_every_pin_and_unrelated_object_before_output_or_check() {
    for fault in [
        "missing_selected",
        "missing_unselected",
        "corrupt_unselected",
        "wrong_unselected_pin",
        "unrelated_corrupt",
        "unknown_alias",
        "empty_lock_corrupt",
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let selected = prepare(root, &package());
        let other = seed(root, "other.json", &package(), true);
        success(&add(root, "other", NIL_MODULE, Some(&other), "pins.json"));
        let mut args = arguments(Path::new("witness.rs"), false);
        let objects = root.join("local/objects");
        let expected = match fault {
            "missing_selected" => {
                fs::remove_file(objects.join(format!("{selected}.json"))).unwrap();
                "unknown_object"
            }
            "missing_unselected" => {
                fs::remove_file(objects.join(format!("{other}.json"))).unwrap();
                "unknown_object"
            }
            "corrupt_unselected" => {
                fs::write(objects.join(format!("{other}.json")), b"{}").unwrap();
                "corrupt_object"
            }
            "wrong_unselected_pin" => {
                let mut lock: Value =
                    serde_json::from_slice(&fs::read(root.join("pins.json")).unwrap()).unwrap();
                lock["dependencies"][0]["artifact"]["bytes"] = json!(2);
                fs::write(root.join("pins.json"), serde_json::to_vec(&lock).unwrap()).unwrap();
                "digest_mismatch"
            }
            "unrelated_corrupt" | "empty_lock_corrupt" => {
                fs::write(objects.join(format!("{}.json", "00".repeat(32))), b"{}").unwrap();
                if fault == "empty_lock_corrupt" {
                    let mut lock: Value =
                        serde_json::from_slice(&fs::read(root.join("pins.json")).unwrap()).unwrap();
                    lock["dependencies"] = json!([]);
                    fs::write(root.join("pins.json"), serde_json::to_vec(&lock).unwrap()).unwrap();
                }
                "corrupt_object"
            }
            "unknown_alias" => {
                let index = args.iter().position(|arg| arg == "--name").unwrap() + 1;
                args[index] = "absent".into();
                "unknown_object"
            }
            _ => unreachable!(),
        };
        let lock = fs::read(root.join("pins.json")).unwrap();
        for check in [false, true] {
            if check {
                args.push("--check".into());
            }
            failure(&command_os(&args, root), expected);
            assert!(!root.join("witness.rs").exists());
            assert_eq!(fs::read(root.join("pins.json")).unwrap(), lock);
            assert!(stages(root).is_empty());
        }
    }
}

#[test]
fn witness_command_flags_are_closed_and_help_is_available_without_tools() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let help = success(&command(
        &["registry", "generate-witness-rust", "--help", "--json"],
        root,
    ));
    assert!(help["usage"].as_str().unwrap().contains("generate-witness-rust --lock PATH --registry DIR --name ASCII_ALIAS --output NEW_OR_IDENTICAL.rs [--check] [--json]"));
    assert!(
        help["usage"]
            .as_str()
            .unwrap()
            .contains("Neither generator falls back to the other")
    );
    for tail in [
        vec![],
        vec!["--check", "--check"],
        vec!["--entry", "inferred"],
        vec!["--binding", "inferred"],
        vec!["--module-hash", NIL_MODULE],
        vec!["--output"],
        vec!["--output", "one", "--output", "two"],
        vec!["--json"],
    ] {
        let mut args = vec!["registry", "generate-witness-rust", "--json"];
        args.extend(tail);
        failure(&command(&args, root), "invalid_manifest");
    }
    assert_eq!(fs::read_dir(root).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn witness_output_uses_os_paths_and_rejects_reserved_and_aliased_destinations() {
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::symlink},
    };
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    prepare(root, &package());
    fs::write(root.join("protected.rs"), "unchanged source").unwrap();
    symlink(root.join("protected.rs"), root.join("link.rs")).unwrap();
    fs::hard_link(root.join("protected.rs"), root.join("hardlink.rs")).unwrap();
    for output in [
        "Cargo.toml",
        "Cargo.lock",
        "Cargo.locK",
        "pins.json",
        "local/module.rs",
        "link.rs",
        "hardlink.rs",
    ] {
        failure(
            &command_os(&arguments(Path::new(output), false), root),
            "invalid_path",
        );
    }
    let name = OsString::from_vec(b"witness-\xff.rs".to_vec());
    let output = command_os(&arguments(Path::new(&name), false), root);
    if output.status.success() {
        assert_eq!(success(&output)["status"], "created");
        assert_eq!(
            success(&command_os(&arguments(Path::new(&name), true), root))["status"],
            "checked"
        );
    } else {
        failure(&output, "io");
    }
    assert_eq!(
        fs::read_to_string(root.join("protected.rs")).unwrap(),
        "unchanged source"
    );
    assert!(stages(root).is_empty());
}
