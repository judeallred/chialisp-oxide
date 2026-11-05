//! Actual source closed move captures; existing v9 explicit-env elaboration.
#[path = "support/callable_fixture.rs"]
mod callable_fixture;
mod support;
#[path = "support/vm.rs"]
mod vm;
use callable_fixture::{expected, fixture, native};
use chialisp_oxide_backend_experiment::{FailureCode, Outcome};
use std::collections::BTreeMap;
const SOURCE: &str = include_str!("fixtures/captured_closures.rs");
#[test]
fn move_capture_source_matches_native_and_independent_languages_both_modes() {
    let policy = fixture(SOURCE);
    native(&policy, 3);
    let result = policy.cargo(true);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let exports = policy.exports();
    assert_eq!(exports.len(), 10);
    let abi_control = fixture(include_str!("fixtures/captured_closures_abi_control.rs"));
    let native_control = abi_control.cargo(false);
    assert!(
        native_control.status.success(),
        "native ABIcontrol:{}",
        String::from_utf8_lossy(&native_control.stderr)
    );
    let control = abi_control.cargo(true);
    assert!(
        control.status.success(),
        "ABIcontrol:{}",
        String::from_utf8_lossy(&control.stderr)
    );
    let controls = abi_control.exports();
    assert_eq!(controls.len(), 1);
    let control = &controls[0];
    assert!(control.uses_callables());
    assert_eq!(serde_json::to_value(control).unwrap()["ir_version"], 9);
    let references: BTreeMap<String, String> =
        serde_json::from_str(include_str!("fixtures/captured_closures.chialisp.json")).unwrap();
    let rue: BTreeMap<String, String> =
        serde_json::from_str(include_str!("fixtures/captured_closures.rue.json")).unwrap();
    for program in exports {
        program.validate().unwrap();
        let name = program.name.rsplit("::").next().unwrap();
        assert!(program.uses_callables(), "{name}");
        assert_eq!(serde_json::to_value(&program).unwrap()["ir_version"], 9);
        let reference = chialisp_oxide_reference::compile(&references[name]).unwrap();
        let mut independent = vec![reference];
        if let Some(source) = rue.get(name) {
            let path = policy.0.join(format!("{name}.rue"));
            std::fs::write(&path, source).unwrap();
            for optimize in [false, true] {
                independent.push(chialisp_oxide_rue::compile_file(&path, optimize).unwrap());
            }
        }
        let cases: &[(&str, &str)] = match name {
            "fresh_frames" => &[("(7 100)", "(8 101)"), ("(100 7)", "(101 8)")],
            "snapshot" => &[("(7)", "(8 9 107)"), ("(0)", "(1 2 100)")],
            "copies" => &[("(7)", "(9 10 28)"), ("(0)", "(2 3 ())")],
            "capture_order" => &[("(7 2)", "12"), ("(100 255)", "358")],
            "fixed" => &[("(0x01030507)", "5"), ("(0x008000ff)", "130")],
            "whole_tuple" => &[("((7 3))", "(7 3)"), ("((0 255))", "(() 255)")],
            "borrowed" => &[
                ("(())", "()"),
                ("(0x00)", "0x00"),
                ("(0x0080ff)", "0x0080ff"),
                ("((1 . 7))", "(1 . 7)"),
            ],
            "unused_capture" => &[("(1)", "99")],
            "lazy_body" => &[("(7)", "99"), ("(0)", "99")],
            "call_order" => &[],
            _ => panic!("{name}"),
        };
        for optimize in [false, true] {
            let artifact = chialisp_oxide_ir::compile(&program, optimize).unwrap();
            for &(input, output) in cases {
                let output = expected(output);
                for code in std::iter::once(&artifact).chain(&independent) {
                    assert_eq!(
                        vm::execute(code, input, 20_000_000).unwrap().outcome,
                        output,
                        "{name}/{optimize}/{input}"
                    );
                }
            }
            for input in match name {
                "unused_capture" => vec!["(())"],
                "call_order" => vec!["(7 ())", "(0 ())"],
                _ => vec![],
            } {
                for code in std::iter::once(&artifact).chain(&independent) {
                    let execution = vm::execute(code, input, 20_000_000).unwrap();
                    assert_eq!(execution.outcome, Outcome::Rejected);
                    assert_eq!(
                        execution.failure.unwrap().code(),
                        FailureCode::Raise,
                        "{name}/{optimize}/{input}"
                    );
                }
            }
            if name == "snapshot" {
                for malformed in [
                    "()",
                    "(1 2)",
                    "(0x0007)",
                    "(-1)",
                    "(18446744073709551616)",
                    "(18446744073709551615)",
                ] {
                    let e = vm::execute(&artifact, malformed, 20_000_000).unwrap();
                    assert_eq!(e.outcome, Outcome::Rejected);
                    assert_eq!(
                        program.parameters.iter().map(|p| &p.ty).collect::<Vec<_>>(),
                        control.parameters.iter().map(|p| &p.ty).collect::<Vec<_>>()
                    );
                    assert_eq!(program.environment, control.environment);
                    assert_eq!(program.result, control.result);
                    let old_code = chialisp_oxide_ir::compile(control, optimize).unwrap();
                    let old = vm::execute(&old_code, malformed, 20_000_000).unwrap();
                    let failure = e.failure.unwrap();
                    assert!(failure.is_program_rejection());
                    assert_eq!(
                        old.outcome,
                        Outcome::Rejected,
                        "priorv9/{optimize}/{malformed}"
                    );
                    assert_eq!(
                        failure,
                        old.failure.unwrap(),
                        "same public v9 ABI/{optimize}/{malformed}"
                    );
                    let expected = if !optimize && malformed == "(1 2)" {
                        FailureCode::InvalidOpArg
                    } else {
                        FailureCode::Raise
                    };
                    assert_eq!(
                        failure.code(),
                        expected,
                        "measured priorv9/{optimize}/{malformed}"
                    );
                    println!(
                        "same-v9 ABI snapshot/{optimize}/{malformed}: {:?}",
                        failure.code()
                    );
                }
            }
        }
    }
}
#[test]
fn native_valid_capturing_extensions_remain_closed() {
    let sources: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/captured_closures.excluded.json")).unwrap();
    for row in sources {
        let label = row["label"].as_str().unwrap();
        let fixture = fixture(row["source"].as_str().unwrap());
        let native = fixture.cargo(false);
        assert!(
            native.status.success(),
            "native {label}:{}",
            String::from_utf8_lossy(&native.stderr)
        );
        let lowered = fixture.cargo(true);
        assert!(!lowered.status.success(), "excluded {label} accepted");
        let error = String::from_utf8_lossy(&lowered.stderr);
        assert!(
            error.contains("chialisp-oxide") && !error.contains("internal compiler error"),
            "{label}:{error}"
        );
        assert!(fixture.exports().is_empty(), "{label} emitted artifact");
        println!("native-valid / closed capture rejected: {label}");
    }
}
#[test]
fn captured_receiver_counts_toward_existing64_argument_bound() {
    let params = (0..64)
        .map(|n| format!("a{n}:u8"))
        .collect::<Vec<_>>()
        .join(",");
    let values = vec!["1"; 64].join(",");
    let source = format!(
        "#[chialisp_oxide::puzzle] pub fn puzzle(x:u8)->u8{{let step=move |{params}|x+a0;step({values})}}"
    );
    let fixture = fixture(&source);
    let native = fixture.cargo(false);
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let extraction = fixture.cargo(true);
    assert!(!extraction.status.success());
    let error = String::from_utf8_lossy(&extraction.stderr);
    assert!(
        error.contains("function exceeds 64 parameters or 2,048 MIR locals")
            && !error.contains("internal compiler error"),
        "{error}"
    );
    assert!(fixture.exports().is_empty());
}
