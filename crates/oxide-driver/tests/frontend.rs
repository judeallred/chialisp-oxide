use std::{path::Path, process::Command};

fn compile(fixture: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_oxide-driver"))
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
        let program: oxide_ir::Program =
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
        ("panic.rs", "generic call"),
        ("generic.rs", "generic puzzle functions"),
        ("unsafe_fn.rs", "declaration of an `unsafe` function"),
        ("floating.rs", "unsupported Rust type"),
        ("division.rs", "unsupported integer operation"),
        ("recursion.rs", "recursion"),
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
    use oxide_backend_experiment::{Outcome, execute};

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
        let program: oxide_ir::Program = serde_json::from_slice(&output.stdout).unwrap();
        for optimize in [false, true] {
            let bytecode = oxide_ir::compile(&program, optimize).unwrap();
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
    for slug in oxide_corpus::PUZZLES {
        let output = Command::new(env!("CARGO_BIN_EXE_oxide-driver"))
            .arg(
                oxide_corpus::root()
                    .join("puzzles")
                    .join(slug)
                    .join("puzzle.rs"),
            )
            .output()
            .expect("run rustc-backed driver on corpus source");
        assert!(
            output.status.success(),
            "{slug}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let program: oxide_ir::Program = serde_json::from_slice(&output.stdout).unwrap();
        let report = oxide_corpus::validate(slug, &program)
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
    use oxide_backend_experiment::{Outcome, execute};

    let output = compile("unused_arguments.rs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let program: oxide_ir::Program = serde_json::from_slice(&output.stdout).unwrap();
    for optimize in [false, true] {
        let bytecode = oxide_ir::compile(&program, optimize).unwrap();
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
    use oxide_backend_experiment::{Outcome, execute};

    let output = compile("signed_overflow.rs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let program: oxide_ir::Program = serde_json::from_slice(&output.stdout).unwrap();
    for optimize in [false, true] {
        let bytecode = oxide_ir::compile(&program, optimize).unwrap();
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
