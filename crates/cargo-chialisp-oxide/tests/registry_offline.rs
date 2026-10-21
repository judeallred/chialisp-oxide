//! Actual prebuilt CLI operations with no Cargo/rustup/driver available.
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
fn command_os(args: &[std::ffi::OsString], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cargo-chialisp-oxide"))
        .args(args)
        .current_dir(cwd)
        .env("PATH", "")
        .env("CARGO", "/nonexistent/cargo")
        .env("RUSTC", "/nonexistent/rustc")
        .env("RUSTUP_TOOLCHAIN", "nonexistent-registry-toolchain")
        .env("CHIALISP_OXIDE_DRIVER", "/nonexistent/driver")
        .env("RUSTC_WRAPPER", "/nonexistent/wrapper")
        .env("RUSTC_WORKSPACE_WRAPPER", "/nonexistent/workspace-wrapper")
        .env("CARGO_BUILD_RUSTC_WRAPPER", "/nonexistent/build-wrapper")
        .output()
        .expect("execute actual prebuilt CLI")
}
fn command(args: &[&str], cwd: &Path) -> Output {
    command_os(
        &args
            .iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>(),
        cwd,
    )
}

fn value(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout must contain exactly one JSON value")
}
fn package() -> Value {
    json!({"schema_version":1,"kind":"clvm_artifact","artifact":{"identity":{"encoding":"clvm-legacy-plain/v1","bytes":1,
        "blob_sha256":"76be8b528d0075f7aae98d6fa57a6d3c83ae480a8469e668d7b0af968995ac71",
        "module_tree_hash":"4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"},"clvm_hex":"80"},
        "rust":null,"source_scope":"not_recorded","sources":[],"provenance":null})
}

const NIL_MODULE: &str = "4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a";

fn success(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    value(output)
}

fn failure(output: &Output, kind: &str) {
    assert!(
        !output.status.success(),
        "unexpected success: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result = value(output);
    assert_eq!(result["status"], "error");
    assert_eq!(
        result["kind"],
        kind,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("existing RUSTC_WRAPPER"));
}

fn seed(root: &Path, filename: &str, package: &Value, pretty: bool) -> String {
    let bytes = if pretty {
        serde_json::to_vec_pretty(package)
    } else {
        serde_json::to_vec(package)
    }
    .unwrap();
    fs::write(root.join(filename), bytes).unwrap();
    let output = command(
        &[
            "registry",
            "publish",
            "--package",
            filename,
            "--registry",
            "local",
            "--json",
        ],
        root,
    );
    success(&output)["package"]["object_id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn add_arguments(
    name: &str,
    module: &str,
    object: Option<&str>,
    registry: &Path,
    lock: &Path,
) -> Vec<std::ffi::OsString> {
    let mut args = vec![
        "registry".into(),
        "add".into(),
        "--name".into(),
        name.into(),
        "--module-hash".into(),
        module.into(),
        "--registry".into(),
        registry.as_os_str().to_owned(),
        "--lock".into(),
        lock.as_os_str().to_owned(),
        "--json".into(),
    ];
    if let Some(object) = object {
        args.extend(["--object-id".into(), object.into()]);
    }
    args
}

#[cfg(unix)]
fn add(root: &Path, name: &str, module: &str, object: Option<&str>, lock: &str) -> Output {
    command_os(
        &add_arguments(name, module, object, Path::new("local"), Path::new(lock)),
        root,
    )
}

#[cfg(unix)]
fn atom_one_package() -> Value {
    let mut other = package();
    // A second complete plain artifact, admitted without executing a VM.
    other["artifact"] = json!({"identity":{
        "encoding":"clvm-legacy-plain/v1", "bytes":1,
        "blob_sha256":chialisp_oxide_package::digest(&[1]).to_string(),
        "module_tree_hash":chialisp_oxide_package::digest(&[1,1]).to_string(),
    },"clvm_hex":"01"});
    other
}

#[cfg(unix)]
#[test]
fn add_pins_exact_identity_offline_in_both_argv_forms_and_preserves_idempotent_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let source = package();
    let id = seed(root, "input.json", &source, false);
    let object_path = root.join("local/objects").join(format!("{id}.json"));
    let object_bytes = fs::read(&object_path).unwrap();
    for prefix in [false, true] {
        let filename = if prefix {
            "prefixed.lock"
        } else {
            "direct.lock"
        };
        let mut args = add_arguments(
            "payment",
            NIL_MODULE,
            None,
            Path::new("local"),
            Path::new(filename),
        );
        if prefix {
            args.insert(0, "chialisp-oxide".into());
        }
        let result = success(&command_os(&args, root));
        assert_eq!(result["status"], "added");
        assert_eq!(result["dependency_count"], 1);
        assert_eq!(
            result["pin"],
            json!({"name":"payment","object_id":id,
            "artifact":source["artifact"]["identity"],"binding":null})
        );
        assert!(
            result["scope"]
                .as_str()
                .unwrap()
                .contains("no recovered bindings")
        );
        let path = root.join(filename);
        let bytes = fs::read(&path).unwrap();
        assert_eq!(
            result["lock_sha256"],
            chialisp_oxide_package::digest(&bytes).to_string()
        );
        let document: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            document,
            json!({"schema_version":1,
            "protocol":"chialisp-oxide-dependencies/v1","scope":"local_exact_artifact_pins",
            "dependencies":[result["pin"]]})
        );

        // A checked but differently formatted old lock must stay byte-exact on
        // AlreadyPresent, rather than being silently rewritten canonically.
        let mut reformatted = b"\n  ".to_vec();
        reformatted.extend(serde_json::to_vec_pretty(&document).unwrap());
        reformatted.extend_from_slice(b"\n\n");
        fs::write(&path, &reformatted).unwrap();
        args.extend(["--object-id".into(), id.clone().into()]);
        let repeated = success(&command_os(&args, root));
        assert_eq!(repeated["status"], "already_present");
        assert_eq!(repeated["pin"], result["pin"]);
        assert_eq!(
            repeated["lock_sha256"],
            chialisp_oxide_package::digest(&reformatted).to_string()
        );
        assert_eq!(fs::read(&path).unwrap(), reformatted);

        let extra = success(&add(root, "backup", NIL_MODULE, Some(&id), filename));
        assert_eq!(extra["status"], "added");
        assert_eq!(extra["dependency_count"], 2);
        let updated: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let pins = updated["dependencies"].as_array().unwrap();
        assert_eq!(
            pins.iter()
                .map(|pin| pin["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["backup", "payment"]
        );
        assert!(
            pins.iter()
                .all(|pin| pin["object_id"] == id && pin["binding"].is_null())
        );
    }
    assert_eq!(fs::read(object_path).unwrap(), object_bytes);
    assert_eq!(
        fs::read(root.join("input.json")).unwrap(),
        serde_json::to_vec(&source).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn add_rejects_ambiguous_or_wrong_exact_selection_and_never_rebinds_an_alias() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let first = seed(root, "compact.json", &package(), false);
    let second = seed(root, "pretty.json", &package(), true);
    assert_ne!(
        first, second,
        "Exact envelope bytes, rather than tree hash, select variants"
    );
    let third_package = atom_one_package();
    let third = seed(root, "other.json", &third_package, false);
    failure(
        &add(root, "payment", NIL_MODULE, None, "consumer.lock"),
        "conflict",
    );
    assert!(!root.join("consumer.lock").exists());
    let result = success(&add(
        root,
        "payment",
        NIL_MODULE,
        Some(&first),
        "consumer.lock",
    ));
    assert_eq!(result["pin"]["object_id"], first);
    let before = fs::read(root.join("consumer.lock")).unwrap();
    failure(
        &add(root, "payment", NIL_MODULE, Some(&second), "consumer.lock"),
        "conflict",
    );
    assert_eq!(fs::read(root.join("consumer.lock")).unwrap(), before);
    failure(
        &add(root, "other", NIL_MODULE, Some(&third), "consumer.lock"),
        "conflict",
    );
    failure(
        &add(
            root,
            "other",
            NIL_MODULE,
            Some(&"00".repeat(32)),
            "consumer.lock",
        ),
        "unknown_object",
    );
    failure(
        &add(root, "other", &"11".repeat(32), None, "consumer.lock"),
        "unknown_object",
    );
    assert_eq!(fs::read(root.join("consumer.lock")).unwrap(), before);
    let other_hash = third_package["artifact"]["identity"]["module_tree_hash"]
        .as_str()
        .unwrap();
    let other = success(&add(
        root,
        "other",
        other_hash,
        Some(&third),
        "consumer.lock",
    ));
    assert_eq!(other["pin"]["object_id"], third);
    assert_eq!(other["dependency_count"], 2);
}

#[cfg(unix)]
#[test]
fn add_rechecks_closed_old_locks_and_complete_registry_before_writing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let id = seed(root, "input.json", &package(), false);
    success(&add(root, "payment", NIL_MODULE, None, "consumer.lock"));
    let path = root.join("consumer.lock");
    let good = fs::read(&path).unwrap();
    let document: Value = serde_json::from_slice(&good).unwrap();
    let mut corruptions = vec![(b"{".to_vec(), "invalid_manifest")];
    let mut unknown_field = document.clone();
    unknown_field["unreviewed"] = json!(true);
    corruptions.push((
        serde_json::to_vec(&unknown_field).unwrap(),
        "invalid_manifest",
    ));
    let mut version = document.clone();
    version["schema_version"] = json!(2);
    corruptions.push((serde_json::to_vec(&version).unwrap(), "unsupported_version"));
    let mut missing_binding = document.clone();
    missing_binding["dependencies"][0]
        .as_object_mut()
        .unwrap()
        .remove("binding");
    corruptions.push((
        serde_json::to_vec(&missing_binding).unwrap(),
        "invalid_manifest",
    ));
    let mut claimed_binding = document.clone();
    claimed_binding["dependencies"][0]["binding"] = json!({"name":"Amount"});
    corruptions.push((
        serde_json::to_vec(&claimed_binding).unwrap(),
        "invalid_manifest",
    ));
    let mut wrong_digest = document.clone();
    wrong_digest["dependencies"][0]["artifact"]["blob_sha256"] = json!("00".repeat(32));
    corruptions.push((
        serde_json::to_vec(&wrong_digest).unwrap(),
        "digest_mismatch",
    ));
    let mut missing_object = document.clone();
    missing_object["dependencies"][0]["object_id"] = json!("00".repeat(32));
    corruptions.push((
        serde_json::to_vec(&missing_object).unwrap(),
        "unknown_object",
    ));
    for (bytes, kind) in corruptions {
        fs::write(&path, &bytes).unwrap();
        failure(
            &add(root, "backup", NIL_MODULE, Some(&id), "consumer.lock"),
            kind,
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            bytes,
            "Rejected old lock must remain exact"
        );
    }
    fs::write(&path, &good).unwrap();
    let object_path = root.join("local/objects").join(format!("{id}.json"));
    let object = fs::read(&object_path).unwrap();
    let mut changed = object.clone();
    changed.push(b'\n');
    fs::write(&object_path, &changed).unwrap();
    failure(
        &add(root, "backup", NIL_MODULE, Some(&id), "consumer.lock"),
        "corrupt_object",
    );
    assert_eq!(fs::read(&path).unwrap(), good);
    assert_eq!(fs::read(&object_path).unwrap(), changed);
    fs::write(&object_path, object).unwrap();

    // An explicit exact good object cannot bypass a later unrelated corrupt
    // object in the complete bounded registry scan.
    let unrelated = root
        .join("local/objects")
        .join(format!("{}.json", "ff".repeat(32)));
    fs::write(&unrelated, b"{}").unwrap();
    failure(
        &add(root, "backup", NIL_MODULE, Some(&id), "consumer.lock"),
        "corrupt_object",
    );
    assert_eq!(fs::read(&path).unwrap(), good);
    failure(
        &add(root, "payment", NIL_MODULE, Some(&id), "new.lock"),
        "corrupt_object",
    );
    assert!(!root.join("new.lock").exists());
}

#[cfg(unix)]
#[test]
fn add_requires_explicit_safe_paths_and_closed_flags_without_build_tools() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let id = seed(root, "input.json", &package(), false);
    for destination in [
        "Cargo.toml",
        "Cargo.lock",
        "cargo.toml",
        "cargo.lock",
        "CARGO.TOML",
        "CaRgO.LoCk",
        "local/consumer.lock",
        "local/objects/consumer.lock",
        "local/.staging/consumer.lock",
    ] {
        failure(
            &add(root, "payment", NIL_MODULE, Some(&id), destination),
            "invalid_path",
        );
        assert!(
            !root.join(destination).exists(),
            "Rejected destination created: {destination}"
        );
    }
    fs::write(root.join("Cargo.toml"), b"existing consumer manifest").unwrap();
    failure(
        &add(root, "payment", NIL_MODULE, Some(&id), "Cargo.toml"),
        "invalid_path",
    );
    assert_eq!(
        fs::read(root.join("Cargo.toml")).unwrap(),
        b"existing consumer manifest"
    );
    fs::create_dir(root.join("directory.lock")).unwrap();
    failure(
        &add(root, "payment", NIL_MODULE, Some(&id), "directory.lock"),
        "invalid_path",
    );
    assert!(root.join("directory.lock").is_dir());
    failure(
        &add(
            root,
            "payment",
            NIL_MODULE,
            Some(&id),
            "missing/consumer.lock",
        ),
        "io",
    );
    assert!(!root.join("missing").exists());
    let long_name = "a".repeat(65);
    for name in [
        "",
        "Payment",
        "0payment",
        "payment/name",
        "payment-name",
        "é",
        &long_name,
    ] {
        failure(
            &add(root, name, NIL_MODULE, Some(&id), "consumer.lock"),
            "invalid_manifest",
        );
        assert!(!root.join("consumer.lock").exists());
    }
    for (name, flag) in [
        ("--lock", "consumer.lock"),
        ("--name", "payment"),
        ("--registry", "local"),
    ] {
        let mut args = add_arguments(
            "payment",
            NIL_MODULE,
            Some(&id),
            Path::new("local"),
            Path::new("consumer.lock"),
        );
        let offset = args.iter().position(|value| value == name).unwrap();
        assert_eq!(args[offset + 1], flag);
        drop(args.drain(offset..offset + 2));
        failure(&command_os(&args, root), "invalid_manifest");
    }
    let mut args = add_arguments(
        "payment",
        NIL_MODULE,
        Some(&id),
        Path::new("local"),
        Path::new("consumer.lock"),
    );
    args.extend(["--binding".into(), "guessed_type".into()]);
    failure(&command_os(&args, root), "invalid_manifest");
    assert!(!root.join("consumer.lock").exists());
    let args = add_arguments(
        "payment",
        NIL_MODULE,
        Some(&id),
        Path::new("absent-registry"),
        Path::new("consumer.lock"),
    );
    failure(&command_os(&args, root), "io");
    assert!(!root.join("absent-registry").exists());
    assert!(!root.join("consumer.lock").exists());
    let output = command(&["registry", "add", "--help", "--json"], root);
    let help = success(&output);
    assert!(
        help["usage"]
            .as_str()
            .unwrap()
            .contains("registry add --name ASCII_ALIAS")
    );
    assert!(help["usage"].as_str().unwrap().contains("--lock PATH"));
}

#[cfg(not(unix))]
#[test]
fn add_reports_unsupported_io_without_creating_a_lock_sidecar_or_stage() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let id = seed(root, "input.json", &package(), false);
    let names = || {
        let mut names = fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        names.sort();
        names
    };
    let before = names();
    for prefixed in [false, true] {
        let mut args = add_arguments(
            "payment",
            NIL_MODULE,
            Some(&id),
            Path::new("local"),
            Path::new("consumer.lock"),
        );
        if prefixed {
            args.insert(0, "chialisp-oxide".into());
        }
        failure(&command_os(&args, root), "io");
        assert!(!root.join("consumer.lock").exists());
        assert!(!root.join(".consumer.lock.write-lock").exists());
        assert_eq!(
            names(),
            before,
            "Unsupported first add must not create coordination or staging files"
        );
    }
}

#[cfg(unix)]
#[test]
fn add_rejects_symlink_lock_destinations_without_changing_either_file() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let id = seed(root, "input.json", &package(), false);
    fs::write(root.join("target.lock"), b"untouched destination").unwrap();
    symlink(root.join("target.lock"), root.join("consumer.lock")).unwrap();
    failure(
        &add(root, "payment", NIL_MODULE, Some(&id), "consumer.lock"),
        "invalid_path",
    );
    assert!(
        fs::symlink_metadata(root.join("consumer.lock"))
            .unwrap()
            .is_symlink()
    );
    assert_eq!(
        fs::read(root.join("target.lock")).unwrap(),
        b"untouched destination"
    );
    fs::remove_file(root.join("consumer.lock")).unwrap();
    symlink(root.join("does-not-exist.lock"), root.join("consumer.lock")).unwrap();
    failure(
        &add(root, "payment", NIL_MODULE, Some(&id), "consumer.lock"),
        "invalid_path",
    );
    assert!(!root.join("does-not-exist.lock").exists());
    assert!(
        fs::symlink_metadata(root.join("consumer.lock"))
            .unwrap()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn add_preserves_non_utf8_registry_and_lock_paths_or_returns_the_filesystem_error() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let id = seed(root, "input.json", &package(), false);
    let mut invalid_name = add_arguments(
        "payment",
        NIL_MODULE,
        Some(&id),
        Path::new("local"),
        Path::new("consumer.lock"),
    );
    invalid_name[3] = OsString::from_vec(vec![0xff]);
    failure(&command_os(&invalid_name, root), "invalid_path");
    assert!(!root.join("consumer.lock").exists());
    let registry = OsString::from_vec(b"registry-\xff".to_vec());
    let lock = OsString::from_vec(b"dependency-\xfe.lock".to_vec());
    let registry_path = Path::new(&registry);
    let lock_path = Path::new(&lock);
    // Some Unix hosts reject these names at the filesystem boundary. Keep that
    // observed I/O failure separate from Rust/CLI UTF-8 identity restrictions.
    match fs::rename(root.join("local"), root.join(registry_path)) {
        Ok(()) => {}
        Err(error) => {
            assert_eq!(
                error.raw_os_error(),
                Some(libc::EILSEQ),
                "unexpected fixture filesystem error"
            );
            let output = command_os(
                &add_arguments("payment", NIL_MODULE, Some(&id), registry_path, lock_path),
                root,
            );
            failure(&output, "io");
            assert!(!root.join(lock_path).exists());
            return;
        }
    }
    let supports_lock = match fs::write(root.join(lock_path), b"probe") {
        Ok(()) => {
            fs::remove_file(root.join(lock_path)).unwrap();
            true
        }
        Err(error) => {
            assert_eq!(
                error.raw_os_error(),
                Some(libc::EILSEQ),
                "unexpected fixture filesystem error"
            );
            false
        }
    };
    let mut args = add_arguments("payment", NIL_MODULE, Some(&id), registry_path, lock_path);
    args.insert(0, "chialisp-oxide".into());
    let output = command_os(&args, root);
    if supports_lock {
        let result = success(&output);
        assert_eq!(result["status"], "added");
        assert_eq!(result["pin"]["object_id"], id);
        assert_eq!(result["pin"]["artifact"]["module_tree_hash"], NIL_MODULE);
        let bytes = fs::read(root.join(lock_path)).unwrap();
        assert_eq!(
            result["lock_sha256"],
            chialisp_oxide_package::digest(&bytes).to_string()
        );
        assert!(
            serde_json::from_slice::<Value>(&bytes).unwrap()["dependencies"][0]["binding"]
                .is_null()
        );
        assert_eq!(
            success(&command_os(&args, root))["status"],
            "already_present"
        );
        assert_eq!(fs::read(root.join(lock_path)).unwrap(), bytes);
    } else {
        failure(&output, "io");
        assert!(!root.join(lock_path).exists());
    }
}
#[test]
fn prebuilt_verify_publish_lookup_get_are_offline_for_both_argv_forms() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(
        root.join("input.json"),
        serde_json::to_vec(&package()).unwrap(),
    )
    .unwrap();
    for prefix in [vec![], vec!["chialisp-oxide"]] {
        let run = |tail: &[&str]| {
            let mut args = prefix.clone();
            args.extend_from_slice(tail);
            command(&args, root)
        };
        let mut args = prefix.clone();
        args.extend(["registry", "verify", "--package", "input.json", "--json"]);
        let output = command(&args, root);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let verified = value(&output);
        assert_eq!(verified["status"], "verified");
        assert_eq!(verified["package"]["source_scope"], "not_recorded");
        let id = verified["package"]["object_id"].as_str().unwrap();
        let output = run(&[
            "registry",
            "publish",
            "--package",
            "input.json",
            "--registry",
            "local",
            "--json",
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result = value(&output);
        assert_eq!(result["package"]["object_id"], id);
        assert!(output.stderr.is_empty());
        let output = run(&[
            "registry",
            "get",
            "--object-id",
            id,
            "--registry",
            "local",
            "--json",
        ]);
        assert!(output.status.success());
        assert_eq!(value(&output)["package"]["object_id"], id);
        let output = run(&[
            "registry",
            "lookup",
            "--module-hash",
            "4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a",
            "--registry",
            "local",
            "--json",
        ]);
        assert!(output.status.success());
        assert_eq!(value(&output)["packages"].as_array().unwrap().len(), 1);
        assert!(
            root.join("local/objects")
                .join(format!("{id}.json"))
                .is_file()
        );
    }
}
#[test]
fn typed_errors_are_machine_json_and_never_start_build_tools() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for args in [
        vec!["registry", "verify", "--package", "missing.json", "--json"],
        vec![
            "registry",
            "lookup",
            "--module-hash",
            "wrong",
            "--registry",
            "local",
            "--json",
        ],
        vec![
            "registry",
            "publish",
            "--bundle",
            "missing.json",
            "--registry",
            "local",
            "--json",
        ],
        vec!["registry", "verify", "--unknown", "--json"],
        vec!["registry", "verify", "--json", "--json"],
    ] {
        let output = command(&args, root);
        assert!(!output.status.success());
        let result = value(&output);
        assert_eq!(result["status"], "error");
        assert!(result["kind"].is_string());
        assert!(!output.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("existing RUSTC_WRAPPER"));
        assert!(
            !root.join("local").exists(),
            "failed import must not create a registry"
        );
    }
    let mut bad = package();
    bad["artifact"]["clvm_hex"] = json!("ff");
    fs::write(root.join("bad.json"), serde_json::to_vec(&bad).unwrap()).unwrap();
    let output = command(
        &[
            "registry",
            "publish",
            "--package",
            "bad.json",
            "--registry",
            "local",
            "--json",
        ],
        root,
    );
    assert!(!output.status.success());
    assert!(!root.join("local").exists());
}
#[test]
fn help_is_available_without_toolchain_and_value_named_help_remains_a_path() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let output = command(&["chialisp-oxide", "registry", "--help", "--json"], root);
    assert!(output.status.success());
    assert_eq!(value(&output)["status"], "help");
    fs::write(root.join("help"), serde_json::to_vec(&package()).unwrap()).unwrap();
    let output = command(&["registry", "verify", "--package", "help", "--json"], root);
    assert!(output.status.success());
    assert_eq!(value(&output)["status"], "verified");
}

#[cfg(unix)]
#[test]
fn filesystem_paths_preserve_non_utf8_and_identity_errors_stay_typed() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let filename = OsString::from_vec(b"puzzle-\xff.json".to_vec());
    let creation = fs::write(
        root.join(&filename),
        serde_json::to_vec(&package()).unwrap(),
    );
    let args = vec![
        OsString::from("registry"),
        "verify".into(),
        "--package".into(),
        filename,
        "--json".into(),
    ];
    let output = command_os(&args, root);
    match creation {
        Ok(()) => {
            assert!(output.status.success());
            assert_eq!(value(&output)["status"], "verified");
        }
        Err(error) => {
            assert_eq!(
                error.raw_os_error(),
                Some(libc::EILSEQ),
                "unexpected fixture filesystem error"
            );
            assert!(!output.status.success());
            assert_eq!(value(&output)["kind"], "io");
            assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
        }
    }
    let args = vec![
        OsString::from("registry"),
        "lookup".into(),
        "--module-hash".into(),
        OsString::from_vec(vec![0xff]),
        "--registry".into(),
        "local".into(),
        "--json".into(),
    ];
    let output = command_os(&args, root);
    assert!(!output.status.success());
    assert_eq!(value(&output)["kind"], "invalid_path");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    let args = vec![
        OsString::from("registry"),
        OsString::from_vec(vec![0xff]),
        "--json".into(),
    ];
    let output = command_os(&args, root);
    assert!(!output.status.success());
    assert_eq!(value(&output)["kind"], "invalid_path");
}

mod inspect_cli_tests {
    use super::*;

    const STANDARD_MODULE: &str =
        "e9aaa49f45bad5c889b86ee3341550c155cfdd10c3a6757de618d20612fffd52";
    const OBSERVED_PUZZLE: &str =
        "545e9487959a1b2f6c50b5567dc63193eba648d8007896ec52bcde427f310499";

    fn fixture() -> Value {
        serde_json::from_str(include_str!(
            "../../chialisp-oxide-package/tests/fixtures/mainnet-standard/expected.json"
        ))
        .unwrap()
    }

    fn module_hex() -> &'static str {
        include_str!(
            "../../chialisp-oxide-package/tests/fixtures/mainnet-standard/pinned-standard-module-hex.body"
        ).trim()
    }

    fn curry(module: &str, argument: &str) -> String {
        // Independent literal encoding of (a (q . module) (c (q . arg) 1)).
        format!("ff02ffff01{module}ffff04ffff01{argument}ff018080")
    }

    fn inspect(root: &Path, filename: &str, expected: Option<&str>) -> Output {
        let mut args = vec!["registry", "inspect", "--clvm-hex-file", filename, "--json"];
        if let Some(expected) = expected {
            args.extend(["--expected-puzzle-hash", expected]);
        }
        command(&args, root)
    }

    #[test]
    fn actual_pinned_standard_module_and_retained_instance_have_distinct_local_identities() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let expected = fixture();
        assert_eq!(expected["module_tree_hash"], STANDARD_MODULE);
        assert_eq!(expected["puzzle_tree_hash"], OBSERVED_PUZZLE);
        assert_eq!(expected["module_hex"], module_hex());
        fs::write(root.join("module.hex"), module_hex()).unwrap();
        fs::write(
            root.join("instance.hex"),
            expected["puzzle_reveal_hex"].as_str().unwrap(),
        )
        .unwrap();
        let raw = success(&inspect(root, "module.hex", Some(STANDARD_MODULE)));
        assert_eq!(raw["status"], "inspected");
        assert!(raw["module_lookup"].is_null());
        let raw = &raw["inspection"];
        assert_eq!(raw["form"], "recognized_raw_module");
        assert_eq!(raw["reveal"]["module_tree_hash"], STANDARD_MODULE);
        assert_eq!(raw["module"]["identity"], raw["reveal"]);
        assert_eq!(raw["module"]["clvm_hex"], module_hex());
        assert_eq!(raw["arguments"], json!([]));
        assert_eq!(raw["module_recognized"], true);
        assert_eq!(raw["standard_arguments"]["status"], "not_curried");

        let mut args = vec![
            "chialisp-oxide",
            "registry",
            "inspect",
            "--clvm-hex-file",
            "instance.hex",
            "--expected-puzzle-hash",
            OBSERVED_PUZZLE,
            "--json",
        ];
        let result = success(&command(&args, root));
        assert_eq!(result["status"], "inspected");
        assert!(result["module_lookup"].is_null());
        let inspection = &result["inspection"];
        assert_eq!(inspection["schema_version"], 1);
        assert_eq!(inspection["form"], "canonical_curry");
        assert_eq!(inspection["reveal"]["module_tree_hash"], OBSERVED_PUZZLE);
        assert_eq!(inspection["reveal"]["bytes"], 291);
        assert_eq!(
            inspection["reveal"]["blob_sha256"],
            expected["puzzle_reveal_blob_sha256"]
        );
        assert_eq!(
            inspection["module"]["identity"]["module_tree_hash"],
            STANDARD_MODULE
        );
        assert_eq!(inspection["module"]["identity"]["bytes"], 227);
        assert_eq!(
            inspection["module"]["identity"]["blob_sha256"],
            expected["module_blob_sha256"]
        );
        assert_ne!(
            inspection["reveal"]["module_tree_hash"],
            inspection["module"]["identity"]["module_tree_hash"]
        );
        assert_eq!(inspection["arguments"].as_array().unwrap().len(), 1);
        assert_eq!(
            inspection["arguments"][0]["clvm_hex"],
            format!("b0{}", expected["arguments"][0]["hex"].as_str().unwrap())
        );
        assert_eq!(inspection["standard_arguments"]["status"], "decoded");
        assert_eq!(
            inspection["standard_arguments"]["role"],
            "synthetic_public_key"
        );
        assert_eq!(
            inspection["standard_arguments"]["synthetic_public_key_hex"],
            expected["arguments"][0]["hex"]
        );
        assert_eq!(inspection["standard_arguments"]["key_is_infinity"], false);
        assert_eq!(inspection["module_recognized"], true);
        assert_eq!(
            inspection["known_module"]["name"],
            "p2_delegated_puzzle_or_hidden_puzzle"
        );
        // No network/coin-height claims may be smuggled into offline identity.
        for field in ["coin_id", "network", "height", "signature_verified"] {
            assert!(inspection.get(field).is_none());
        }
        args.extend(["--registry", "missing"]);
        failure(&command(&args, root), "io");
        assert!(!root.join("missing").exists());
    }

    #[test]
    fn generic_shapes_preserve_arguments_and_standard_schema_errors_are_successful_reports() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let generic = curry("ff0880", "ff0007"); // Quoted raising module, improper byte-zero argument.
        fs::write(root.join("generic.hex"), &generic).unwrap();
        let result = success(&inspect(root, "generic.hex", None));
        let inspection = &result["inspection"];
        assert_eq!(inspection["form"], "canonical_curry");
        assert_eq!(inspection["module"]["clvm_hex"], "ff0880");
        assert_eq!(inspection["arguments"][0]["clvm_hex"], "ff0007");
        assert_eq!(inspection["module_recognized"], false);
        assert!(inspection["known_module"].is_null());
        assert_eq!(inspection["standard_arguments"]["status"], "not_applicable");
        assert!(
            inspection.get("failure").is_none(),
            "Inspection must not execute the raising module"
        );
        for altered in [
            generic.replacen("ff02", "ff03", 1),
            generic.replacen("ff02", "ff820002", 1),
        ] {
            fs::write(root.join("unclassified.hex"), altered).unwrap();
            let result = success(&inspect(root, "unclassified.hex", None));
            assert_eq!(result["inspection"]["form"], "unclassified");
            assert!(result["inspection"]["module"].is_null());
        }
        let invalid = [
            (format!("ff02ffff01{}ff0180", module_hex()), "wrong_arity"),
            (curry(module_hex(), "ff0007"), "expected_atom"),
            (curry(module_hex(), "00"), "wrong_public_key_length"),
            (
                curry(module_hex(), &format!("b0{}", "00".repeat(48))),
                "invalid_public_key",
            ),
        ];
        for (hex, reason) in invalid {
            fs::write(root.join("invalid-key.hex"), hex).unwrap();
            let result = success(&inspect(root, "invalid-key.hex", None));
            assert_eq!(result["inspection"]["form"], "canonical_curry");
            assert_eq!(result["inspection"]["module_recognized"], true);
            assert_eq!(
                result["inspection"]["standard_arguments"],
                json!({"status":"invalid","reason":reason})
            );
        }
    }

    #[test]
    fn optional_module_lookup_never_substitutes_a_module_for_the_full_puzzle_hash() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let expected = fixture();
        let mut module = package();
        module["artifact"] = json!({"identity":{
            "encoding":"clvm-legacy-plain/v1","bytes":227,
            "blob_sha256":expected["module_blob_sha256"],"module_tree_hash":STANDARD_MODULE,
        },"clvm_hex":module_hex()});
        let id = seed(root, "module.json", &module, false);
        fs::write(
            root.join("instance.hex"),
            expected["puzzle_reveal_hex"].as_str().unwrap(),
        )
        .unwrap();
        let result = success(&command(
            &[
                "registry",
                "inspect",
                "--clvm-hex-file",
                "instance.hex",
                "--expected-puzzle-hash",
                OBSERVED_PUZZLE,
                "--registry",
                "local",
                "--json",
            ],
            root,
        ));
        assert_eq!(
            result["inspection"]["reveal"]["module_tree_hash"],
            OBSERVED_PUZZLE
        );
        assert_eq!(result["module_lookup"]["module_tree_hash"], STANDARD_MODULE);
        let packages = result["module_lookup"]["packages"].as_array().unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0]["object_id"], id);
        let exact = success(&command(
            &[
                "registry",
                "lookup",
                "--module-hash",
                OBSERVED_PUZZLE,
                "--registry",
                "local",
                "--json",
            ],
            root,
        ));
        assert_eq!(exact["packages"], json!([]));
        fs::write(root.join("generic.hex"), curry("ff0880", "80")).unwrap();
        let generic = success(&command(
            &[
                "registry",
                "inspect",
                "--clvm-hex-file",
                "generic.hex",
                "--registry",
                "local",
                "--json",
            ],
            root,
        ));
        assert_eq!(generic["inspection"]["module_recognized"], false);
        assert_eq!(
            generic["module_lookup"]["module_tree_hash"],
            generic["inspection"]["module"]["identity"]["module_tree_hash"]
        );
        assert_eq!(generic["module_lookup"]["packages"], json!([]));
        // Full reveal verification happens before any explicit registry read.
        failure(
            &command(
                &[
                    "registry",
                    "inspect",
                    "--clvm-hex-file",
                    "instance.hex",
                    "--expected-puzzle-hash",
                    STANDARD_MODULE,
                    "--registry",
                    "missing",
                    "--json",
                ],
                root,
            ),
            "digest_mismatch",
        );
        assert!(!root.join("missing").exists());
        fs::write(root.join("unknown.hex"), "80").unwrap();
        let unknown = success(&command(
            &[
                "registry",
                "inspect",
                "--clvm-hex-file",
                "unknown.hex",
                "--registry",
                "missing",
                "--json",
            ],
            root,
        ));
        assert_eq!(unknown["inspection"]["form"], "unclassified");
        assert!(unknown["module_lookup"].is_null());
        assert!(!root.join("missing").exists());
        fs::write(
            root.join("local/objects")
                .join(format!("{}.json", "ff".repeat(32))),
            b"{}",
        )
        .unwrap();
        failure(
            &command(
                &[
                    "registry",
                    "inspect",
                    "--clvm-hex-file",
                    "instance.hex",
                    "--registry",
                    "local",
                    "--json",
                ],
                root,
            ),
            "corrupt_object",
        );
    }

    #[test]
    fn hex_transport_expected_hash_and_closed_flags_fail_with_typed_host_errors() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for (text, kind) in [
            ("", "invalid_manifest"),
            ("0x", "invalid_manifest"),
            ("8", "invalid_manifest"),
            ("zz", "invalid_manifest"),
            ("80 80", "invalid_manifest"),
            ("ff", "malformed_serialization"),
            ("8080", "malformed_serialization"),
            ("8101", "non_canonical_serialization"),
            ("fe", "unsupported_encoding"),
        ] {
            fs::write(root.join("bad.hex"), text).unwrap();
            failure(&inspect(root, "bad.hex", None), kind);
            assert_eq!(fs::read_to_string(root.join("bad.hex")).unwrap(), text);
        }
        fs::write(root.join("input.hex"), "\n0X80\r\n").unwrap();
        let accepted = success(&inspect(root, "input.hex", Some(NIL_MODULE)));
        assert_eq!(
            accepted["inspection"]["reveal"]["module_tree_hash"],
            NIL_MODULE
        );
        failure(
            &inspect(root, "input.hex", Some(STANDARD_MODULE)),
            "digest_mismatch",
        );
        let uppercase = "A".repeat(64);
        for expected in ["wrong", &uppercase] {
            failure(
                &inspect(root, "input.hex", Some(expected)),
                "invalid_manifest",
            );
        }
        for tail in [
            vec!["--package", "input.json"],
            vec!["--module-hash", NIL_MODULE],
            vec!["--lock", "consumer.lock"],
            vec!["--object-id", STANDARD_MODULE],
            vec!["--clvm-hex-file", "other.hex"],
            vec!["--json"],
            vec!["--expected-puzzle-hash"],
        ] {
            let mut args = vec![
                "registry",
                "inspect",
                "--clvm-hex-file",
                "input.hex",
                "--json",
            ];
            args.extend(tail);
            failure(&command(&args, root), "invalid_manifest");
        }
        failure(
            &command(&["registry", "inspect", "--json"], root),
            "invalid_manifest",
        );
        assert!(!root.join("local").exists());
        assert!(!root.join("consumer.lock").exists());
        let help = success(&command(&["registry", "inspect", "--help", "--json"], root));
        assert!(
            help["usage"]
                .as_str()
                .unwrap()
                .contains("inspect --clvm-hex-file PATH")
        );
        assert!(
            help["usage"]
                .as_str()
                .unwrap()
                .contains("full reveal puzzle hash")
        );
    }

    #[test]
    fn nonregular_missing_and_oversized_hex_files_never_create_registry_state() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("directory.hex")).unwrap();
        failure(&inspect(root, "directory.hex", None), "invalid_path");
        failure(&inspect(root, "missing.hex", None), "io");
        let over = chialisp_oxide_package::Limits::default().max_blob_bytes * 2 + 5;
        fs::write(root.join("oversized.hex"), vec![b'0'; over]).unwrap();
        failure(&inspect(root, "oversized.hex", None), "resource_limit");
        assert!(!root.join("local").exists());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("missing.hex"), root.join("link.hex")).unwrap();
            failure(&inspect(root, "link.hex", None), "invalid_path");
            assert!(!root.join("missing.hex").exists());
        }
    }

    #[cfg(unix)]
    #[test]
    fn inspection_keeps_non_utf8_paths_and_rejects_non_utf8_hashes_without_tool_queries() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let filename = OsString::from_vec(b"reveal-\xff.hex".to_vec());
        let creation = fs::write(root.join(&filename), "80");
        let args = vec![
            "chialisp-oxide".into(),
            "registry".into(),
            "inspect".into(),
            "--clvm-hex-file".into(),
            filename,
            "--json".into(),
        ];
        let output = command_os(&args, root);
        match creation {
            Ok(()) => assert_eq!(
                success(&output)["inspection"]["reveal"]["module_tree_hash"],
                NIL_MODULE
            ),
            Err(error) => {
                assert_eq!(
                    error.raw_os_error(),
                    Some(libc::EILSEQ),
                    "unexpected fixture filesystem error"
                );
                failure(&output, "io");
            }
        }
        fs::write(root.join("input.hex"), "80").unwrap();
        let args = vec![
            "registry".into(),
            "inspect".into(),
            "--clvm-hex-file".into(),
            "input.hex".into(),
            "--expected-puzzle-hash".into(),
            OsString::from_vec(vec![0xff]),
            "--json".into(),
        ];
        failure(&command_os(&args, root), "invalid_path");
        assert!(!root.join("local").exists());
    }
}

#[path = "registry_offline/generate.rs"]
mod generate_cli_tests;
