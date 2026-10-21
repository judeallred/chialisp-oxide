//! Resolution admits complete local locks and stores without building or writing.
use super::*;
use chialisp_oxide_package::{dependency, digest};
use std::{collections::BTreeMap, ffi::OsString, path::PathBuf};

fn arguments(lock: &Path, registry: &Path, name: Option<&str>) -> Vec<OsString> {
    let mut args = vec![
        "registry".into(),
        "resolve".into(),
        "--lock".into(),
        lock.as_os_str().to_owned(),
        "--registry".into(),
        registry.as_os_str().to_owned(),
        "--json".into(),
    ];
    if let Some(name) = name {
        args.extend(["--name".into(), name.into()]);
    }
    args
}

fn resolve(root: &Path, name: Option<&str>) -> Output {
    command_os(
        &arguments(Path::new("pins.json"), Path::new("local"), name),
        root,
    )
}

fn pin(name: &str, object: &str, source: &Value) -> Value {
    json!({"name":name,"object_id":object,"artifact":source["artifact"]["identity"],"binding":null})
}

fn write_lock(root: &Path, pins: &[Value]) -> Vec<u8> {
    let document = json!({"schema_version":1,"protocol":dependency::LOCK_PROTOCOL,
        "scope":dependency::LOCK_SCOPE,"dependencies":pins});
    // Deliberately noncanonical JSON layout: the digest commits these bytes,
    // and a read-only operation must not rewrite them into another layout.
    let mut bytes = b"\n  ".to_vec();
    bytes.extend(serde_json::to_vec_pretty(&document).unwrap());
    bytes.extend_from_slice(b"\n\n");
    fs::write(root.join("pins.json"), &bytes).unwrap();
    bytes
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut pending = vec![root.to_path_buf()];
    let mut result = BTreeMap::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            let bytes = if metadata.is_dir() {
                pending.push(path.clone());
                None
            } else {
                assert!(metadata.is_file() && !metadata.file_type().is_symlink());
                Some(fs::read(&path).unwrap())
            };
            result.insert(path.strip_prefix(root).unwrap().to_path_buf(), bytes);
        }
    }
    result
}

#[test]
fn exact_sorted_pins_and_aliases_resolve_in_both_argv_forms_without_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let source = package();
    let first = seed(root, "compact.json", &source, false);
    let second = seed(root, "pretty.json", &source, true);
    assert_ne!(first, second, "envelope variants must remain distinct");
    let pins = vec![
        pin("backup", &first, &source),
        pin("payment", &first, &source),
        pin("variant", &second, &source),
    ];
    let lock = write_lock(root, &pins);
    // Invalid Cargo files and unavailable tools must be irrelevant to offline
    // dispatch; command_os also removes PATH and supplies invalid wrappers.
    fs::write(root.join("Cargo.toml"), "not a Cargo manifest").unwrap();
    fs::write(root.join("Cargo.lock"), "keep these exact bytes").unwrap();
    let before = snapshot(root);
    let expected = json!({"status":"resolved","lock_sha256":digest(&lock),
        "dependencies":pins,"scope":dependency::VERIFICATION_SCOPE});
    for prefix in [false, true] {
        let mut args = arguments(Path::new("pins.json"), Path::new("local"), None);
        if prefix {
            args.insert(0, "chialisp-oxide".into());
        }
        assert_eq!(success(&command_os(&args, root)), expected);
        for (name, index) in [("backup", 0), ("payment", 1), ("variant", 2)] {
            let selected = success(&resolve(root, Some(name)));
            assert_eq!(selected["status"], "resolved");
            assert_eq!(selected["lock_sha256"], expected["lock_sha256"]);
            assert_eq!(selected["dependencies"], json!([pins[index]]));
            assert_eq!(selected["scope"], dependency::VERIFICATION_SCOPE);
            assert_eq!(selected.as_object().unwrap().len(), 4);
        }
        assert_eq!(snapshot(root), before);
    }
    assert!(!root.join("pins.json.lock").exists());
}

#[test]
fn selected_alias_cannot_hide_another_bad_pin_or_unrelated_corrupt_object() {
    for fault in [
        "missing_selected",
        "missing_unselected",
        "mismatched_unselected",
        "corrupt_unselected",
        "unrelated_corrupt",
        "unknown_alias_with_corruption",
        "unknown_alias",
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let source = package();
        let selected = seed(root, "compact.json", &source, false);
        let other = seed(root, "pretty.json", &source, true);
        let mut pins = vec![
            pin("other", &other, &source),
            pin("program", &selected, &source),
        ];
        if fault == "mismatched_unselected" {
            pins[0]["artifact"]["bytes"] = json!(2);
        }
        write_lock(root, &pins);
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
            "mismatched_unselected" => "digest_mismatch",
            "corrupt_unselected" => {
                fs::write(objects.join(format!("{other}.json")), b"{}").unwrap();
                "corrupt_object"
            }
            "unrelated_corrupt" | "unknown_alias_with_corruption" => {
                fs::write(objects.join(format!("{}.json", "00".repeat(32))), b"{}").unwrap();
                "corrupt_object"
            }
            "unknown_alias" => "unknown_object",
            _ => unreachable!(),
        };
        let before = snapshot(root);
        let name = if fault.starts_with("unknown_alias") {
            "absent"
        } else {
            "program"
        };
        failure(&resolve(root, Some(name)), expected);
        // Omitting --name still checks the same complete lock and store.
        if fault != "unknown_alias" {
            failure(&resolve(root, None), expected);
        }
        assert_eq!(snapshot(root), before, "{fault}");
    }
}

#[test]
fn an_empty_lock_still_admits_the_store_and_does_not_create_missing_inputs() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let object = seed(root, "input.json", &package(), false);
    let bytes = write_lock(root, &[]);
    let before = snapshot(root);
    assert_eq!(
        success(&resolve(root, None)),
        json!({"status":"resolved",
        "lock_sha256":digest(&bytes),"dependencies":[],"scope":dependency::VERIFICATION_SCOPE})
    );
    failure(&resolve(root, Some("absent")), "unknown_object");
    assert_eq!(snapshot(root), before);
    fs::write(
        root.join("local/objects").join(format!("{object}.json")),
        b"{}",
    )
    .unwrap();
    let before = snapshot(root);
    failure(&resolve(root, None), "corrupt_object");
    failure(&resolve(root, Some("absent")), "corrupt_object");
    assert_eq!(snapshot(root), before);

    let empty = tempfile::tempdir().unwrap();
    failure(&resolve(empty.path(), None), "io");
    write_lock(empty.path(), &[]);
    let before = snapshot(empty.path());
    failure(&resolve(empty.path(), None), "io");
    assert_eq!(snapshot(empty.path()), before);
    assert!(!empty.path().join("local").exists());
}

fn quoted_atom_package(length: usize) -> Value {
    let payload = vec![0x83; length];
    let mut bytes = vec![0xff, 1]; // (q . PAYLOAD), never execute while resolving.
    bytes.extend_from_slice(match length {
        257 => &[0xc1, 1],
        8192 => &[0xe0, 0x20, 0],
        _ => panic!("unsupported fixture length"),
    });
    bytes.extend_from_slice(&payload);
    let mut atom_preimage = vec![1];
    atom_preimage.extend_from_slice(&payload);
    let mut pair_preimage = vec![2];
    pair_preimage.extend_from_slice(digest(&[1, 1]).as_bytes());
    pair_preimage.extend_from_slice(digest(&atom_preimage).as_bytes());
    let mut source = package();
    source["artifact"] = json!({"identity":{
        "encoding":"clvm-legacy-plain/v1","bytes":bytes.len(),
        "blob_sha256":digest(&bytes),"module_tree_hash":digest(&pair_preimage)},
        "clvm_hex":hex::encode(&bytes)});
    source
}

#[test]
fn resolution_is_independent_of_the_static_rust_generation_atom_limit() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let mut pins = Vec::new();
    for (name, length) in [("atom_257", 257), ("atom_8192", 8192)] {
        let source = quoted_atom_package(length);
        let object = seed(root, &format!("{name}.json"), &source, false);
        pins.push(pin(name, &object, &source));
    }
    let lock = write_lock(root, &pins);
    let before = snapshot(root);
    let result = success(&resolve(root, None));
    assert_eq!(result["dependencies"], json!(pins));
    assert_eq!(result["lock_sha256"], digest(&lock).to_string());
    for (index, name) in ["atom_257", "atom_8192"].into_iter().enumerate() {
        assert_eq!(
            success(&resolve(root, Some(name)))["dependencies"],
            json!([pins[index]])
        );
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
                    name,
                    "--output",
                    "unused.rs",
                    "--json",
                ],
                root,
            ),
            "resource_limit",
        );
        assert!(!root.join("unused.rs").exists());
    }
    assert_eq!(snapshot(root), before);
}

#[test]
fn resolve_flags_and_selected_alias_grammar_are_closed_and_help_is_offline() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let help = success(&command(&["registry", "resolve", "--help", "--json"], root));
    let usage = help["usage"].as_str().unwrap();
    assert!(usage.contains("resolve --lock PATH --registry DIR [--name ASCII_ALIAS] [--json]"));
    assert!(usage.contains("checks every pin and the complete local store"));
    for tail in [
        vec![],
        vec!["--lock", "pins.json"],
        vec!["--registry", "local"],
        vec!["--check"],
        vec!["--output", "unused.rs"],
        vec!["--module-hash", NIL_MODULE],
        vec!["--object-id", NIL_MODULE],
        vec!["--binding", "inferred"],
        vec!["--name"],
        vec!["--name", "one", "--name", "two"],
        vec!["--lock", "one", "--lock", "two"],
        vec!["--json"],
    ] {
        let mut args = vec!["registry", "resolve", "--json"];
        args.extend(tail);
        failure(&command(&args, root), "invalid_manifest");
    }
    assert!(snapshot(root).is_empty());
    seed(root, "input.json", &package(), false);
    write_lock(root, &[]);
    let before = snapshot(root);
    for name in [
        "Upper",
        "_leading",
        "1leading",
        "hyphen-name",
        "space name",
        "é",
        &"a".repeat(65),
    ] {
        failure(&resolve(root, Some(name)), "invalid_manifest");
    }
    // Both inclusive ends of the shared alias grammar are valid, albeit absent.
    for name in ["a", &"a".repeat(64)] {
        failure(&resolve(root, Some(name)), "unknown_object");
    }
    assert_eq!(snapshot(root), before);
}

#[test]
fn resolve_rejects_invalid_closed_lock_protocols_and_resource_limits_readonly() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let source = package();
    let object = seed(root, "input.json", &source, false);
    let bytes = write_lock(root, &[pin("program", &object, &source)]);
    let original: Value = serde_json::from_slice(&bytes).unwrap();
    for fault in [
        "version",
        "protocol",
        "scope",
        "extra",
        "binding",
        "missing_binding",
        "duplicate_alias",
    ] {
        let mut lock = original.clone();
        let expected = match fault {
            "version" => {
                lock["schema_version"] = json!(2);
                "unsupported_version"
            }
            "protocol" => {
                lock["protocol"] = json!("inferred/v1");
                "invalid_manifest"
            }
            "scope" => {
                lock["scope"] = json!("trusted_source");
                "invalid_manifest"
            }
            "extra" => {
                lock["inferred_abi"] = json!("unknown");
                "invalid_manifest"
            }
            "binding" => {
                lock["dependencies"][0]["binding"] = json!("inferred");
                "invalid_manifest"
            }
            "missing_binding" => {
                lock["dependencies"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("binding");
                "invalid_manifest"
            }
            "duplicate_alias" => {
                lock["dependencies"]
                    .as_array_mut()
                    .unwrap()
                    .push(original["dependencies"][0].clone());
                "invalid_manifest"
            }
            _ => unreachable!(),
        };
        fs::write(root.join("pins.json"), serde_json::to_vec(&lock).unwrap()).unwrap();
        let before = snapshot(root);
        failure(&resolve(root, Some("program")), expected);
        assert_eq!(snapshot(root), before, "{fault}");
    }
    fs::write(
        root.join("pins.json"),
        vec![b' '; dependency::MAX_LOCK_BYTES + 1],
    )
    .unwrap();
    let before = snapshot(root);
    failure(&resolve(root, None), "resource_limit");
    assert_eq!(snapshot(root), before);
}

#[cfg(unix)]
#[test]
fn resolution_preserves_non_utf8_os_paths_but_requires_utf8_aliases() {
    use std::os::unix::ffi::OsStringExt;
    for target in ["lock", "registry"] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let source = package();
        let object = seed(root, "input.json", &source, false);
        write_lock(root, &[pin("program", &object, &source)]);
        let filename = OsString::from_vec(
            format!("{target}-")
                .into_bytes()
                .into_iter()
                .chain([0xff])
                .collect(),
        );
        let (lock, registry, original) = if target == "lock" {
            (Path::new(&filename), Path::new("local"), "pins.json")
        } else {
            (Path::new("pins.json"), Path::new(&filename), "local")
        };
        let moved = fs::rename(root.join(original), root.join(&filename));
        let before = snapshot(root);
        let output = command_os(&arguments(lock, registry, Some("program")), root);
        match moved {
            Ok(()) => assert_eq!(
                success(&output)["dependencies"],
                json!([pin("program", &object, &source)])
            ),
            Err(error) => {
                assert_eq!(
                    error.raw_os_error(),
                    Some(libc::EILSEQ),
                    "unexpected filesystem error"
                );
                failure(&output, "io");
            }
        }
        assert_eq!(snapshot(root), before);
    }
    let temporary = tempfile::tempdir().unwrap();
    let mut args = arguments(Path::new("pins.json"), Path::new("local"), None);
    args.extend(["--name".into(), OsString::from_vec(vec![0xff])]);
    failure(&command_os(&args, temporary.path()), "invalid_path");
    assert!(snapshot(temporary.path()).is_empty());
}
