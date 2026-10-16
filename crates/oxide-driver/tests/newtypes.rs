use std::{path::Path, process::Command};

use oxide_backend_experiment::{Outcome, execute};

#[path = "fixtures/newtype_arithmetic.rs"]
mod native_arithmetic;
#[path = "fixtures/newtype_nested.rs"]
mod native_nested;
#[path = "fixtures/newtype_roundtrip.rs"]
mod native_roundtrip;
#[path = "fixtures/newtype_sdk.rs"]
mod native_sdk;

fn compile(fixture: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_oxide-driver"))
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(fixture),
        )
        .output()
        .expect("run the Rust frontend")
}

fn extract(fixture: &str) -> oxide_ir::Program {
    let output = compile(fixture);
    assert!(
        output.status.success(),
        "{fixture}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("validated typed IR")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn quoted_output(value: &str) -> Outcome {
    let reference = oxide_chialisp::compile(&format!("(mod () (q . {value}))")).unwrap();
    execute(&reference, "()", 1_000_000).unwrap().outcome
}

#[test]
fn rustc_rejects_domain_swaps_before_ir_lowering() {
    for (fixture, names) in [
        ("newtype_swap.rs", ["PuzzleHash", "CoinId"]),
        ("newtype_time_swap.rs", ["Timestamp", "Amount"]),
    ] {
        let output = compile(fixture);
        assert!(!output.status.success(), "{fixture} accepted a domain swap");
        assert!(output.stdout.is_empty());
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(
            errors.contains("E0308"),
            "expected Rust's type error: {errors}"
        );
        for name in names {
            assert!(errors.contains(name), "{errors}");
        }
    }
}

#[test]
fn rejects_layouts_and_semantics_that_cannot_be_erased() {
    for (fixture, diagnostic) in [
        (
            "newtype_plain.rs",
            "local or oxide_sdk #[repr(transparent)] structs with exactly one field",
        ),
        (
            "newtype_multifield.rs",
            "local or oxide_sdk #[repr(transparent)] structs with exactly one field",
        ),
        ("newtype_drop.rs", "custom Drop"),
        ("newtype_unsupported.rs", "unsupported Rust type"),
        ("newtype_generic.rs", "generic transparent newtypes"),
        ("newtype_macro.rs", "macro-generated newtype"),
        ("newtype_zero_sized.rs", "zero-sized transparent newtypes"),
        (
            "newtype_external_inner.rs",
            "local or oxide_sdk #[repr(transparent)] structs with exactly one field",
        ),
    ] {
        let output = compile(fixture);
        assert!(
            !output.status.success(),
            "{fixture} accepted an unsupported wrapper"
        );
        assert!(output.stdout.is_empty());
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(errors.contains(diagnostic), "{fixture}: {errors}");
        assert!(
            errors.contains(fixture),
            "source location missing: {errors}"
        );
    }
}

#[test]
fn nominal_byte_domains_roundtrip_with_the_same_bytes32_abi() {
    let program = extract("newtype_roundtrip.rs");
    assert!(
        program
            .parameters
            .iter()
            .all(|parameter| parameter.ty == oxide_ir::Type::Bytes { len: 32 })
    );
    for optimize in [false, true] {
        let bytes = oxide_ir::compile(&program, optimize).unwrap();
        for (hash, coin) in [
            ([0; 32], [255; 32]),
            ([128; 32], [1; 32]),
            (std::array::from_fn(|i| i as u8), [42; 32]),
        ] {
            let native = native_roundtrip::puzzle(
                native_roundtrip::PuzzleHash(hash),
                native_roundtrip::CoinId(coin),
            );
            let expected =
                quoted_output(&format!("(0x{} 0x{})", hex(&native.0.0), hex(&native.1.0)));
            let solution = format!("(0x{} 0x{})", hex(&hash), hex(&coin));
            assert_eq!(
                execute(&bytes, &solution, 1_000_000).unwrap().outcome,
                expected
            );
        }
        for solution in [
            format!("(0x{} 0x{})", "42".repeat(31), "17".repeat(32)),
            format!("(0x{} 0x{})", "42".repeat(32), "17".repeat(33)),
            "((1) ())".into(),
            "()".into(),
        ] {
            assert_eq!(
                execute(&bytes, &solution, 1_000_000).unwrap().outcome,
                Outcome::Rejected
            );
        }
    }
}

#[test]
fn sdk_owned_helpers_preserve_native_conditions_and_checked_amounts() {
    let program = extract("newtype_sdk.rs");
    for optimize in [false, true] {
        let bytes = oxide_ir::compile(&program, optimize).unwrap();
        for amount in [0, 42, 1000] {
            let hash = [0x42; 32];
            let native = native_sdk::puzzle(
                native_sdk::oxide::PuzzleHash(hash),
                native_sdk::oxide::Amount(amount),
            );
            let expected = quoted_output(&format!(
                "(({} 0x{} {}) ({} {}))",
                native.0.0,
                hex(&native.0.1.0),
                native.0.2.0,
                native.1.0,
                native.1.1.0
            ));
            assert_eq!(
                execute(&bytes, &format!("(0x{} {amount})", hex(&hash)), 1_000_000)
                    .unwrap()
                    .outcome,
                expected
            );
        }
        for amount in ["1001", "18446744073709551616", "-1", "0x0001"] {
            assert_eq!(
                execute(
                    &bytes,
                    &format!("(0x{} {amount})", "42".repeat(32)),
                    1_000_000
                )
                .unwrap()
                .outcome,
                Outcome::Rejected
            );
        }
    }
}

#[test]
fn nested_wrappers_erase_only_the_wrapper_fields() {
    let program = extract("newtype_nested.rs");
    assert_eq!(
        program.parameters[0].ty,
        oxide_ir::Type::Tuple {
            items: vec![
                oxide_ir::Type::Int {
                    bits: 8,
                    signed: false
                },
                oxide_ir::Type::Bool
            ]
        }
    );
    for optimize in [false, true] {
        let bytes = oxide_ir::compile(&program, optimize).unwrap();
        for (number, enabled) in [(0, false), (128, true), (255, false)] {
            let native = native_nested::puzzle(native_nested::Outer(native_nested::Inner((
                number, enabled,
            ))));
            let expected = quoted_output(&format!(
                "({} {})",
                native.0.0.0,
                if native.0.0.1 { "1" } else { "()" }
            ));
            let solution = format!("(({number} {}))", if enabled { "1" } else { "()" });
            assert_eq!(
                execute(&bytes, &solution, 1_000_000).unwrap().outcome,
                expected
            );
        }
    }
}

#[test]
fn wrapper_field_assignment_keeps_checked_integer_semantics() {
    let program = extract("newtype_arithmetic.rs");
    for optimize in [false, true] {
        let bytes = oxide_ir::compile(&program, optimize).unwrap();
        for value in [0, 42, u64::MAX - 1] {
            let native = native_arithmetic::puzzle(native_arithmetic::Count(value));
            let expected = quoted_output(&native.0.to_string());
            assert_eq!(
                execute(&bytes, &format!("({value})"), 1_000_000)
                    .unwrap()
                    .outcome,
                expected
            );
        }
        assert_eq!(
            execute(&bytes, "(18446744073709551615)", 1_000_000)
                .unwrap()
                .outcome,
            Outcome::Rejected
        );
    }
}
