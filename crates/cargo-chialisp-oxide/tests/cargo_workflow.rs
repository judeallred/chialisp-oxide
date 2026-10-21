//! Exercise genuine Cargo resolution, host build scripts, rustc checking, and CLVM.
use chialisp_oxide_backend_experiment::{FailureCode, Outcome, execute};
use chialisp_oxide_package::{
    Digest32, Limits, bundle::import_bundle, digest, package::SourceScope, store::Registry,
};
use serde_json::Value;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn invocation(
    root: &Path,
    scratch: &Path,
    fixture: &str,
    command: &str,
    extra: &[&str],
) -> Command {
    let executable = Path::new(env!("CARGO_BIN_EXE_cargo-chialisp-oxide"));
    let driver = executable.with_file_name("chialisp-oxide-driver");
    assert!(
        driver.is_file(),
        "run cargo build --workspace before this integration test"
    );
    let mut process = Command::new(executable);
    process
        .args(["chialisp-oxide", command, "--manifest-path"])
        .arg(root.join("examples").join(fixture).join("Cargo.toml"))
        .args(["--locked", "--offline", "--output-dir"])
        .arg(scratch.join("artifacts"))
        .args(extra)
        .env("CARGO_TARGET_DIR", scratch.join("target"))
        .env("CHIALISP_OXIDE_DRIVER", driver)
        // The compiler must be chosen by the toolchain pin, not this override.
        .env("RUSTC", "chialisp-oxide-test-nonexistent-rustc")
        .env("CARGO_BUILD_BUILD_DIR", scratch.join("user-intermediates"))
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CARGO_BUILD_RUSTC_WRAPPER")
        .env_remove("CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER");
    process
}

fn run(root: &Path, scratch: &Path, fixture: &str, command: &str, extra: &[&str]) -> Output {
    invocation(root, scratch, fixture, command, extra)
        .output()
        .expect("run cargo chialisp-oxide")
}

fn success(output: Output) {
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn manifest(scratch: &Path) -> (PathBuf, Value) {
    let output = scratch.join("artifacts");
    let bundle = cargo_chialisp_oxide::load_bundle(&output).unwrap();
    (bundle.manifest_path, bundle.manifest)
}

#[test]
fn nested_modules_export_same_leaf_names_without_identity_collisions() {
    let scratch = Scratch(std::env::temp_dir().join(format!(
            "chialisp-oxide-modules-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
    std::fs::create_dir_all(scratch.0.join("src")).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    std::fs::write(scratch.0.join("Cargo.toml"),format!("[package]\nname=\"nested-puzzles\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n[dependencies]\nchialisp-oxide={{path={}}}\n",serde_json::to_string(&root.join("crates/chialisp-oxide")).unwrap())).unwrap();
    std::fs::write(scratch.0.join("src/lib.rs"),"pub mod alpha { #[chialisp_oxide::puzzle] pub fn puzzle(value:u8)->u8 {value+1} } pub mod beta { pub mod nested { #[chialisp_oxide::puzzle] pub fn puzzle(value:u8)->u8 {value+2} } }").unwrap();
    let pin = include_str!("../../../rust-toolchain.toml")
        .lines()
        .find_map(|line| line.strip_prefix("channel = "))
        .unwrap()
        .trim_matches('"');
    success(
        Command::new("rustup")
            .args(["run", pin, "cargo", "generate-lockfile", "--offline"])
            .current_dir(&scratch.0)
            .output()
            .unwrap(),
    );
    let executable = Path::new(env!("CARGO_BIN_EXE_cargo-chialisp-oxide"));
    let output = scratch.0.join("artifacts");
    let search_path = std::env::var_os("PATH").unwrap();
    let search_path = std::env::join_paths(
        std::iter::once(executable.parent().unwrap().to_path_buf())
            .chain(std::env::split_paths(&search_path)),
    )
    .unwrap();
    let mut published = None;
    for subcommand in ["build", "check"] {
        // Exercise actual Cargo plugin dispatch and its injected first argument.
        success(
            Command::new("rustup")
                .args([
                    "run",
                    pin,
                    "cargo",
                    "chialisp-oxide",
                    subcommand,
                    "--manifest-path",
                ])
                .arg(scratch.0.join("Cargo.toml"))
                .args(["--locked", "--offline", "--output-dir"])
                .arg(&output)
                .env("PATH", &search_path)
                .env(
                    "CHIALISP_OXIDE_DRIVER",
                    executable.with_file_name("chialisp-oxide-driver"),
                )
                .output()
                .unwrap(),
        );
        let pointer = std::fs::read(output.join("latest.json")).unwrap();
        if let Some(previous) = &published {
            assert_eq!(previous, &pointer, "check changed the published bundle");
        } else {
            published = Some(pointer);
        }
    }
    let bundle = cargo_chialisp_oxide::load_bundle(&output).unwrap();
    assert_eq!(
        bundle
            .programs
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "nested_puzzles::alpha::puzzle",
            "nested_puzzles::beta::nested::puzzle"
        ]
    );
    assert_eq!(bundle.manifest["entries"].as_array().unwrap().len(), 2);
    let mut files = std::collections::BTreeSet::new();
    for entry in bundle.manifest["entries"].as_array().unwrap() {
        assert!(files.insert(entry["ir"].as_str().unwrap()));
        let name = entry["entry"].as_str().unwrap();
        let expected = if name.contains("::alpha::") {
            "2a"
        } else {
            "2b"
        };
        assert_eq!(
            execution(&bundle.manifest_path, &bundle.manifest, name, "(41)"),
            Outcome::Accepted {
                output_hex: expected.into()
            }
        );
        assert_eq!(
            execution(&bundle.manifest_path, &bundle.manifest, name, "(255)"),
            Outcome::Rejected
        );
    }
}

fn accepts(path: &Path, manifest: &Value, suffix: &str, solution: &str) -> bool {
    matches!(
        execution(path, manifest, suffix, solution),
        Outcome::Accepted { .. }
    )
}

fn execution(path: &Path, manifest: &Value, suffix: &str, solution: &str) -> Outcome {
    let entries = manifest["entries"].as_array().unwrap();
    let entry = entries
        .iter()
        .find(|entry| entry["entry"].as_str().unwrap().ends_with(suffix))
        .unwrap();
    let hex = std::fs::read_to_string(path.parent().unwrap().join(entry["clvm"].as_str().unwrap()))
        .unwrap();
    let execution = execute(&hex::decode(hex.trim()).unwrap(), solution, 10_000_000).unwrap();
    assert_ne!(
        execution.outcome,
        Outcome::Inconclusive,
        "Cargo artifact execution is inconclusive: {execution:?}"
    );
    if matches!(execution.outcome, Outcome::Rejected) {
        assert_eq!(
            execution.failure.unwrap().code(),
            FailureCode::Raise,
            "{execution:?}"
        );
    }
    execution.outcome
}

fn exact_outputs(path: &Path, manifest: &Value) {
    // Independent, canonical CLVM serialization of the expected condition tree:
    // ((73 1000) (51 0x4242...4242 101)). No compiler/SDK encoder is reused.
    let expected = format!("ffff49ff8203e880ffff33ffa0{}ff658080", "42".repeat(32));
    assert_eq!(
        execution(path, manifest, "::pay", "(101)"),
        Outcome::Accepted {
            output_hex: expected
        }
    );
    assert_eq!(
        execution(path, manifest, "::guarded_add", "(37 64)"),
        Outcome::Accepted {
            output_hex: "65".into()
        }
    );
}

fn offline_registry(scratch: &Path, args: &[OsString]) -> Value {
    let unavailable = scratch.join("nonexistent-build-tool");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-chialisp-oxide"))
        .args(args)
        .current_dir(scratch)
        .env("PATH", "")
        .env("CARGO", &unavailable)
        .env("RUSTC", &unavailable)
        .env("CHIALISP_OXIDE_DRIVER", &unavailable)
        .env("RUSTC_WRAPPER", &unavailable)
        .env("RUSTC_WORKSPACE_WRAPPER", &unavailable)
        .env("CARGO_BUILD_RUSTC_WRAPPER", &unavailable)
        .env("CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER", &unavailable)
        .output()
        .expect("run prebuilt offline registry executable");
    assert!(
        output.status.success(),
        "offline registry stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "offline registry emitted diagnostics"
    );
    serde_json::from_slice(&output.stdout).expect("one machine-readable registry result")
}

fn registry_round_trip(scratch: &Path, path: &Path, manifest: &Value) {
    // Reuse the actual source-produced bundle already compiled and executed
    // above. Registry admission itself adds no VM or source-equivalence credit.
    let scratch = scratch.canonicalize().unwrap();
    let entry = manifest["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["entry"].as_str().unwrap().ends_with("::pay"))
        .unwrap();
    let name = entry["entry"].as_str().unwrap();
    let artifact = &entry["artifact"];
    let module_hash: Digest32 = artifact["tree_hash"].as_str().unwrap().parse().unwrap();
    let clvm_file =
        std::fs::read_to_string(path.parent().unwrap().join(entry["clvm"].as_str().unwrap()))
            .unwrap();
    assert_eq!(
        clvm_file,
        format!("{}\n", artifact["clvm_hex"].as_str().unwrap())
    );
    let blob = hex::decode(clvm_file.trim_end()).unwrap();
    let ir_bytes =
        std::fs::read(path.parent().unwrap().join(entry["ir"].as_str().unwrap())).unwrap();
    let manifest_bytes = std::fs::read(path).unwrap();
    let package = import_bundle(path, name, &Limits::default()).unwrap();
    assert_eq!(package.module_tree_hash(), module_hash);
    assert_eq!(package.artifact().bytes(), blob.as_slice());
    assert_eq!(package.artifact().identity().blob_sha256, digest(&blob));
    assert_eq!(
        package.artifact().identity().bytes,
        artifact["bytes"].as_u64().unwrap()
    );
    let rust = package.manifest().rust.as_ref().unwrap();
    assert_eq!(rust.entry, name);
    assert_eq!(rust.ir_json.as_bytes(), ir_bytes.as_slice());
    assert_eq!(rust.ir_sha256, digest(&ir_bytes));
    assert_eq!(
        rust.bundle_manifest_json.as_bytes(),
        manifest_bytes.as_slice()
    );
    assert_eq!(rust.bundle_manifest_sha256, digest(&manifest_bytes));
    assert_eq!(package.manifest().source_scope, SourceScope::NotRecorded);
    assert!(package.manifest().sources.is_empty());
    let expected = serde_json::to_value(package.summary()).unwrap();
    assert_eq!(expected["source_scope"], "not_recorded");
    assert_eq!(expected["source_files"], 0);

    let store = scratch.join("registry");
    let published = offline_registry(
        &scratch,
        &[
            "registry".into(),
            "publish".into(),
            "--bundle".into(),
            path.as_os_str().to_owned(),
            "--entry".into(),
            name.into(),
            "--registry".into(),
            store.as_os_str().to_owned(),
            "--json".into(),
        ],
    );
    assert_eq!(published["publication"]["status"], "published");
    assert_eq!(
        published["publication"]["object_id"],
        package.object_id().to_hex()
    );
    assert_eq!(published["package"], expected);
    let stored_path = store
        .join("objects")
        .join(format!("{}.json", package.object_id()));
    assert_eq!(std::fs::read(&stored_path).unwrap(), package.bytes());
    let verified = offline_registry(
        &scratch,
        &[
            "registry".into(),
            "verify".into(),
            "--package".into(),
            stored_path.as_os_str().to_owned(),
            "--json".into(),
        ],
    );
    assert_eq!(verified["status"], "verified");
    assert_eq!(verified["package"], expected);
    let found = offline_registry(
        &scratch,
        &[
            "registry".into(),
            "lookup".into(),
            "--module-hash".into(),
            module_hash.to_hex().into(),
            "--registry".into(),
            store.as_os_str().to_owned(),
            "--json".into(),
        ],
    );
    assert_eq!(found["status"], "lookup_complete");
    assert_eq!(found["module_tree_hash"], module_hash.to_hex());
    assert_eq!(
        found["packages"].as_array().unwrap().as_slice(),
        std::slice::from_ref(&expected)
    );
    // Exercise Cargo's injected prefix without invoking Cargo on the offline
    // path. The prebuilt executable must dispatch before driver/wrapper checks.
    let retrieved = offline_registry(
        &scratch,
        &[
            "chialisp-oxide".into(),
            "registry".into(),
            "get".into(),
            "--object-id".into(),
            package.object_id().to_hex().into(),
            "--registry".into(),
            store.as_os_str().to_owned(),
            "--json".into(),
        ],
    );
    assert_eq!(retrieved["status"], "verified");
    assert_eq!(retrieved["package"], expected);
    let registry = Registry::open(&store, Limits::default()).unwrap();
    assert_eq!(
        registry.get(package.object_id()).unwrap().bytes(),
        package.bytes()
    );
    let variants = registry.lookup(module_hash).unwrap();
    assert_eq!(variants.len(), 1);
    assert_eq!(variants[0].bytes(), package.bytes());
    assert_eq!(std::fs::read(path).unwrap(), manifest_bytes);
    assert_eq!(
        std::fs::read(path.parent().unwrap().join(entry["ir"].as_str().unwrap())).unwrap(),
        ir_bytes
    );
}

#[test]
fn real_cargo_annotations_features_cache_and_failure_boundaries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let scratch = Scratch(std::env::temp_dir().join(
        format!("chialisp-oxide-cargo-{}-{}", std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()),
    ));
    std::fs::create_dir_all(&scratch.0).unwrap();
    let user_build_dir = scratch.0.join("user-intermediates");
    std::fs::create_dir(&user_build_dir).unwrap();
    std::fs::write(user_build_dir.join("sentinel"), "keep native intermediates").unwrap();
    success(run(&root, &scratch.0, "cargo-puzzles", "build", &[]));
    let (first_path, first) = manifest(&scratch.0);
    assert_eq!(first["entries"].as_array().unwrap().len(), 2);
    assert_eq!(std::fs::read_dir(&user_build_dir).unwrap().count(), 1);
    assert!(accepts(&first_path, &first, "::pay", "(101)"));
    assert!(accepts(&first_path, &first, "::guarded_add", "(101 0)"));
    exact_outputs(&first_path, &first);
    assert!(!accepts(&first_path, &first, "::pay", "(1001)"));
    assert!(!accepts(&first_path, &first, "::pay", "(0x0001)"));
    registry_round_trip(&scratch.0, &first_path, &first);
    // Same source/features with dependencies already fresh still regenerates
    // exports, rather than reporting success using an old IR file.
    success(run(&root, &scratch.0, "cargo-puzzles", "build", &[]));
    let (second_path, second) = manifest(&scratch.0);
    assert_ne!(first_path, second_path);
    assert_eq!(first["entries"], second["entries"]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let shim = scratch.0.join("forward-driver.sh");
        std::fs::write(&shim, "#!/bin/sh\n# distinct tested wrapper identity\nexec \"$CHIALISP_OXIDE_TEST_REAL_DRIVER\" \"$@\"\n").unwrap();
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
        success(
            invocation(&root, &scratch.0, "cargo-puzzles", "build", &[])
                .env("CHIALISP_OXIDE_DRIVER", &shim)
                .env(
                    "CHIALISP_OXIDE_TEST_REAL_DRIVER",
                    Path::new(env!("CARGO_BIN_EXE_cargo-chialisp-oxide"))
                        .with_file_name("chialisp-oxide-driver"),
                )
                .output()
                .unwrap(),
        );
        let (_, shim_manifest) = manifest(&scratch.0);
        assert_ne!(first["driver_sha256"], shim_manifest["driver_sha256"]);
        assert_eq!(first["entries"], shim_manifest["entries"]);
        for hash in [&first["driver_sha256"], &shim_manifest["driver_sha256"]] {
            assert!(
                scratch
                    .0
                    .join("target/chialisp-oxide/cargo")
                    .join(hash.as_str().unwrap())
                    .is_dir()
            );
        }
    }
    success(run(
        &root,
        &scratch.0,
        "cargo-puzzles",
        "build",
        &["--features", "strict-cap"],
    ));
    let (strict_path, strict) = manifest(&scratch.0);
    assert!(!accepts(&strict_path, &strict, "::pay", "(101)"));
    assert!(!accepts(&strict_path, &strict, "::guarded_add", "(101 0)"));
    assert!(accepts(&strict_path, &strict, "::pay", "(100)"));
    success(run(
        &root,
        &scratch.0,
        "cargo-puzzles",
        "build",
        &["--no-optimize"],
    ));
    let (plain_path, plain) = manifest(&scratch.0);
    assert!(accepts(&plain_path, &plain, "::pay", "(101)"));
    exact_outputs(&plain_path, &plain);
    assert!(!accepts(&plain_path, &plain, "::guarded_add", "(1001 0)"));
    // Cleaning only dev would leave the second release invocation fresh and
    // skip the wrapper. Invalidate the selected profile too.
    for _ in 0..2 {
        success(run(
            &root,
            &scratch.0,
            "cargo-puzzles",
            "build",
            &["--release"],
        ));
        let (path, report) = manifest(&scratch.0);
        exact_outputs(&path, &report);
    }
    let latest = std::fs::read(scratch.0.join("artifacts/latest.json")).unwrap();
    success(run(&root, &scratch.0, "cargo-puzzles", "check", &[]));
    assert_eq!(
        latest,
        std::fs::read(scratch.0.join("artifacts/latest.json")).unwrap()
    );
    for (extra, diagnostic) in [
        (vec![], "unsupported integer operation"),
        (vec!["--features", "type-error"], "mismatched types"),
    ] {
        let output = run(&root, &scratch.0, "cargo-invalid", "build", &extra);
        assert!(!output.status.success());
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(errors.contains(diagnostic), "{errors}");
        assert_eq!(
            latest,
            std::fs::read(scratch.0.join("artifacts/latest.json")).unwrap()
        );
    }
    // Cargo searches configuration from the working directory, even when an
    // absolute manifest path points elsewhere. Reject these wrappers explicitly.
    std::fs::create_dir_all(scratch.0.join(".cargo")).unwrap();
    for key in ["rustc-wrapper", "rustc-workspace-wrapper"] {
        std::fs::write(
            scratch.0.join(".cargo/config.toml"),
            format!("[build]\n{key} = '/nonexistent-wrapper'\n"),
        )
        .unwrap();
        let output = invocation(&root, &scratch.0, "cargo-puzzles", "check", &[])
            .current_dir(&scratch.0)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(&format!("configured build.{key}"))
        );
    }
    std::fs::write(
        scratch.0.join(".cargo/config.toml"),
        "[build]\ntarget = 'wasm32-unknown-unknown'\n",
    )
    .unwrap();
    for _ in 0..2 {
        success(
            invocation(&root, &scratch.0, "cargo-puzzles", "check", &[])
                .current_dir(&scratch.0)
                .output()
                .unwrap(),
        );
    }
    assert_eq!(std::fs::read_dir(user_build_dir).unwrap().count(), 1);
}
