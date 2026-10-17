mod support;
use std::{collections::BTreeSet, process::Command};
use support::Fixture;

#[path = "support/vm.rs"]
mod vm;
use vm::execute;

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
                execute(&bytes, &solution, 1_000_000).unwrap().outcome,
                execute(&reference, "()", 1_000_000).unwrap().outcome,
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
    let expected = execute(&expected, "()", 1_000_000).unwrap().outcome;
    for optimize in [false, true] {
        let bytes = chialisp_oxide_ir::compile(&programs[0], optimize).unwrap();
        assert_eq!(
            execute(&bytes, "(42)", 1_000_000).unwrap().outcome,
            expected
        );
        for solution in ["(43)", "(0x0001)", "((42))"] {
            assert_eq!(
                execute(&bytes, solution, 1_000_000).unwrap().outcome,
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
            "unknown #[puzzle] option",
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
fn mixed_environment_policies_preserve_defaults_failures_and_lazy_branches() {
    use chialisp_oxide_backend_experiment::Outcome;
    use chialisp_oxide_ir::Environment;

    let fixture = Fixture::new();
    fixture.source(
        r#"
        #[chialisp_oxide::puzzle]
        pub fn default_strict() -> u8 { 7 }
        #[chialisp_oxide::puzzle(environment = "strict")]
        pub fn explicit_strict() -> u8 { 7 }
        #[chialisp_oxide::puzzle(environment = "strict")]
        pub fn typed_unused(_: u8) -> u8 { 7 }
        #[chialisp_oxide::puzzle(environment = "ignored")]
        pub fn ignored() -> u8 { 7 }
        fn validate(value:u8) { assert!(value > 0); }
        fn increment(value:u8)->u8 { value + 1 }
        fn discard(_:u8)->u8 { 7 }
        fn choose()->bool { false }
        #[chialisp_oxide::puzzle(environment = "ignored")]
        pub fn failing_assert() -> u8 { validate(0); 7 }
        #[chialisp_oxide::puzzle(environment = "ignored")]
        pub fn failing_discarded_argument() -> u8 { discard(increment(255)) }
        #[chialisp_oxide::puzzle(environment = "ignored")]
        pub fn lazy() -> u8 { if choose() { discard(increment(255)) } else { 7 } }
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
    assert_eq!(programs.len(), 7);
    let reference = chialisp_oxide_reference::compile("(mod () (q . 7))").unwrap();
    let expected = execute(&reference, "()", 1_000_000).unwrap().outcome;
    // Include atoms, padded integer encodings, proper/improper lists, and
    // nested trees. These are environments, not a decoded argument list.
    let environments = [
        "()",
        "1",
        "-1",
        "0x00",
        "0x0001",
        "\"anything\"",
        "(1 2)",
        "(1 . 2)",
        "((1 . 2) (0x00 () (3 . 4)))",
    ];
    for program in &programs {
        let leaf = program.name.rsplit("::").next().unwrap();
        let ignored = matches!(
            leaf,
            "ignored" | "failing_assert" | "failing_discarded_argument" | "lazy"
        );
        assert_eq!(program.environment == Environment::Ignored, ignored);
        let json = serde_json::to_value(program).unwrap();
        if ignored {
            assert_eq!(json["ir_version"], 2);
            assert_eq!(json["environment"], "ignored");
            assert_eq!(json["program"]["name"], program.name);
            assert!(json.get("name").is_none());
        } else {
            assert!(json.get("ir_version").is_none());
            assert!(json.get("environment").is_none());
            assert_eq!(json["name"], program.name);
        }
        for optimize in [false, true] {
            let bytes = chialisp_oxide_ir::compile(program, optimize).unwrap();
            for environment in environments {
                let accepted = match leaf {
                    "ignored" | "lazy" => true,
                    "default_strict" | "explicit_strict" => environment == "()",
                    _ => false,
                };
                assert_eq!(
                    execute(&bytes, environment, 1_000_000).unwrap().outcome,
                    if accepted {
                        expected.clone()
                    } else {
                        Outcome::Rejected
                    },
                    "{leaf}, optimize={optimize}, environment={environment}"
                );
            }
            if leaf == "typed_unused" {
                for environment in ["(1)", "(255)"] {
                    assert_eq!(
                        execute(&bytes, environment, 1_000_000).unwrap().outcome,
                        expected
                    );
                }
                for environment in ["(256)", "(0x0001)", "((1))", "(1 2)"] {
                    assert_eq!(
                        execute(&bytes, environment, 1_000_000).unwrap().outcome,
                        Outcome::Rejected
                    );
                }
            }
        }
    }
    for optimize in [false, true] {
        let find = |name: &str| {
            programs
                .iter()
                .find(|program| program.name.ends_with(name))
                .unwrap()
        };
        assert_eq!(
            chialisp_oxide_ir::compile(find("::default_strict"), optimize).unwrap(),
            chialisp_oxide_ir::compile(find("::explicit_strict"), optimize).unwrap()
        );
    }
}

#[test]
fn annotation_options_and_ignored_parameters_have_native_compile_errors() {
    let fixture = Fixture::new();
    fixture.source(
        r#"
        #[chialisp_oxide::puzzle(unknown = "ignored")] pub fn unknown_option() {}
        #[chialisp_oxide::puzzle(environment = "other")] pub fn unknown_policy() {}
        #[chialisp_oxide::puzzle(environment = true)] pub fn not_a_string() {}
        #[chialisp_oxide::puzzle(environment = "strict", environment = "ignored")]
        pub fn duplicate_option() {}
        #[chialisp_oxide::puzzle(environment = "ignored")] pub fn typed(_:u8) {}
        #[chialisp_oxide::puzzle(environment = "ignored")] pub fn unit(_:()) {}
        #[chialisp_oxide::puzzle(environment = "ignored")] pub fn destructured(():()) {}
        "#,
    );
    let output = fixture.cargo(false);
    assert!(!output.status.success());
    let errors = String::from_utf8_lossy(&output.stderr);
    for diagnostic in [
        "unknown #[puzzle] option",
        "unknown environment policy",
        "expected string literal",
        "duplicate #[puzzle] environment option",
        "requires a function with no parameters",
    ] {
        assert!(errors.contains(diagnostic), "{diagnostic}: {errors}");
    }
    assert_eq!(
        errors
            .matches("requires a function with no parameters")
            .count(),
        3
    );
    assert!(fixture.exports().is_empty());
}

#[test]
fn conflicting_forged_or_misplaced_markers_prevent_every_cargo_export() {
    let fixture = Fixture::new();
    for (invalid, diagnostic) in [
        (
            r#"#[doc="__oxide_puzzle_v1"] #[doc="__oxide_puzzle_v1"] pub fn invalid() {}"#,
            "one annotation",
        ),
        (
            r#"#[doc="__chialisp_oxide_puzzle_v2_environment_ignored"] #[doc="__chialisp_oxide_puzzle_v2_environment_ignored"] pub fn invalid() {}"#,
            "one annotation",
        ),
        (
            r#"#[doc="__oxide_puzzle_v1"] #[doc="__chialisp_oxide_puzzle_v2_environment_ignored"] pub fn invalid() {}"#,
            "one annotation",
        ),
        (
            r#"#[doc="__oxide_puzzle_v1"] #[doc="__chialisp_oxide_puzzle_v3_raw"] pub fn invalid() {}"#,
            "unknown #[puzzle] export marker",
        ),
        (
            r#"#[doc="__chialisp_oxide_puzzle_v2_environment_ignored"] pub fn invalid(_:u8) {}"#,
            "requires a function with no parameters",
        ),
        (
            r#"#[doc="__chialisp_oxide_puzzle_v2_environment_ignored"] pub fn invalid(_:()) {}"#,
            "requires a function with no parameters",
        ),
        (
            r#"#[doc="__chialisp_oxide_puzzle_v2_environment_ignored"] pub struct Invalid;"#,
            "on a free function",
        ),
        (
            r#"pub struct Invalid; impl Invalid { #[doc="__oxide_puzzle_v1"] pub fn invalid() {} }"#,
            "on a free function",
        ),
        (
            r#"#[doc="__oxide_puzzle_v1"] pub const INVALID:u8=7;"#,
            "on a free function",
        ),
        (
            r#"#[chialisp_oxide::puzzle] #[chialisp_oxide::puzzle(environment="ignored")] pub fn invalid() {}"#,
            "one annotation",
        ),
    ] {
        fixture.source(&format!(
            "#[chialisp_oxide::puzzle] pub fn valid()->u8 {{7}}\n{invalid}"
        ));
        let output = fixture.cargo(true);
        assert!(!output.status.success(), "accepted {invalid}");
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(errors.contains(diagnostic), "{invalid}: {errors}");
        assert!(!errors.contains("internal compiler error"), "{errors}");
        assert!(fixture.exports().is_empty(), "partial export for {invalid}");
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
        .args(["--target", &fixture.host(), "--crate-name", "fixture"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("compiler commit mismatch"));
}
