//! Source-level domain safety and condition wire conformance, not ledger validity.
#[path = "named_conditions/oracle.rs"]
mod oracle;
mod support;
#[path = "support/vm.rs"]
mod vm;

use chialisp_oxide_backend_experiment::{Execution, FailureCode, FailureKind, Outcome};
use chialisp_oxide_ir::Program;
use clvmr_sdk::{Allocator, serde::node_from_bytes, serde::node_to_bytes};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path, process::Command};
use support::Fixture;

const MAX_VM_COST: u64 = 10_000_000;

fn native_command(fixture: &Fixture, operation: &str) -> Command {
    let mut command = Command::new(Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/cargo"));
    command
        .current_dir(&fixture.0)
        .env("RUSTC", fixture.compiler())
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CHIALISP_OXIDE_CARGO_MODE")
        .args([
            operation,
            "--lib",
            "--offline",
            "--jobs",
            "1",
            "--target-dir",
        ])
        .arg(fixture.0.join("native-build"));
    command
}

fn checked_fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.source(include_str!("named_conditions/library.rs"));
    for (name, source) in [
        ("native.rs", include_str!("named_conditions/native.rs")),
        ("oracle.rs", include_str!("named_conditions/oracle.rs")),
    ] {
        std::fs::write(fixture.0.join("src").join(name), source).unwrap();
    }
    // These are host-only test dependencies. The annotated Cargo exports still
    // depend solely on the real facade; extraction never enables cfg(test).
    let manifest = fixture.0.join("Cargo.toml");
    let mut source = std::fs::read_to_string(&manifest).unwrap();
    source.push_str(
        "\n[dev-dependencies]\nchia-sdk-types=\"=0.36.0\"\nclvm-traits=\"=0.36.1\"\nclvmr-sdk={package=\"clvmr\",version=\"=0.16.2\"}\n",
    );
    std::fs::write(manifest, source).unwrap();
    let native = native_command(&fixture, "test").output().unwrap();
    assert!(
        native.status.success(),
        "native Cargo fixture failed:\n{}\n{}",
        String::from_utf8_lossy(&native.stdout),
        String::from_utf8_lossy(&native.stderr),
    );
    let extraction = fixture.cargo(true);
    assert!(
        extraction.status.success(),
        "annotated Cargo extraction failed: {}",
        String::from_utf8_lossy(&extraction.stderr),
    );
    assert!(extraction.stdout.is_empty());
    fixture
}

fn find<'a>(programs: &'a [Program], name: &str) -> &'a Program {
    programs
        .iter()
        .find(|program| program.name == format!("wrapper_fixture::{name}"))
        .unwrap()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn atom(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        "()".into()
    } else {
        format!("0x{}", hex(bytes))
    }
}

fn accepted(program: &[u8], environment: &str) -> Execution {
    let execution = vm::execute(program, environment, MAX_VM_COST).unwrap();
    assert!(
        matches!(execution.outcome, Outcome::Accepted { .. }),
        "expected successful execution for {environment}: {execution:?}",
    );
    assert!(execution.failure.is_none());
    execution
}

fn assert_vm_conditions(
    program: &[u8],
    environment: &str,
    expected: Vec<chia_sdk_types::Condition>,
) {
    let execution = accepted(program, environment);
    let Outcome::Accepted { output_hex } = execution.outcome else {
        unreachable!()
    };
    assert!(output_hex.len() <= 16_384 && output_hex.len().is_multiple_of(2));
    let bytes = (0..output_hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&output_hex[index..index + 2], 16).unwrap())
        .collect::<Vec<_>>();
    // Decode primary VM 0.21 output using the independent SDK's 0.16 allocator;
    // this is wire decoding, not a second VM run or a consensus submission.
    let a = &mut Allocator::new_limited(1_048_576);
    let node = node_from_bytes(a, &bytes).unwrap();
    assert_eq!(node_to_bytes(a, node).unwrap(), bytes);
    oracle::assert_exact_conditions(a, node, expected);
}

fn assert_abi_rejection(program: &[u8], environment: &str) {
    assert_abi_failure(program, environment, FailureCode::Raise);
}

// A remaining pair after the last parameter is compared with nil. Unoptimized
// Rust faults that comparison as InvalidOpArg; the optimized decoder raises.
// Both are typed program failures, matching the recorded boomerang evidence.
fn assert_extra_argument_rejection(program: &[u8], environment: &str, optimize: bool) {
    let expected = if optimize {
        FailureCode::Raise
    } else {
        FailureCode::InvalidOpArg
    };
    assert_abi_failure(program, environment, expected);
}

fn assert_abi_failure(program: &[u8], environment: &str, expected: FailureCode) {
    let execution = vm::execute(program, environment, MAX_VM_COST).unwrap();
    assert_eq!(
        execution.outcome,
        Outcome::Rejected,
        "{environment}: {execution:?}"
    );
    let failure = execution.failure.expect("typed ABI failure");
    assert_eq!(failure.kind(), FailureKind::Program);
    assert_eq!(failure.code(), expected, "{environment}: {execution:?}");
}

fn has_sha256(program: &Program) -> bool {
    let json = serde_json::to_value(program).unwrap();
    let mut pending = vec![&json];
    while let Some(value) = pending.pop() {
        match value {
            serde_json::Value::Object(fields) => {
                if fields.get("op").and_then(serde_json::Value::as_str) == Some("sha256") {
                    return true;
                }
                pending.extend(fields.values());
            }
            serde_json::Value::Array(items) => pending.extend(items),
            _ => {}
        }
    }
    false
}

#[test]
fn ordinary_cargo_helpers_match_native_and_sdk_conditions_in_both_compiler_modes() {
    let fixture = checked_fixture();
    let programs = fixture.exports();
    assert_eq!(
        programs
            .iter()
            .map(|program| program.name.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "wrapper_fixture::announcement_conditions",
            "wrapper_fixture::discarded_announcement",
            "wrapper_fixture::height_window",
            "wrapper_fixture::supplied_announcement",
            "wrapper_fixture::without_announcement",
        ]),
    );
    for program in &programs {
        program.validate().unwrap();
        assert_eq!(program.environment, chialisp_oxide_ir::Environment::Strict);
    }
    let discarded = find(&programs, "discarded_announcement");
    assert!(discarded.uses_runtime());
    assert!(has_sha256(discarded));
    assert!(has_sha256(&chialisp_oxide_ir::optimize(discarded).unwrap()));

    for optimize in [false, true] {
        let heights =
            chialisp_oxide_ir::compile(find(&programs, "height_window"), optimize).unwrap();
        for values in oracle::HEIGHT_CASES {
            assert_vm_conditions(
                &heights,
                &format!("({} {} {} {})", values[0], values[1], values[2], values[3]),
                oracle::height_conditions(values),
            );
        }
        for environment in ["()", "1", "(1 2 3)", "(1 2 3 . 4)"] {
            assert_abi_rejection(&heights, environment);
        }
        assert_extra_argument_rejection(&heights, "(1 2 3 4 5)", optimize);
        // Exercise each nominal u32 slot, including padded zero/one. The wire
        // has no runtime age/height tag; its contract is exact canonical u32.
        for position in 0..4 {
            for invalid in ["-1", "4294967296", "0x00", "0x0001", "0x80", "(1)"] {
                let mut fields = ["1", "2", "3", "4"];
                fields[position] = invalid;
                assert_abi_rejection(&heights, &format!("({})", fields.join(" ")));
            }
        }

        let announcements =
            chialisp_oxide_ir::compile(find(&programs, "announcement_conditions"), optimize)
                .unwrap();
        let discarded = chialisp_oxide_ir::compile(discarded, optimize).unwrap();
        let without =
            chialisp_oxide_ir::compile(find(&programs, "without_announcement"), optimize).unwrap();
        for case in oracle::announcement_cases() {
            let mut independent = Sha256::new();
            independent.update(case.origin);
            independent.update(case.message);
            let digest: [u8; 32] = independent.finalize().into();
            assert_eq!(digest, case.digest, "{}", case.name);
            let environment = format!("({} {})", atom(&case.origin), atom(&case.message));
            assert_vm_conditions(
                &announcements,
                &environment,
                oracle::announcement_conditions(case),
            );
            let actual = accepted(&discarded, &environment);
            let baseline = accepted(&without, &environment);
            let seven = Outcome::Accepted {
                output_hex: "07".into(),
            };
            assert_eq!(actual.outcome, seven);
            assert_eq!(baseline.outcome, seven);
            assert!(
                actual.cost.unwrap() > baseline.cost.unwrap(),
                "unused SHA lost its strict work: optimize={optimize}, {}",
                case.name,
            );
        }
        let valid = atom(&[0x42; 32]);
        let mut malformed = vec![
            "()".to_owned(),
            "1".to_owned(),
            format!("({valid})"),
            format!("({valid} . {valid})"),
        ];
        for invalid in [
            "()".to_owned(),
            "(1)".to_owned(),
            atom(&[0x42; 31]),
            atom(&[0x42; 33]),
        ] {
            malformed.push(format!("({invalid} {valid})"));
            malformed.push(format!("({valid} {invalid})"));
        }
        for program in [&announcements, &discarded, &without] {
            for environment in &malformed {
                assert_abi_rejection(program, environment);
            }
            assert_extra_argument_rejection(program, &format!("({valid} {valid} 1)"), optimize);
        }

        let supplied =
            chialisp_oxide_ir::compile(find(&programs, "supplied_announcement"), optimize).unwrap();
        for arbitrary in [
            [0; 32],
            [0xff; 32],
            std::array::from_fn(|index| index as u8),
        ] {
            assert_vm_conditions(
                &supplied,
                &format!("({})", atom(&arbitrary)),
                oracle::supplied_announcement(arbitrary),
            );
        }
        for environment in [
            "()".to_owned(),
            "1".to_owned(),
            "(())".to_owned(),
            "((1))".to_owned(),
            format!("({})", atom(&[0; 31])),
            format!("({})", atom(&[0; 33])),
            format!("({valid} . 1)"),
        ] {
            assert_abi_rejection(&supplied, &environment);
        }
        assert_extra_argument_rejection(&supplied, &format!("({valid} 1)"), optimize);
    }
}

#[test]
fn ordinary_rustc_reports_each_accidental_unit_or_identifier_swap() {
    let fixture = Fixture::new();
    fixture.source(include_str!("named_conditions/type_errors.rs"));
    let output = native_command(&fixture, "check")
        .args(["--message-format=json", "--color=never"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(fixture.exports().is_empty());
    let diagnostics = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| {
            message["reason"] == "compiler-message" && message["message"]["level"] == "error"
        })
        .map(|message| message["message"].clone())
        .collect::<Vec<_>>();
    let cases = [
        ("absolute_height_rejects_age", "BlockHeight", "BlockAge"),
        ("relative_height_rejects_height", "BlockAge", "BlockHeight"),
        ("before_absolute_rejects_age", "BlockHeight", "BlockAge"),
        ("before_relative_rejects_height", "BlockAge", "BlockHeight"),
        (
            "announcement_rejects_coin_id",
            "CoinAnnouncementId",
            "CoinId",
        ),
        (
            "own_id_rejects_announcement",
            "CoinId",
            "CoinAnnouncementId",
        ),
        ("message_rejects_coin_id", "Message32", "CoinId"),
        ("origin_rejects_puzzle_hash", "CoinId", "PuzzleHash"),
    ];
    assert_eq!(diagnostics.len(), cases.len(), "{diagnostics:#?}");
    let source = include_str!("named_conditions/type_errors.rs");
    for (function, expected, supplied) in cases {
        let function_line = source
            .lines()
            .position(|line| line.contains(&format!("fn {function}(")))
            .unwrap()
            + 1;
        let call_line = function_line + 1;
        let matching =
            diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic["code"]["code"] == "E0308"
                        && diagnostic["spans"].as_array().unwrap().iter().any(|span| {
                            span["is_primary"] == true && span["line_start"] == call_line
                        })
                })
                .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1, "{function}: {diagnostics:#?}");
        let rendered = matching[0]["rendered"].as_str().unwrap();
        for name in [expected, supplied] {
            assert!(
                rendered.contains(name),
                "{function} omitted {name}: {rendered}"
            );
        }
        assert!(!rendered.contains("internal compiler error"));
    }
}
