//! Ordinary Rust Option outputs have a bounded list encoding, not a general
//! enum ABI. Native tests and independent Chialisp check the same constructors.
mod support;
#[path = "support/vm.rs"]
mod vm;

use chialisp_oxide_backend_experiment::{FailureCode, Outcome};
use chialisp_oxide_ir::Program;
use std::process::Command;
use support::Fixture;

const SOURCE: &str = r#"
use chialisp_oxide::Tree;
type MaybeByte = core::option::Option<u8>;

#[chialisp_oxide::puzzle]
pub fn maybe(choose: bool, n: u8) -> MaybeByte {
    if choose { Some(n) } else { None }
}
#[chialisp_oxide::puzzle]
pub fn unit(choose: bool) -> Option<()> {
    if choose { Some(()) } else { None }
}
#[chialisp_oxide::puzzle]
pub fn bytes(choose: bool, value: [u8; 2]) -> Option<[u8; 2]> {
    if choose { Some(value) } else { None }
}
#[chialisp_oxide::puzzle]
pub fn tuple(choose: bool, n: u8) -> Option<(u8, bool)> {
    if choose { Some((n, choose)) } else { None }
}
#[chialisp_oxide::puzzle]
pub fn nested(outer: bool, inner: bool, n: u8) -> Option<Option<u8>> {
    if outer { Some(if inner { Some(n) } else { None }) } else { None }
}
use chialisp_oxide::Cons;

#[chialisp_oxide::puzzle]
pub fn tail(choose: bool, n: u8) -> Cons<u8, Option<u8>> {
    Cons { first: 7, rest: if choose { Some(n) } else { None } }
}
mod documented_conditions {
use chialisp_oxide::{
    AggSigMe, Amount, AssertMyAmount, AssertSecondsAbsolute, Cons, CreateCoin,
    PuzzleHash, Timestamp, assert_before_seconds_absolute, assert_my_amount,
    assert_seconds_absolute, create_coin,
};

pub type DeadlineConditions = Cons<
    AssertSecondsAbsolute,
    Cons<AssertMyAmount, Cons<CreateCoin, Option<AggSigMe>>>,
>;

pub fn assemble_conditions(
    before_deadline: bool,
    deadline: Timestamp,
    amount: Amount,
    destination: PuzzleHash,
    authorization: Option<AggSigMe>,
) -> DeadlineConditions {
    let time_condition = if before_deadline {
        assert_before_seconds_absolute(deadline)
    } else {
        assert_seconds_absolute(deadline)
    };
    Cons {
        first: time_condition,
        rest: Cons {
            first: assert_my_amount(amount),
            rest: Cons {
                first: create_coin(destination, amount),
                rest: authorization,
            },
        },
    }
}
}
#[chialisp_oxide::puzzle]
pub fn condition_tail(before_deadline: bool, signed: bool) -> documented_conditions::DeadlineConditions {
    use chialisp_oxide::{Amount, Message32, PublicKey, PuzzleHash, Timestamp, agg_sig_me};
    // Opaque fixture bytes exercise the wire shape; these tests do not ask
    // consensus to parse the key or verify any BLS signature.
    const FIXTURE_KEY: PublicKey = PublicKey([0x11; 48]);
    const FIXTURE_INTENT: Message32 = Message32([0x22; 32]);
    const PAYMENT: Amount = Amount::from_mojos(1_000);
    const DESTINATION: PuzzleHash = PuzzleHash([0x42; 32]);
    const DEADLINE: Timestamp = Timestamp::from_unix_seconds(100);
    let authorization = if signed {
        Some(agg_sig_me(FIXTURE_KEY, FIXTURE_INTENT))
    } else {
        None
    };
    documented_conditions::assemble_conditions(
        before_deadline, DEADLINE, PAYMENT, DESTINATION, authorization,
    )
}
fn forward(value: Option<u8>) -> Option<u8> { value }
#[chialisp_oxide::puzzle]
pub fn forwarded(choose: bool, n: u8) -> std::option::Option<u8> {
    forward(if choose { std::option::Option::Some(n) } else { std::option::Option::None })
}
#[chialisp_oxide::puzzle]
pub fn snapshot(n: u8) -> (Option<u8>, Option<u8>) {
    let mut value = Some(n);
    let before = value;
    value = None;
    (before, value)
}
#[chialisp_oxide::puzzle(environment = "raw")]
pub fn raw<'a>(value: &'a Tree<'a>) -> Option<&'a Tree<'a>> { Some(value) }
#[chialisp_oxide::puzzle]
pub fn strict_none() -> Option<u8> { None }
#[chialisp_oxide::puzzle(environment = "ignored")]
pub fn ignored_some() -> Option<()> { Some(()) }
fn checked(n: u8) -> u8 { assert!(n < 7); n }
#[chialisp_oxide::puzzle]
pub fn computed(choose: bool, n: u8) -> Option<u8> {
    if choose { Some(checked(n)) } else { None }
}
#[chialisp_oxide::puzzle]
pub fn discarded(n: u8) -> u8 { let _ = Some(checked(n)); 9 }
#[chialisp_oxide::puzzle]
pub fn ordinary(n: u8) -> u8 { n }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_source_native_values_and_panics() {
        for n in 0..=u8::MAX {
            assert_eq!(maybe(false, n), None);
            assert_eq!(maybe(true, n), Some(n));
            assert_eq!(tuple(true, n), Some((n, true)));
            assert_eq!(nested(false, true, n), None);
            assert_eq!(nested(true, false, n), Some(None));
            assert_eq!(nested(true, true, n), Some(Some(n)));
            assert_eq!(tail(false, n).rest, None);
            assert_eq!(tail(true, n).rest, Some(n));
            assert_eq!(forwarded(false, n), None);
            assert_eq!(forwarded(true, n), Some(n));
            assert_eq!(snapshot(n), (Some(n), None));
            assert_eq!(computed(false, n), None);
        }
        assert_eq!(unit(false), None);
        assert_eq!(unit(true), Some(()));
        assert_eq!(bytes(false, [0, 255]), None);
        assert_eq!(bytes(true, [0, 255]), Some([0, 255]));
        let leaf = Tree::Atom(&[0, 0, 1]);
        assert_eq!(raw(&leaf), Some(&leaf));
        assert_eq!(strict_none(), None);
        assert_eq!(ignored_some(), Some(()));
        assert_eq!(computed(true, 6), Some(6));
        assert_eq!(discarded(6), 9);
        assert!(std::panic::catch_unwind(|| computed(true, 7)).is_err());
        assert!(std::panic::catch_unwind(|| discarded(7)).is_err());
        for before_deadline in [false, true] {
            for signed in [false, true] {
                let conditions = condition_tail(before_deadline, signed);
                assert_eq!(conditions.first.0, if before_deadline { 85 } else { 81 });
                assert_eq!(conditions.first.1.unix_seconds(), 100);
                let amount_condition = conditions.rest.first;
                assert_eq!(amount_condition.0, 73);
                assert_eq!(amount_condition.1.mojos(), 1_000);
                let payout = conditions.rest.rest.first;
                assert_eq!(payout.0, 51);
                assert_eq!(payout.1.0, [0x42; 32]);
                assert_eq!(payout.2.mojos(), 1_000);
                match conditions.rest.rest.rest {
                    Some((opcode, key, intent)) => {
                        assert!(signed);
                        assert_eq!(opcode, 50);
                        assert_eq!(key.0, [0x11; 48]);
                        assert_eq!(intent.0, [0x22; 32]);
                    }
                    None => assert!(!signed),
                }
            }
        }
    }
}
"#;

#[test]
fn documented_rust_examples_are_exact_parts_of_the_checked_cargo_fixture() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/optional-lists.md");
    let document = std::fs::read_to_string(path).unwrap();
    let blocks = document
        .split("```rust\n")
        .skip(1)
        .map(|tail| tail.split_once("\n```").expect("closed Rust example").0)
        .collect::<Vec<_>>();
    assert_eq!(
        blocks.len(),
        2,
        "Each Rust example needs an execution binding"
    );
    for block in blocks {
        assert!(
            SOURCE.contains(block),
            "Documentation differs from the native and CLVM fixture:\n{block}"
        );
    }
}

fn success(output: std::process::Output) {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn native_tests(fixture: &Fixture) {
    let compiler = fixture.compiler();
    success(
        Command::new(compiler.parent().unwrap().join("cargo"))
            .args(["test", "--lib", "--offline", "--jobs", "1", "--target-dir"])
            .arg(fixture.0.join("native-build"))
            .current_dir(&fixture.0)
            .env("RUSTC", compiler)
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER")
            .env_remove("CHIALISP_OXIDE_CARGO_MODE")
            .output()
            .unwrap(),
    );
}

fn find<'a>(programs: &'a [Program], name: &str) -> &'a Program {
    programs
        .iter()
        .find(|program| program.name.ends_with(&format!("::{name}")))
        .unwrap()
}

fn compare(program: &Program, reference: &str, solutions: &[&str]) {
    let reference = chialisp_oxide_reference::compile(reference).unwrap();
    let optimized = chialisp_oxide_ir::optimize(program).unwrap();
    let transported: Program =
        serde_json::from_value(serde_json::to_value(&optimized).unwrap()).unwrap();
    for candidate in [program, &transported] {
        for optimize in [false, true] {
            let code = chialisp_oxide_ir::compile(candidate, optimize).unwrap();
            for solution in solutions {
                let actual = vm::execute(&code, solution, 10_000_000).unwrap();
                let expected = vm::execute(&reference, solution, 10_000_000).unwrap();
                assert_eq!(
                    actual.outcome, expected.outcome,
                    "{}, {solution}, optimize={optimize}",
                    program.name
                );
                if actual.outcome == Outcome::Rejected {
                    assert_eq!(actual.failure.unwrap().code(), FailureCode::Raise);
                }
            }
        }
    }
}

#[test]
fn actual_core_option_outputs_are_proper_optional_lists_in_both_modes() {
    let fixture = Fixture::new();
    fixture.source(SOURCE);
    native_tests(&fixture);
    success(fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 15);
    for name in [
        "maybe",
        "unit",
        "bytes",
        "tuple",
        "nested",
        "tail",
        "condition_tail",
        "forwarded",
        "snapshot",
        "raw",
        "strict_none",
        "ignored_some",
        "computed",
    ] {
        let program = find(&programs, name);
        assert!(program.uses_optional() && program.uses_runtime(), "{name}");
        assert_eq!(serde_json::to_value(program).unwrap()["ir_version"], 6);
    }
    assert!(find(&programs, "tail").uses_pairs());
    assert!(
        serde_json::to_value(find(&programs, "ordinary"))
            .unwrap()
            .get("ir_version")
            .is_none()
    );
    for name in ["maybe", "forwarded"] {
        compare(
            find(&programs, name),
            "(mod (CHOOSE N) (if CHOOSE (list N) ()))",
            &["(() 0)", "(1 0)", "(() 255)", "(1 255)"],
        );
    }
    compare(
        find(&programs, "unit"),
        "(mod (CHOOSE) (if CHOOSE (list ()) ()))",
        &["(())", "(1)"],
    );
    compare(
        find(&programs, "bytes"),
        "(mod (CHOOSE VALUE) (if CHOOSE (list VALUE) ()))",
        &["(() 0x0000)", "(1 0x0000)", "(1 0x00ff)", "(1 0xff00)"],
    );
    compare(
        find(&programs, "tuple"),
        "(mod (CHOOSE N) (if CHOOSE (list (list N CHOOSE)) ()))",
        &["(() 0)", "(1 0)", "(1 255)"],
    );
    compare(
        find(&programs, "nested"),
        "(mod (OUTER INNER N) (if OUTER (list (if INNER (list N) ())) ()))",
        &[
            "(() () 0)",
            "(() 1 255)",
            "(1 () 0)",
            "(1 1 0)",
            "(1 1 255)",
        ],
    );
    compare(
        find(&programs, "tail"),
        "(mod (CHOOSE N) (c 7 (if CHOOSE (list N) ())))",
        &["(() 0)", "(1 0)", "(1 255)"],
    );
    let conditions_reference = format!(
        "(mod (BEFORE SIGNED) (c (list (if BEFORE 85 81) 100) \
         (c (list 73 1000) (c (list 51 0x{} 1000) \
         (if SIGNED (list (list 50 0x{} 0x{})) ())))))",
        "42".repeat(32),
        "11".repeat(48),
        "22".repeat(32),
    );
    compare(
        find(&programs, "condition_tail"),
        &conditions_reference,
        &["(() ())", "(() 1)", "(1 ())", "(1 1)"],
    );
    compare(
        find(&programs, "snapshot"),
        "(mod (N) (list (list N) ()))",
        &["(0)", "(7)", "(255)"],
    );
    compare(
        find(&programs, "raw"),
        "(mod ENV (list ENV))",
        &["()", "0x00", "0x000001", "(1 . 2)", "((1 . 2) . 0x0000)"],
    );
    compare(find(&programs, "strict_none"), "(mod () ())", &["()"]);
    compare(
        find(&programs, "ignored_some"),
        "(mod ENV (list ()))",
        &["()", "0x00", "(1 . 2)"],
    );
    compare(
        find(&programs, "computed"),
        "(mod (CHOOSE N) (if CHOOSE (if (> 7 N) (list N) (x)) ()))",
        &["(() 7)", "(() 255)", "(1 0)", "(1 6)", "(1 7)"],
    );
    compare(
        find(&programs, "discarded"),
        "(mod (N) (if (> 7 N) 9 (x)))",
        &["(0)", "(6)", "(7)", "(255)"],
    );
    for optimize in [false, true] {
        for (name, solutions) in [
            (
                "maybe",
                &[
                    "()",
                    "(1)",
                    "(1 0 2)",
                    "(1 . 2)",
                    "(0x00 7)",
                    "(2 7)",
                    "(1 0x0001)",
                    "(1 256)",
                ][..],
            ),
            ("strict_none", &["0x00", "(1)", "(1 . 2)"][..]),
        ] {
            let code = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            for solution in solutions {
                assert_eq!(
                    vm::execute(&code, solution, 10_000_000).unwrap().outcome,
                    Outcome::Rejected,
                    "{name}, {solution}, optimize={optimize}"
                );
            }
        }
    }
}

fn rejected_native_valid(fixture: &Fixture, source: &str, diagnostic: Option<&str>) {
    fixture.source(source);
    success(fixture.cargo(false));
    let output = fixture.cargo(true);
    assert!(!output.status.success(), "unexpected admission: {source}");
    if let Some(diagnostic) = diagnostic {
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(diagnostic),
            "expected {diagnostic}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn output_support_does_not_admit_optional_input_decoding_or_inspection() {
    let fixture = Fixture::new();
    for source in [
        "#[chialisp_oxide::puzzle]pub fn puzzle(_:Option<u8>)->u8{7}",
        "#[chialisp_oxide::puzzle]pub fn puzzle(_:(u8,Option<u8>))->u8{7}",
        "#[chialisp_oxide::puzzle]pub fn puzzle(_:chialisp_oxide::Cons<u8,Option<u8>>)->u8{7}",
        "#[chialisp_oxide::puzzle]pub fn puzzle(_:[Option<u8>;2])->u8{7}",
        "#[repr(transparent)]pub struct Wrapped(Option<u8>);#[chialisp_oxide::puzzle]pub fn puzzle(_:Wrapped)->u8{7}",
        "fn maybe(flag:bool,n:u8)->Option<u8>{if flag{Some(n)}else{None}}#[chialisp_oxide::puzzle]pub fn puzzle(flag:bool,n:u8)->u8{match maybe(flag,n){Some(value)=>value,None=>0}}",
        "fn maybe(flag:bool,n:u8)->Option<u8>{if flag{Some(n)}else{None}}#[chialisp_oxide::puzzle]pub fn puzzle(flag:bool,n:u8)->bool{maybe(flag,n).is_some()}",
        "fn maybe(flag:bool,n:u8)->Option<u8>{if flag{Some(n)}else{None}}#[chialisp_oxide::puzzle]pub fn puzzle(flag:bool,n:u8)->u8{maybe(flag,n).unwrap()}",
    ] {
        rejected_native_valid(&fixture, source, Some("optional"));
    }
}

#[test]
fn names_unsupported_payloads_and_drops_cannot_acquire_option_encoding() {
    let fixture = Fixture::new();
    for source in [
        "pub enum Option<T>{None,Some(T)}#[chialisp_oxide::puzzle]pub fn puzzle()->Option<u8>{Option::Some(7)}",
        "mod core{pub mod option{pub enum Option<T>{None,Some(T)}}}#[chialisp_oxide::puzzle]pub fn puzzle()->core::option::Option<u8>{core::option::Option::Some(7)}",
        "pub struct Dropped(u8);impl Drop for Dropped{fn drop(&mut self){panic!()}}#[chialisp_oxide::puzzle]pub fn puzzle()->Option<Dropped>{None}",
        "pub struct Dropped(u8);impl Drop for Dropped{fn drop(&mut self){panic!()}}#[chialisp_oxide::puzzle]pub fn puzzle()->Option<(u8,Dropped)>{None}",
        "#[chialisp_oxide::puzzle]pub fn puzzle()->Option<Box<u8>>{None}",
        "#[chialisp_oxide::puzzle]pub fn puzzle()->Option<Vec<u8>>{None}",
        "#[chialisp_oxide::puzzle]pub fn puzzle()->Option<f64>{None}",
        "#[chialisp_oxide::puzzle]pub fn puzzle()->&'static Option<u8>{&None}",
    ] {
        rejected_native_valid(&fixture, source, None);
    }
}

#[test]
fn an_allowed_sdk_crate_name_does_not_authorize_a_fake_option_enum() {
    let fixture = Fixture::new();
    let metadata = fixture.dependency_source(
        "chialisp_oxide",
        true,
        "pub enum Option<T> { None, Some(T) }",
    );
    fixture.source(
        "#[doc=\"__oxide_puzzle_v1\"]pub fn puzzle()->sdk::Option<u8>{sdk::Option::Some(7)}",
    );
    success(
        Command::new(fixture.compiler())
            .args([
                "--crate-name",
                "native_optional_control",
                "--crate-type",
                "lib",
                "--edition",
                "2024",
                "--emit",
                "metadata",
                "--extern",
            ])
            .arg(format!("sdk={}", metadata.display()))
            .arg("--out-dir")
            .arg(&fixture.0)
            .arg(fixture.0.join("src/lib.rs"))
            .output()
            .unwrap(),
    );
    let output = fixture.check_external("sdk", &metadata);
    assert!(
        !output.status.success(),
        "an enum in a crate named chialisp_oxide is not core::option::Option"
    );
}

#[test]
fn optional_payload_discovery_keeps_the_existing_type_depth_bound() {
    let fixture = Fixture::new();
    let result = format!("{}u8{}", "Option<".repeat(40), ">".repeat(40));
    rejected_native_valid(
        &fixture,
        &format!("#[chialisp_oxide::puzzle]pub fn puzzle()->{result}{{None}}"),
        Some("limit"),
    );
}

const SOURCE_ORDER: &str = r#"
fn checked(n: u8) -> u8 { assert!(n != 7); n }
fn spin_for_seven(n: u8) -> u8 {
    if n == 7 { spin_for_seven(n) } else { n }
}
#[chialisp_oxide::puzzle]
pub fn reject_first(n: u8) -> (Option<u8>, u8) {
    (Some(checked(n)), spin_for_seven(n))
}
#[chialisp_oxide::puzzle]
pub fn spin_first(n: u8) -> (u8, Option<u8>) {
    (spin_for_seven(n), Some(checked(n)))
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn finite_native_values_and_first_panic() {
        assert_eq!(reject_first(6), (Some(6), 6));
        assert_eq!(spin_first(6), (6, Some(6)));
        assert!(std::panic::catch_unwind(|| reject_first(7)).is_err());
        // spin_first(7) is deliberately never called on the native stack.
    }
}
"#;

#[test]
fn some_payload_evaluation_precedes_later_fields_and_retains_divergence_order() {
    let fixture = Fixture::new();
    fixture.source(SOURCE_ORDER);
    native_tests(&fixture);
    success(fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 2);
    for program in &programs {
        let reject_first = program.name.ends_with("::reject_first");
        let reference = if reject_first {
            "(mod (N) (list (list N) N))"
        } else {
            "(mod (N) (list N (list N)))"
        };
        compare(program, reference, &["(0)", "(6)", "(8)", "(255)"]);
        let optimized = chialisp_oxide_ir::optimize(program).unwrap();
        let transported: Program =
            serde_json::from_value(serde_json::to_value(&optimized).unwrap()).unwrap();
        for candidate in [program, &transported] {
            for optimize in [false, true] {
                let code = chialisp_oxide_ir::compile(candidate, optimize).unwrap();
                if reject_first {
                    let actual = vm::execute(&code, "(7)", 1_000_000).unwrap();
                    assert_eq!(actual.outcome, Outcome::Rejected);
                    assert_eq!(actual.failure.unwrap().code(), FailureCode::Raise);
                } else {
                    // This resource observation distinguishes order; it is
                    // inconclusive and is never credited as policy rejection.
                    let actual =
                        chialisp_oxide_backend_experiment::execute(&code, "(7)", 1_000_000)
                            .unwrap();
                    assert_eq!(actual.outcome, Outcome::Inconclusive);
                    assert_eq!(actual.failure.unwrap().code(), FailureCode::CostExceeded);
                }
            }
        }
    }
}
