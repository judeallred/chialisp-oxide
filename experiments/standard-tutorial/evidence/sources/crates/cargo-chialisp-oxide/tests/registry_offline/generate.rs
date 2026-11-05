//! Actual offline prebuilt commands; source generation is not compilation.
use super::*;

fn generate_arguments(output: &Path, check: bool) -> Vec<std::ffi::OsString> {
    let mut args = vec![
        "registry".into(),
        "generate-rust".into(),
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
        args.push("--check".into());
    }
    args
}
#[cfg(unix)]
fn prepare(root: &Path) -> String {
    let id = seed(root, "input.json", &package(), false);
    success(&add(root, "program", NIL_MODULE, Some(&id), "pins.json"));
    id
}
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
#[test]
fn actual_offline_generation_in_both_argv_forms_is_exact_and_readonly_check_regenerates() {
    for prefix in [false, true] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let id = prepare(root);
        fs::write(root.join("Cargo.toml"), "protected manifest bytes").unwrap();
        fs::write(root.join("Cargo.lock"), "protected Cargo lock bytes").unwrap();
        let lock_before = fs::read(root.join("pins.json")).unwrap();
        let object = root.join("local/objects").join(format!("{id}.json"));
        let object_before = fs::read(&object).unwrap();
        let mut args = generate_arguments(Path::new("module.rs"), false);
        if prefix {
            args.insert(0, "chialisp-oxide".into());
        }
        let first = success(&command_os(&args, root));
        assert_eq!(first["status"], "created");
        assert_eq!(first["output"]["status"], "created");
        assert_eq!(first["generation"]["schema_version"], 1);
        assert_eq!(
            first["generation"]["recipe"],
            "chialisp-oxide-generated-tree/v1"
        );
        assert_eq!(first["generation"]["object_id"], id);
        assert_eq!(first["generation"]["selected_alias"], "program");
        assert_eq!(
            first["generation"]["artifact"],
            package()["artifact"]["identity"]
        );
        assert_eq!(
            first["generation"]["lock_sha256"],
            chialisp_oxide_package::digest(&lock_before).to_string()
        );
        let source = fs::read(root.join("module.rs")).unwrap();
        let source_hash = chialisp_oxide_package::digest(&source).to_string();
        assert_eq!(first["generation"]["source_sha256"], source_hash);
        assert_eq!(first["generation"]["source_bytes"], source.len());
        assert_eq!(first["output"]["source_sha256"], source_hash);
        assert_eq!(first["output"]["source_bytes"], source.len());
        let source_text = std::str::from_utf8(&source).unwrap();
        assert!(source_text.contains("pub(super) fn run"));
        assert!(!source_text.contains("#[chialisp_oxide::puzzle"));
        let repeated = success(&command_os(&args, root));
        assert_eq!(repeated["status"], "already_present");
        assert_eq!(repeated["generation"], first["generation"]);
        args.push("--check".into());
        let checked = success(&command_os(&args, root));
        assert_eq!(checked["status"], "checked");
        assert_eq!(checked["generation"], first["generation"]);
        assert_eq!(fs::read(root.join("module.rs")).unwrap(), source);
        assert_eq!(fs::read(root.join("pins.json")).unwrap(), lock_before);
        assert_eq!(fs::read(object).unwrap(), object_before);
        assert_eq!(
            fs::read_to_string(root.join("Cargo.toml")).unwrap(),
            "protected manifest bytes"
        );
        assert_eq!(
            fs::read_to_string(root.join("Cargo.lock")).unwrap(),
            "protected Cargo lock bytes"
        );
        assert!(stages(root).is_empty());
    }
}

#[cfg(unix)]
#[test]
fn exact_source_check_detects_manual_changes_and_normal_generation_never_clobbers() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    prepare(root);
    failure(
        &command_os(&generate_arguments(Path::new("module.rs"), true), root),
        "conflict",
    );
    assert!(!root.join("module.rs").exists());
    success(&command_os(
        &generate_arguments(Path::new("module.rs"), false),
        root,
    ));
    let mut changed = fs::read(root.join("module.rs")).unwrap();
    changed.extend_from_slice(b"\n// author modified source\n");
    fs::write(root.join("module.rs"), &changed).unwrap();
    for check in [false, true] {
        failure(
            &command_os(&generate_arguments(Path::new("module.rs"), check), root),
            "conflict",
        );
        assert_eq!(fs::read(root.join("module.rs")).unwrap(), changed);
    }
    assert!(stages(root).is_empty());
}

#[cfg(unix)]
#[test]
fn complete_pins_and_unrelated_store_corruption_fail_before_output_creation() {
    for fault in [
        "missing_selected",
        "missing_unselected",
        "corrupt_unselected",
        "unrelated_corrupt",
        "unknown_alias",
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let selected = prepare(root);
        let other_package = atom_one_package();
        let other = seed(root, "other.json", &other_package, false);
        success(&add(
            root,
            "other",
            other_package["artifact"]["identity"]["module_tree_hash"]
                .as_str()
                .unwrap(),
            Some(&other),
            "pins.json",
        ));
        let lock = fs::read(root.join("pins.json")).unwrap();
        let objects = root.join("local/objects");
        let mut args = generate_arguments(Path::new("module.rs"), false);
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
            "unrelated_corrupt" => {
                fs::write(objects.join(format!("{}.json", "ff".repeat(32))), b"{}").unwrap();
                "corrupt_object"
            }
            "unknown_alias" => {
                let index = args.iter().position(|arg| arg == "--name").unwrap();
                args[index + 1] = "absent".into();
                "unknown_object"
            }
            _ => unreachable!(),
        };
        failure(&command_os(&args, root), expected);
        assert!(!root.join("module.rs").exists());
        assert_eq!(fs::read(root.join("pins.json")).unwrap(), lock);
        assert!(stages(root).is_empty());
    }
}

#[cfg(unix)]
#[test]
fn generated_source_profile_rejects_257_byte_atom_without_splitting_or_partial_output() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let payload = vec![0xab; 257];
    let mut blob = vec![0xc1, 0x01];
    blob.extend_from_slice(&payload);
    let mut leaf = vec![1];
    leaf.extend_from_slice(&payload);
    let mut oversized = package();
    oversized["artifact"] = json!({"identity":{"encoding":"clvm-legacy-plain/v1","bytes":blob.len(),
        "blob_sha256":chialisp_oxide_package::digest(&blob),"module_tree_hash":chialisp_oxide_package::digest(&leaf)},"clvm_hex":hex::encode(&blob)});
    let id = seed(root, "large.json", &oversized, false);
    success(&add(
        root,
        "program",
        oversized["artifact"]["identity"]["module_tree_hash"]
            .as_str()
            .unwrap(),
        Some(&id),
        "pins.json",
    ));
    failure(
        &command_os(&generate_arguments(Path::new("module.rs"), false), root),
        "resource_limit",
    );
    assert!(!root.join("module.rs").exists());
    assert!(stages(root).is_empty());
}

#[test]
fn generation_flags_are_closed_and_help_is_offline() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let help = success(&command(
        &["registry", "generate-rust", "--help", "--json"],
        root,
    ));
    let usage = help["usage"].as_str().unwrap();
    assert!(usage.contains("generate-rust --lock PATH --registry DIR --name ASCII_ALIAS --output NEW_OR_IDENTICAL.rs [--check]"));
    assert!(usage.contains("regenerates and compares read-only"));
    assert!(usage.contains("still requires native Rust and CLVM subset checks"));
    for tail in [
        vec!["--check", "--check"],
        vec!["--module-hash", NIL_MODULE],
        vec!["--entry", "inferred"],
        vec!["--output"],
        vec!["--binding", "inferred"],
        vec!["--json", "--json"],
    ] {
        let mut args = vec!["registry", "generate-rust", "--json"];
        args.extend(tail);
        failure(&command(&args, root), "invalid_manifest");
    }
    failure(
        &command(&["registry", "lookup", "--check", "--json"], root),
        "invalid_manifest",
    );
    failure(
        &command(&["registry", "generate-rust", "--json"], root),
        "invalid_manifest",
    );
    assert!(!root.join("local").exists());
    assert!(!root.join("pins.json").exists());
    assert!(!root.join("module.rs").exists());
}

#[cfg(unix)]
#[test]
fn output_paths_preserve_os_bytes_and_reserved_or_aliased_paths_fail_closed() {
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::symlink},
    };
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    prepare(root);
    let protected = root.join("protected.rs");
    fs::write(&protected, "keep these bytes").unwrap();
    symlink(&protected, root.join("link.rs")).unwrap();
    fs::hard_link(&protected, root.join("hardlink.rs")).unwrap();
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        "Cargo.locK",
        "pins.json",
        "local/module.rs",
        "link.rs",
        "hardlink.rs",
    ] {
        failure(
            &command_os(&generate_arguments(Path::new(path), false), root),
            "invalid_path",
        );
    }
    assert_eq!(fs::read_to_string(protected).unwrap(), "keep these bytes");
    let name = OsString::from_vec(b"generated-\xff.rs".to_vec());
    // macOS filesystems may reject invalid UTF-8; Linux accepts it. An actual
    // host IO failure remains distinct from a path parser UTF-8 rejection.
    let output = command_os(&generate_arguments(Path::new(&name), false), root);
    if output.status.success() {
        assert_eq!(success(&output)["status"], "created");
        assert!(root.join(&name).is_file());
        assert_eq!(
            success(&command_os(
                &generate_arguments(Path::new(&name), true),
                root
            ))["status"],
            "checked"
        );
    } else {
        assert_eq!(value(&output)["kind"], "io");
        assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    }
    assert!(stages(root).is_empty());
}
