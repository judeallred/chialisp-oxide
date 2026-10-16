use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[path = "support/vm.rs"]
mod vm;
use vm::execute;

#[test]
fn inconclusive_execution_cannot_satisfy_a_negative_source_test() {
    let program = chialisp_oxide_reference::compile("(mod () (q . 7))").unwrap();
    // This deliberately insufficient budget must fail the conclusive-test
    // boundary, rather than becoming a false boolean acceptance result.
    assert!(execute(&program, "()", 1).is_err());
    assert!(matches!(
        execute(&program, "()", 1_000_000).unwrap().outcome,
        chialisp_oxide_backend_experiment::Outcome::Accepted { .. }
    ));
}

fn compile(fixture: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_chialisp-oxide-driver"))
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(fixture),
        )
        .output()
        .expect("run rustc-backed driver")
}

#[test]
fn extracts_checked_rust_with_guards_tuples_and_helper_calls() {
    for fixture in [
        "guarded_payment.rs",
        "arithmetic.rs",
        "helpers.rs",
        "branches.rs",
        "bytes.rs",
        "unit.rs",
        "signed.rs",
        "constant_bytes.rs",
    ] {
        let output = compile(fixture);
        assert!(
            output.status.success(),
            "{fixture}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let program: chialisp_oxide_ir::Program =
            serde_json::from_slice(&output.stdout).expect("typed JSON IR");
        assert_eq!(program.name, "puzzle");
        assert!(!output.stdout.is_empty());
    }
}

#[test]
fn completes_rust_type_and_borrow_checking_before_output() {
    for (fixture, diagnostic) in [
        ("type_error.rs", "cannot add"),
        ("borrow_error.rs", "cannot borrow"),
    ] {
        let output = compile(fixture);
        assert!(!output.status.success(), "{fixture} unexpectedly accepted");
        assert!(
            output.stdout.is_empty(),
            "invalid Rust must never emit an IR artifact"
        );
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(
            errors.contains(diagnostic),
            "expected native Rust diagnostic: {errors}"
        );
    }
}

#[test]
fn rejects_features_without_faithful_semantics_with_source_locations() {
    for (fixture, diagnostic) in [
        ("loops.rs", "loops are unsupported"),
        ("references.rs", "unsupported Rust type"),
        ("casts.rs", "unsupported MIR value"),
        ("overflowing.rs", "external call"),
        ("wrapping.rs", "external call"),
        ("panic.rs", "external call"),
        ("generic.rs", "generic puzzle functions"),
        ("unsafe_fn.rs", "declaration of an `unsafe` function"),
        ("floating.rs", "unsupported Rust type"),
        ("division.rs", "unsupported integer operation"),
        (
            "ignored_parameter.rs",
            "requires a function with no parameters",
        ),
    ] {
        let output = compile(fixture);
        assert!(!output.status.success(), "{fixture} unexpectedly accepted");
        assert!(output.stdout.is_empty());
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(
            errors.contains(diagnostic),
            "{fixture}: expected {diagnostic:?}: {errors}"
        );
        assert!(
            errors.contains(fixture),
            "diagnostic needs a source location: {errors}"
        );
    }
}

#[test]
fn standalone_ignored_marker_uses_the_same_versioned_policy_as_cargo() {
    use chialisp_oxide_backend_experiment::Outcome;
    let output = compile("ignored_environment.rs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ir_version"], 2);
    assert!(json.get("name").is_none());
    let program: chialisp_oxide_ir::Program = serde_json::from_value(json).unwrap();
    assert_eq!(program.environment, chialisp_oxide_ir::Environment::Ignored);
    for optimize in [false, true] {
        let bytes = chialisp_oxide_ir::compile(&program, optimize).unwrap();
        assert!(matches!(
            execute(&bytes, "(1 . 2)", 1_000_000).unwrap().outcome,
            Outcome::Accepted { .. }
        ));
    }
}

#[test]
fn standalone_raw_tree_marker_preserves_the_whole_environment() {
    let output = compile("raw_environment.rs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ir_version"], 3);
    assert_eq!(json["environment"], "raw");
    assert!(json.get("name").is_none());
    let program: chialisp_oxide_ir::Program = serde_json::from_value(json).unwrap();
    let reference =
        chialisp_oxide_reference::compile("(mod (conditions) (qq (q . (unquote conditions))))")
            .unwrap();
    for optimize in [false, true] {
        let bytes = chialisp_oxide_ir::compile(&program, optimize).unwrap();
        for input in ["()", "0x0001", "(0x0001 . 2)", "((1 . 2) . (3 . 4))"] {
            assert_eq!(
                execute(&bytes, input, 1_000_000).unwrap().outcome,
                execute(&reference, input, 1_000_000).unwrap().outcome
            );
        }
    }
}

#[test]
fn unused_failing_helper_is_still_evaluated() {
    let output = compile("helpers.rs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let program: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(program["body"].to_string().contains("sequence"));
    assert!(program["body"].to_string().contains("reject"));
}

#[test]
fn compiled_vm_preserves_overflow_strictness_and_branch_laziness() {
    use chialisp_oxide_backend_experiment::Outcome;

    for (fixture, solution, accepted) in [
        ("helpers.rs", "(9)", false),
        ("helpers.rs", "(10)", true),
        ("unused_arithmetic.rs", "(18446744073709551615)", false),
        ("unused_arithmetic.rs", "(0)", true),
        ("lazy_arithmetic.rs", "(18446744073709551615 ())", true),
        ("lazy_arithmetic.rs", "(18446744073709551615 1)", false),
        (
            "signed.rs",
            "(-9223372036854775808 -9223372036854775808)",
            false,
        ),
        ("signed.rs", "(9223372036854775807 0)", true),
    ] {
        let output = compile(fixture);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let program: chialisp_oxide_ir::Program = serde_json::from_slice(&output.stdout).unwrap();
        for optimize in [false, true] {
            let bytecode = chialisp_oxide_ir::compile(&program, optimize).unwrap();
            let result = execute(&bytecode, solution, 10_000_000).unwrap();
            assert_eq!(
                matches!(result.outcome, Outcome::Accepted { .. }),
                accepted,
                "{fixture} optimize={optimize} solution={solution}: {result:?}"
            );
        }
    }
}

#[test]
fn real_rust_sources_match_native_chialisp_and_consensus_corpus() {
    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let scratch = Scratch(std::env::temp_dir().join(format!(
            "chialisp-oxide-cargo-corpus-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
    let driver = Path::new(env!("CARGO_BIN_EXE_chialisp-oxide-driver"));
    let cargo = driver.with_file_name("cargo-chialisp-oxide");
    assert!(
        cargo.is_file(),
        "run cargo build --workspace before testing Cargo corpus extraction"
    );
    let output = Command::new(cargo)
        .args(["build", "--manifest-path"])
        .arg(chialisp_oxide_corpus::root().join("puzzles/Cargo.toml"))
        .args(["--locked", "--offline", "--output-dir"])
        .arg(&scratch.0)
        .env("CHIALISP_OXIDE_DRIVER", driver)
        .output()
        .expect("run genuine Cargo corpus extraction");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bundle = cargo_chialisp_oxide::load_bundle(&scratch.0).unwrap();
    assert_eq!(bundle.manifest["package"], "chialisp-oxide-puzzles");
    assert_eq!(bundle.programs.len(), chialisp_oxide_corpus::PUZZLES.len());
    for slug in chialisp_oxide_corpus::PUZZLES {
        let name = format!("chialisp_oxide_puzzles::{}::puzzle", slug.replace('-', "_"));
        let program = bundle
            .programs
            .get(&name)
            .unwrap_or_else(|| panic!("missing Cargo export {name}"));
        let report = chialisp_oxide_corpus::validate(slug, program)
            .unwrap_or_else(|error| panic!("{slug}: {error:#}"));
        assert!(report.validated);
        assert!(!report.cases.is_empty());
        eprintln!(
            "{slug}: {} vectors, {} generated cases, {} ledger scenarios",
            report.cases.len(),
            report.generated_cases,
            report.simulator.len()
        );
    }
}

#[test]
fn bounds_substitution_expansion_before_emitting_an_artifact() {
    for fixture in ["expression_blowup.rs", "type_blowup.rs"] {
        let output = compile(fixture);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(errors.contains("expansion limit"), "{errors}");
        assert!(errors.contains(fixture), "{errors}");
    }
}

#[test]
fn unused_inputs_still_require_exact_canonical_typed_encodings() {
    use chialisp_oxide_backend_experiment::Outcome;

    let output = compile("unused_arguments.rs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let program: chialisp_oxide_ir::Program = serde_json::from_slice(&output.stdout).unwrap();
    for optimize in [false, true] {
        let bytecode = chialisp_oxide_ir::compile(&program, optimize).unwrap();
        for (solution, accepted) in [
            ("(1 1 0x0102 (2 ()))", true),
            ("(0x0080 () 0xffff (255 1))", true),
            ("(0x0001 1 0x0102 (2 ()))", false),
            ("(0x00 1 0x0102 (2 ()))", false),
            ("(1 0x00 0x0102 (2 ()))", false),
            ("(1 2 0x0102 (2 ()))", false),
            ("(1 (1) 0x0102 (2 ()))", false),
            ("(1 1 (1 2) (2 ()))", false),
            ("(1 1 0x01 (2 ()))", false),
            ("(1 1 0x0102 (2 (3)))", false),
            ("(1 1 0x0102 (2 () 4))", false),
            ("(1 1 0x0102 (2 . 1))", false),
            ("(1 1 0x0102 (2 ()) 9)", false),
            ("(1 1 0x0102 (2 ()) . 9)", false),
        ] {
            let result = execute(&bytecode, solution, 10_000_000).unwrap();
            assert_eq!(
                matches!(result.outcome, Outcome::Accepted { .. }),
                accepted,
                "optimize={optimize}, solution={solution}: {result:?}"
            );
        }
    }
}

#[test]
fn signed_minimum_and_maximum_overflow_reject_in_the_vm() {
    use chialisp_oxide_backend_experiment::Outcome;

    let output = compile("signed_overflow.rs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let program: chialisp_oxide_ir::Program = serde_json::from_slice(&output.stdout).unwrap();
    for optimize in [false, true] {
        let bytecode = chialisp_oxide_ir::compile(&program, optimize).unwrap();
        for (solution, accepted) in [
            ("(0 0)", true),
            ("(-3 2)", true),
            ("(-9223372036854775808 -1)", false),
            ("(-9223372036854775808 1)", false),
            ("(9223372036854775807 1)", false),
            ("(9223372036854775807 2)", false),
            ("(0xffffffffffffffffff 0)", false),
            ("(0xff8000000000000000 0)", false),
            ("(0x0001 0)", false),
        ] {
            let result = execute(&bytecode, solution, 10_000_000).unwrap();
            assert_eq!(
                matches!(result.outcome, Outcome::Accepted { .. }),
                accepted,
                "optimize={optimize}, solution={solution}: {result:?}"
            );
        }
    }
}
