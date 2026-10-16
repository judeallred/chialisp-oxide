use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        Self::with_sdk_alias("chialisp-oxide")
    }
    fn with_sdk_alias(alias: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "chialisp-oxide-cargo-driver-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir_all(path.join("src")).unwrap();
        let sdk = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../chialisp-oxide")
            .canonicalize()
            .unwrap();
        std::fs::write(path.join("Cargo.toml"), format!("[package]\nname=\"chialisp-oxide-driver-cargo-fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n[lib]\nname=\"wrapper_fixture\"\n[dependencies]\n{alias}={{package=\"chialisp-oxide\",path={}}}\n", serde_json::to_string(&sdk).unwrap())).unwrap();
        Self(path)
    }
    fn source(&self, source: &str) {
        std::fs::write(self.0.join("src/lib.rs"), source).unwrap();
    }
    fn compiler(&self) -> PathBuf {
        Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/rustc")
    }
    fn host(&self) -> String {
        let output = Command::new(self.compiler()).arg("-vV").output().unwrap();
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .unwrap()
            .into()
    }
    fn cargo(&self, wrapped: bool) -> std::process::Output {
        let mut command = Command::new(Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/cargo"));
        command
            .current_dir(&self.0)
            .env("RUSTC", self.compiler())
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER")
            .env_remove("CHIALISP_OXIDE_CARGO_MODE");
        command
            .args(["check", "--lib", "--offline", "--jobs", "1", "--target-dir"])
            .arg(self.0.join(if wrapped {
                "extract-build"
            } else {
                "native-build"
            }));
        if wrapped {
            command
                .args(["--target", &self.host()])
                .env("RUSTC_WRAPPER", env!("CARGO_BIN_EXE_chialisp-oxide-driver"))
                .env("CHIALISP_OXIDE_CARGO_MODE", "1")
                .env("CHIALISP_OXIDE_PRIMARY_PACKAGES", "wrapper_fixture")
                .env("CHIALISP_OXIDE_OUTPUT_DIR", self.0.join("exports"));
        }
        command.output().unwrap()
    }
    fn exports(&self) -> Vec<chialisp_oxide_ir::Program> {
        let dir = self.0.join("exports");
        if !dir.exists() {
            return Vec::new();
        }
        std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| {
                serde_json::from_slice(&std::fs::read(entry.unwrap().path()).unwrap()).unwrap()
            })
            .collect()
    }
    fn dependency(&self, crate_name: &str, encode_mir: bool) -> PathBuf {
        let source = self.0.join("external.rs");
        std::fs::write(&source, "pub fn identity(value:u64)->u64 { value }").unwrap();
        let mut command = Command::new(self.compiler());
        command
            .args([
                "--crate-name",
                crate_name,
                "--crate-type",
                "rlib",
                "--edition",
                "2024",
                "--emit",
                "metadata",
                "--out-dir",
            ])
            .arg(&self.0)
            .arg(source);
        if encode_mir {
            command.arg("-Zalways-encode-mir");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        self.0.join(format!("lib{crate_name}.rmeta"))
    }
    fn check_external(&self, alias: &str, metadata: &Path) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_chialisp-oxide-driver"))
            .arg("--cargo-wrapper")
            .arg(self.compiler())
            .args([
                "--crate-name",
                "wrapper_fixture",
                "--crate-type",
                "lib",
                "--edition",
                "2024",
                "--emit",
                "metadata",
                "--target",
                &self.host(),
                "--extern",
            ])
            .arg(format!("{alias}={}", metadata.display()))
            .arg("--out-dir")
            .arg(&self.0)
            .arg(self.0.join("src/lib.rs"))
            .env("CARGO_PRIMARY_PACKAGE", "1")
            .env("CHIALISP_OXIDE_OUTPUT_DIR", self.0.join("exports"))
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn cargo_dependencies_annotations_and_colliding_path_spellings_work() {
    let fixture = Fixture::new();
    fixture.source(r#"
        const PAYMENT_CAP: chialisp_oxide::Amount = chialisp_oxide::Amount::from_mojos(42);
        const RECIPIENT: chialisp_oxide::PuzzleHash = chialisp_oxide::PuzzleHash([0x42;32]);
        #[chialisp_oxide::puzzle]
        pub fn pay(destination: chialisp_oxide::PuzzleHash, amount: chialisp_oxide::Amount) -> chialisp_oxide::CreateCoin {
            assert!(amount.is_at_most(PAYMENT_CAP));
            chialisp_oxide::create_coin(destination, amount)
        }
        #[chialisp_oxide::puzzle]
        pub fn fixed_payment(amount: chialisp_oxide::Amount) -> chialisp_oxide::CreateCoin {
            chialisp_oxide::create_coin(RECIPIENT, chialisp_oxide::Amount::from_mojos(amount.mojos()))
        }
        #[chialisp_oxide::puzzle] pub fn a__b(value: u8) -> u8 { value + 1 }
        pub mod a { #[chialisp_oxide::puzzle] pub fn b(value: u8) -> u8 { value } }
        pub fn native_only() -> Vec<String> {
            [1, 2].iter().map(|value| format!("native {value}")).collect()
        }
    "#);
    let native = fixture.cargo(false);
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let output = fixture.cargo(true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "Cargo mode must not print IR on stdout"
    );
    let programs = fixture.exports();
    assert_eq!(
        programs
            .iter()
            .map(|program| program.name.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "wrapper_fixture::pay",
            "wrapper_fixture::fixed_payment",
            "wrapper_fixture::a__b",
            "wrapper_fixture::a::b"
        ])
    );
    for program in &programs {
        program.validate().unwrap();
        for optimize in [false, true] {
            let bytes = chialisp_oxide_ir::compile(program, optimize).unwrap();
            let solution = if program.name.ends_with("::pay") {
                format!("(0x{} 42)", "42".repeat(32))
            } else {
                "(41)".into()
            };
            let expected = if program.name.ends_with("::pay") {
                format!("(51 0x{} 42)", "42".repeat(32))
            } else if program.name.ends_with("::fixed_payment") {
                format!("(51 0x{} 41)", "42".repeat(32))
            } else if program.name.ends_with("::a__b") {
                "42".into()
            } else {
                "41".into()
            };
            let reference =
                chialisp_oxide_reference::compile(&format!("(mod () (q . {expected}))")).unwrap();
            assert_eq!(
                chialisp_oxide_backend_experiment::execute(&bytes, &solution, 1_000_000)
                    .unwrap()
                    .outcome,
                chialisp_oxide_backend_experiment::execute(&reference, "()", 1_000_000)
                    .unwrap()
                    .outcome,
            );
        }
    }
}

#[test]
fn renamed_dependency_keys_preserve_the_sdk_crate_identity_and_nominal_abi() {
    let fixture = Fixture::with_sdk_alias("clvm");
    fixture.source(
        r#"
        use clvm::{Amount, CreateCoin, PuzzleHash, create_coin};
        const CAP:Amount=Amount::from_mojos(42);
        const RECIPIENT:PuzzleHash=PuzzleHash([0x42;32]);
        #[clvm::puzzle]
        pub fn pay(amount:Amount)->CreateCoin {
            assert!(amount.is_at_most(CAP));
            create_coin(RECIPIENT,Amount::from_mojos(amount.mojos()))
        }
    "#,
    );
    for wrapped in [false, true] {
        let output = fixture.cargo(wrapped);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let programs = fixture.exports();
    assert_eq!(programs.len(), 1);
    assert_eq!(programs[0].name, "wrapper_fixture::pay");
    let expected =
        chialisp_oxide_reference::compile(&format!("(mod () (q . (51 0x{} 42)))", "42".repeat(32)))
            .unwrap();
    let expected = chialisp_oxide_backend_experiment::execute(&expected, "()", 1_000_000)
        .unwrap()
        .outcome;
    for optimize in [false, true] {
        let bytes = chialisp_oxide_ir::compile(&programs[0], optimize).unwrap();
        assert_eq!(
            chialisp_oxide_backend_experiment::execute(&bytes, "(42)", 1_000_000)
                .unwrap()
                .outcome,
            expected
        );
        for solution in ["(43)", "(0x0001)", "((42))"] {
            assert_eq!(
                chialisp_oxide_backend_experiment::execute(&bytes, solution, 1_000_000)
                    .unwrap()
                    .outcome,
                chialisp_oxide_backend_experiment::Outcome::Rejected
            );
        }
    }
}

#[test]
fn exported_unsupported_calls_and_invalid_annotation_targets_fail_closed() {
    for (source, diagnostic) in [
        (
            "#[chialisp_oxide::puzzle] pub fn wrapped(value:u64)->u64 { value.wrapping_add(1) }",
            "external call",
        ),
        (
            "#[chialisp_oxide::puzzle] type Callback = fn(u64)->u64;",
            "expected",
        ),
        (
            "#[chialisp_oxide::puzzle(invalid)] pub fn entry(value:u64)->u64 { value }",
            "does not accept arguments",
        ),
        (
            "#[chialisp_oxide::puzzle] #[chialisp_oxide::puzzle] pub fn entry(value:u64)->u64 { value }",
            "one annotation",
        ),
        (
            "#[chialisp_oxide::puzzle] pub extern \"C\" fn entry(value:u64)->u64 { value }",
            "ordinary Rust function ABI",
        ),
        (
            "#![feature(custom_mir)]\n#[chialisp_oxide::puzzle] pub fn entry(value:u64)->u64 { value }",
            "unstable feature",
        ),
        (
            "#[chialisp_oxide::puzzle] pub fn entry(value:u64)->u64 { value }\n\
             pub fn native_only() { let text=String::new(); let closure=||drop(text); closure(); closure(); }",
            "use of moved value: `closure`",
        ),
    ] {
        let fixture = Fixture::new();
        fixture.source(source);
        let output = fixture.cargo(true);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(diagnostic),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(fixture.exports().is_empty());
    }
}

#[test]
fn missing_dependency_mir_has_a_diagnostic_instead_of_an_ice() {
    let fixture = Fixture::new();
    let dependency = fixture.dependency("chialisp_oxide", false);
    fixture.source(
        "#[doc=\"__oxide_puzzle_v1\"] pub fn entry(value:u64)->u64 { chialisp_oxide::identity(value) }",
    );
    let output = fixture.check_external("chialisp_oxide", &dependency);
    assert!(!output.status.success());
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(errors.contains("MIR is unavailable"), "{errors}");
    assert!(!errors.contains("internal compiler error"), "{errors}");
    assert!(fixture.exports().is_empty());
}

#[test]
fn old_sdk_crate_identity_is_not_admitted_by_the_renamed_allowlist() {
    let fixture = Fixture::new();
    // Intentionally the former metadata identity, not an alias for the new SDK.
    let dependency = fixture.dependency("oxide_sdk", true);
    fixture.source(
        "#[doc=\"__oxide_puzzle_v1\"] pub fn entry(value:u64)->u64 { legacy::identity(value) }",
    );
    let output = fixture.check_external("legacy", &dependency);
    assert!(!output.status.success());
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(
        errors.contains("external call") && errors.contains("oxide_sdk"),
        "{errors}"
    );
    assert!(
        !errors.contains("MIR is unavailable") && !errors.contains("internal compiler error"),
        "{errors}"
    );
    assert!(fixture.exports().is_empty());
}

#[test]
fn external_sdk_units_cannot_enable_unsafe_or_custom_mir() {
    for (source, diagnostic) in [
        ("pub unsafe fn ignored() {}", "-F unsafe-code"),
        (
            "#![feature(custom_mir)]\npub fn identity(value:u64)->u64 { value }",
            "unstable feature",
        ),
    ] {
        let fixture = Fixture::new();
        let source_path = fixture.0.join("sdk.rs");
        std::fs::write(&source_path, source).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_chialisp-oxide-driver"))
            .arg("--cargo-wrapper")
            .arg(fixture.compiler())
            .args([
                "--crate-name",
                "chialisp_oxide",
                "--crate-type",
                "rlib",
                "--edition",
                "2024",
                "--emit",
                "metadata",
                "--target",
                &fixture.host(),
                "--out-dir",
            ])
            .arg(&fixture.0)
            .arg(source_path)
            .env_remove("CARGO_PRIMARY_PACKAGE")
            .output()
            .unwrap();
        assert!(!output.status.success());
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(errors.contains(diagnostic), "{errors}");
        assert!(!errors.contains("internal compiler error"), "{errors}");
    }
}

#[cfg(unix)]
#[test]
fn wrapper_rejects_a_different_compiler_before_loading_its_crates() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let compiler = fixture.0.join("fake-rustc");
    std::fs::write(
        &compiler,
        "#!/bin/sh\nprintf '%s\\n' 'rustc fake' 'commit-hash: wrong'\n",
    )
    .unwrap();
    std::fs::set_permissions(&compiler, std::fs::Permissions::from_mode(0o755)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_chialisp-oxide-driver"))
        .arg("--cargo-wrapper")
        .arg(compiler)
        .args(["--target", "fake-target", "--crate-name", "fixture"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("compiler commit mismatch"));
}
