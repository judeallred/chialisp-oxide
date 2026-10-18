//! Actual Cargo source fallback controls; VM resources stay inconclusive.
mod support;
#[path = "support/vm.rs"]
mod vm;

use chialisp_oxide_backend_experiment::{FailureCode, Outcome};
use std::process::Command;
use support::Fixture;

const SOURCE: &str = include_str!("fixtures/acyclic-expansion/src/lib.rs");
fn success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn expected(value: u16) -> Outcome {
    vm::execute(&[1], &value.to_string(), 1_000_000)
        .unwrap()
        .outcome
}

#[test]
fn finite_helper_dag_uses_tables_after_legacy_expansion_limit() {
    let fixture = Fixture::new();
    fixture.source(SOURCE);
    success(
        &Command::new(fixture.compiler().parent().unwrap().join("cargo"))
            .args(["test", "--lib", "--offline", "--target-dir"])
            .arg(fixture.0.join("native-build"))
            .current_dir(&fixture.0)
            .env("RUSTC", fixture.compiler())
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER")
            .env_remove("CHIALISP_OXIDE_CARGO_MODE")
            .output()
            .unwrap(),
    );
    success(&fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 6);
    for program in &programs {
        program.validate().unwrap();
        assert_eq!(serde_json::to_value(program).unwrap()["ir_version"], 4);
        assert!((9..=10).contains(&program.functions.len()));
    }
    for optimize in [false, true] {
        let mut costs = std::collections::HashMap::new();
        for program in &programs {
            let name = program.name.rsplit("::").next().unwrap();
            let lazy = name == "lazy";
            let overflow = name == "unused_overflow";
            let copied = name == "copied_result";
            let returns_increment = name == "dag_guard" || lazy;
            let threshold = if overflow { 255 } else { 100 };
            let expression = if copied {
                "(* (+ VALUE 1) 2)"
            } else if returns_increment {
                "(+ VALUE 1)"
            } else {
                "7"
            };
            let body = format!("(if (> {threshold} VALUE) {expression} (x))");
            let source = if lazy {
                format!("(mod (VALUE CHOOSE) (if CHOOSE {body} 7))")
            } else {
                format!("(mod (VALUE) {body})")
            };
            let reference = chialisp_oxide_reference::compile(&source).unwrap();
            let bytes = chialisp_oxide_ir::compile(program, optimize).unwrap();
            for choose in if lazy { vec![false, true] } else { vec![true] } {
                for value in 0..=u8::MAX {
                    let input = if lazy {
                        format!("({value} {})", u8::from(choose))
                    } else {
                        format!("({value})")
                    };
                    let execution = vm::execute(&bytes, &input, 10_000_000).unwrap();
                    let outcome = execution.outcome;
                    assert_eq!(
                        outcome,
                        vm::execute(&reference, &input, 10_000_000).unwrap().outcome,
                        "{name}: {input}, optimize={optimize}"
                    );
                    if !choose || u16::from(value) < threshold {
                        let result = if !choose {
                            7
                        } else if copied {
                            (u16::from(value) + 1) * 2
                        } else if returns_increment {
                            u16::from(value) + 1
                        } else {
                            7
                        };
                        assert_eq!(outcome, expected(result));
                    } else {
                        assert_eq!(outcome, Outcome::Rejected);
                        assert_eq!(execution.failure.unwrap().code(), FailureCode::Raise);
                    }
                    if !lazy && value == 7 {
                        costs.insert(name.to_owned(), execution.cost.unwrap());
                    }
                }
            }
            let malformed = if lazy {
                vec![
                    "()",
                    "(1)",
                    "(1 1 2)",
                    "(1 . 2)",
                    "((1 . 2) 1)",
                    "(0x0001 1)",
                    "(256 1)",
                    "(-1 1)",
                    "(1 2)",
                ]
            } else {
                vec![
                    "()",
                    "(1 2)",
                    "(1 . 2)",
                    "((1 . 2))",
                    "(0x0001)",
                    "(0x0000)",
                    "(256)",
                    "(-1)",
                ]
            };
            for input in malformed {
                let failed = vm::execute(&bytes, input, 10_000_000).unwrap();
                assert_eq!(failed.outcome, Outcome::Rejected, "{name}: {input}");
                let expected_code = if !optimize && input == if lazy { "(1 1 2)" } else { "(1 2)" }
                {
                    FailureCode::InvalidOpArg
                } else {
                    FailureCode::Raise
                };
                assert_eq!(
                    failed.failure.unwrap().code(),
                    expected_code,
                    "{name}: {input}"
                );
            }
        }
        // A copied call result remains a captured value; finite VM work must
        // not accidentally repeat the complete 128-leaf helper DAG.
        assert!(costs["copied_result"] < 2 * costs["dag_guard"]);
    }
}

#[test]
fn early_rejection_precedes_finite_expensive_argument_work() {
    let fixture = Fixture::new();
    let mut source = SOURCE.to_owned();
    for i in 8..=12 {
        source.push_str(&format!("#[inline(never)] fn dag{i}(value:u8)->u8 {{let answer=dag{}(value);let _checked=dag{}(value);answer}}\n", i-1, i-1));
    }
    source.push_str(r#"
#[inline(never)] fn reject(_value:u8)->u8 {panic!()}
#[inline(never)] fn first(value:u8,_unused:u8)->u8 {value}
#[chialisp_oxide::puzzle] pub fn reject_before_work(value:u8)->u8 {first(reject(value),dag12(value))}
#[chialisp_oxide::puzzle] pub fn work_before_reject(value:u8)->u8 {first(dag12(value),reject(value))}
"#);
    fixture.source(&source);
    success(&fixture.cargo(true));
    let programs = fixture.exports();
    for optimize in [false, true] {
        let find = |name: &str| {
            programs
                .iter()
                .find(|p| p.name.ends_with(&format!("::{name}")))
                .unwrap()
        };
        let before = chialisp_oxide_ir::compile(find("reject_before_work"), optimize).unwrap();
        let after = chialisp_oxide_ir::compile(find("work_before_reject"), optimize).unwrap();
        let rejected = vm::execute(&before, "(7)", 100_000).unwrap();
        assert_eq!(rejected.outcome, Outcome::Rejected);
        assert_eq!(rejected.failure.unwrap().code(), FailureCode::Raise);
        let unfinished =
            chialisp_oxide_backend_experiment::execute(&after, "(7)", 100_000).unwrap();
        assert_eq!(unfinished.outcome, Outcome::Inconclusive);
        assert_eq!(
            unfinished.failure.unwrap().code(),
            FailureCode::CostExceeded
        );
    }
}

#[test]
fn fallback_does_not_hide_unsupported_helpers_or_existing_table_limits() {
    let helpers = SOURCE.replace("#[chialisp_oxide::puzzle]", "");
    for (body, diagnostic) in [
        (
            "#[inline(never)] fn unsupported(value:u8)->u8 {value / 2} #[chialisp_oxide::puzzle] pub fn bad(value:u8)->u8 {let _checked=dag7(value);unsupported(value)}",
            "unsupported integer operation",
        ),
        (
            "#[inline(never)] fn unsupported<T>(value:u8)->u8 {value} #[chialisp_oxide::puzzle] pub fn bad(value:u8)->u8 {let _checked=dag7(value);unsupported::<u8>(value)}",
            "generic call",
        ),
        (
            "#[chialisp_oxide::puzzle] pub fn bad(value:u8)->u8 {let _checked=dag7(value);value.wrapping_add(1)}",
            "external call",
        ),
        (
            "#[chialisp_oxide::puzzle] pub fn bad(value:u8)->u8 {let _checked=dag7(value);let mut next=value;while next<9 {next+=1;}next}",
            "loops are unsupported",
        ),
    ] {
        let fixture = Fixture::new();
        fixture.source(&format!("{helpers}\n{body}"));
        success(&fixture.cargo(false));
        let output = fixture.cargo(true);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(diagnostic),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(fixture.exports().is_empty());
    }
    for count in [64, 256] {
        let fixture = Fixture::new();
        let mut source = "#[inline(never)] fn leaf(value:u8)->u8 {value}\n".to_owned();
        for index in 0..count {
            let previous = if index == 0 {
                "leaf".to_owned()
            } else {
                format!("chain{}", index - 1)
            };
            source.push_str(&format!(
                "#[inline(never)] fn chain{index}(value:u8)->u8 {{{previous}(value)}}\n"
            ));
        }
        source.push_str(&format!(
            "#[chialisp_oxide::puzzle] pub fn bad(value:u8)->u8 {{chain{}(value)}}",
            count - 1
        ));
        fixture.source(&source);
        success(&fixture.cargo(false));
        let output = fixture.cargo(true);
        assert!(!output.status.success());
        let diagnostic = if count == 64 {
            "runtime call graphs are limited to 64 functions"
        } else {
            "call-graph discovery exceeded 256 functions"
        };
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(diagnostic),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(fixture.exports().is_empty());
    }
}

#[test]
fn a_supported_deep_helper_chain_can_use_existing_tables() {
    let fixture = Fixture::new();
    let mut source = "#[inline(never)] fn leaf(value:u8)->u8 {value+1}\n".to_owned();
    for index in 0..33 {
        let previous = if index == 0 {
            "leaf".to_owned()
        } else {
            format!("chain{}", index - 1)
        };
        source.push_str(&format!(
            "#[inline(never)] fn chain{index}(value:u8)->u8 {{{previous}(value)}}\n"
        ));
    }
    source.push_str("#[chialisp_oxide::puzzle] pub fn puzzle(value:u8)->u8 {chain32(value)}");
    fixture.source(&source);
    success(&fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 1);
    assert_eq!(programs[0].functions.len(), 35);
    for optimize in [false, true] {
        let bytes = chialisp_oxide_ir::compile(&programs[0], optimize).unwrap();
        assert_eq!(
            vm::execute(&bytes, "(7)", 10_000_000).unwrap().outcome,
            expected(8)
        );
        let failure = vm::execute(&bytes, "(255)", 10_000_000).unwrap();
        assert_eq!(failure.outcome, Outcome::Rejected);
        assert_eq!(failure.failure.unwrap().code(), FailureCode::Raise);
    }
}
