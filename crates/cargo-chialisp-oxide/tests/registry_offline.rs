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
