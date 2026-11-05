//! Actual Cargo/rustc admission and finite VM comparisons for explicit pairs.
mod support;
#[path = "support/vm.rs"]
mod vm;
use chialisp_oxide_backend_experiment::Outcome;
use chialisp_oxide_ir::Program;
use std::process::Command;
use support::Fixture;

const SOURCE: &str = r#"
use chialisp_oxide::{Amount, Cons, Tree, sha256};
#[chialisp_oxide::puzzle]
pub fn append<'a>(tail: Tree<'a>) -> Cons<(u64,[u8;32]),Tree<'a>> {
    Cons { first: (50,[7;32]), rest: tail }
}
#[chialisp_oxide::puzzle]
pub fn hash_prefix<'a>(payload: &[u8], tail: &'a Tree<'a>) -> Cons<[u8;32], &'a Tree<'a>> {
    let digest = sha256(payload);
    Cons { first: digest, rest: tail }
}
#[chialisp_oxide::puzzle]
pub fn nested<'a>(value: Cons<Cons<u8,u8>,Tree<'a>>) -> Cons<u8,Tree<'a>> {
    Cons { first:value.first.rest, rest:value.rest }
}
#[chialisp_oxide::puzzle]
pub fn nominal(amount: Amount) -> Cons<Amount,u8> { Cons { first:amount, rest:3 } }
#[chialisp_oxide::puzzle]
pub fn head(value: Cons<u8,u8>) -> u8 { value.first }
#[chialisp_oxide::puzzle]
pub fn snapshot(n:u8) -> (u8,u8) {
    let mut value=Cons { first:n, rest:4u8 };
    let before=value.first;
    value=Cons { first:2, rest:value.rest };
    (before,value.first)
}
fn checked(n:u8)->u8 { assert!(n<7); n }
#[chialisp_oxide::puzzle]
pub fn discarded(n:u8)->u8 {
    let _value=Cons { first:7u8, rest:checked(n) };
    7
}
#[cfg(test)] mod tests {
 use super::*;
 #[test] fn native_fields_and_strict_initializer() {
  let tail=Tree::Atom(&[0,0,1]); let result=append(tail);
  assert_eq!(result.first,(50,[7;32])); assert_eq!(result.rest,tail);
  let computed = hash_prefix(b"abc", &tail);
  assert_eq!(computed.rest, &tail);
  assert_eq!(computed.first, [0xba,0x78,0x16,0xbf,0x8f,0x01,0xcf,0xea,0x41,0x41,0x40,0xde,0x5d,0xae,0x22,0x23,0xb0,0x03,0x61,0xa3,0x96,0x17,0x7a,0x9c,0xb4,0x10,0xff,0x61,0xf2,0x00,0x15,0xad]);
  let nested_result=nested(Cons { first:Cons {first:1,rest:2}, rest:tail });
  assert_eq!(nested_result.first,2); assert_eq!(nested_result.rest,tail);
  let nominal_result=nominal(Amount::from_mojos(7));
  assert_eq!(nominal_result.first.mojos(),7); assert_eq!(nominal_result.rest,3);
  assert_eq!(head(Cons{first:99,rest:8}),99);
  assert_eq!(snapshot(99),(99,2)); assert_eq!(discarded(6),7);
  assert!(std::panic::catch_unwind(||discarded(7)).is_err());
 }
}
"#;

fn success(output: std::process::Output) {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn find<'a>(programs: &'a [Program], name: &str) -> &'a Program {
    programs
        .iter()
        .find(|p| p.name.ends_with(&format!("::{name}")))
        .unwrap()
}
fn native_external(fixture: &Fixture, metadata: &std::path::Path) {
    success(
        Command::new(fixture.compiler())
            .args([
                "--crate-name",
                "native_pair_control",
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
}
fn native_tests(fixture: &Fixture) {
    success(
        Command::new(fixture.compiler().parent().unwrap().join("cargo"))
            .current_dir(&fixture.0)
            .env("RUSTC", fixture.compiler())
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER")
            .env_remove("CHIALISP_OXIDE_CARGO_MODE")
            .args(["test", "--offline", "--lib", "--target-dir"])
            .arg(fixture.0.join("native-build"))
            .output()
            .unwrap(),
    );
}
fn check(program: &Program, reference: &str, solutions: &[&str]) {
    let reference = chialisp_oxide_reference::compile(reference).unwrap();
    for optimize in [false, true] {
        let code = chialisp_oxide_ir::compile(program, optimize).unwrap();
        for solution in solutions {
            assert_eq!(
                vm::execute(&code, solution, 10_000_000).unwrap().outcome,
                vm::execute(&reference, solution, 10_000_000)
                    .unwrap()
                    .outcome,
                "{solution}, optimize={optimize}"
            );
        }
    }
}

#[test]
fn actual_sdk_struct_literals_fields_snapshots_and_strict_helpers_match_native_and_reference() {
    let fixture = Fixture::new();
    fixture.source(SOURCE);
    native_tests(&fixture);
    success(fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 7);
    for p in &programs {
        assert!(p.uses_pairs() && p.uses_runtime());
        assert_eq!(serde_json::to_value(p).unwrap()["ir_version"], 5);
    }
    let reference = format!("(mod (TAIL) (c (list 50 0x{}) TAIL))", "07".repeat(32));
    check(
        find(&programs, "append"),
        &reference,
        &[
            "(())",
            "(0x0000)",
            "(0x000001)",
            "((1 . 0xff))",
            "(((1 2) . 3))",
        ],
    );
    check(
        find(&programs, "hash_prefix"),
        "(mod (BYTES TAIL) (c (sha256 BYTES) TAIL))",
        &[
            "(() ())",
            "(0x616263 0x000001)",
            "(0x00 (1 . 2))",
            "(0x0000 ((1 2) . 3))",
            "(0x005c 0x000001)",
            "((1 . 2) ())",
        ],
    );
    check(
        find(&programs, "nested"),
        "(mod (((A . B) . TAIL)) (c B TAIL))",
        &[
            "(((1 . 2) . ()))",
            "(((0 . 255) . 0x000001))",
            "(((99 . 8) . (1 . 2)))",
        ],
    );
    check(
        find(&programs, "nominal"),
        "(mod (N) (c N 3))",
        &["(0)", "(7)", "(18446744073709551615)"],
    );
    check(
        find(&programs, "head"),
        "(mod ((A . B)) A)",
        &["((0 . 7))", "((99 . 255))", "((255 . 0))"],
    );
    check(
        find(&programs, "snapshot"),
        "(mod (N) (list N 2))",
        &["(0)", "(99)", "(255)"],
    );
    check(
        find(&programs, "discarded"),
        "(mod (N) (if (> 7 N) 7 (x)))",
        &["(0)", "(6)", "(7)", "(255)"],
    );
    for optimize in [false, true] {
        let code = chialisp_oxide_ir::compile(find(&programs, "head"), optimize).unwrap();
        for bad in ["()", "(1)", "((0x0063 . 7))", "((1 . 256))", "((1 . 2) 3)"] {
            assert_eq!(
                vm::execute(&code, bad, 10_000_000).unwrap().outcome,
                Outcome::Rejected,
                "{bad}"
            );
        }
    }
}

#[test]
fn actual_v5_cargo_sources_preserve_strict_raw_and_ignored_environments() {
    let fixture = Fixture::new();
    fixture.source(
        r#"
use chialisp_oxide::{Cons,Tree};
#[chialisp_oxide::puzzle]
pub fn strict(n:u8)->Cons<u8,u8>{Cons{first:n,rest:3}}
#[chialisp_oxide::puzzle]
pub fn strict_empty()->Cons<u8,u8>{Cons{first:1,rest:2}}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn raw<'a>(value:&'a Tree<'a>)->Cons<u8,&'a Tree<'a>>{Cons{first:7,rest:value}}
#[chialisp_oxide::puzzle(environment="ignored")]
pub fn ignored()->Cons<u8,u8>{Cons{first:1,rest:2}}
#[cfg(test)] mod tests {
 use super::*;
 #[test] fn native_same_source() {
  let env=Tree::Atom(&[0,0,1]); let result=raw(&env);
  assert_eq!(result.first,7); assert_eq!(result.rest,&env);
  assert_eq!(strict(99).first,99); assert_eq!(strict(99).rest,3);
  assert_eq!(strict_empty().first,1); assert_eq!(strict_empty().rest,2);
  assert_eq!(ignored().first,1); assert_eq!(ignored().rest,2);
 }
}
"#,
    );
    native_tests(&fixture);
    success(fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 4);
    for (name, environment) in [
        ("strict", "strict"),
        ("strict_empty", "strict"),
        ("raw", "raw"),
        ("ignored", "ignored"),
    ] {
        let program = find(&programs, name);
        let bytes = serde_json::to_vec(program).unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(wire["ir_version"], 5);
        assert_eq!(wire["environment"], environment);
        let decoded: Program = serde_json::from_slice(&bytes).unwrap();
        assert!(decoded.uses_pairs() && decoded.uses_runtime());
    }
    check(
        find(&programs, "strict"),
        "(mod (N) (c N 3))",
        &["(0)", "(99)", "(255)"],
    );
    check(find(&programs, "strict_empty"), "(mod () (c 1 2))", &["()"]);
    let arbitrary = ["()", "0x00", "0x000001", "(1 . 2)", "((1 . 2) . (3 . 4))"];
    check(find(&programs, "raw"), "(mod ENV (c 7 ENV))", &arbitrary);
    check(find(&programs, "ignored"), "(mod ENV (c 1 2))", &arbitrary);
    for optimize in [false, true] {
        for (name, inputs) in [
            (
                "strict",
                &["()", "(0x0063)", "(256)", "(1 . 2)", "(1 2)"][..],
            ),
            ("strict_empty", &["0x00", "(1)", "(1 . 2)"][..]),
        ] {
            let code = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            for input in inputs {
                assert_eq!(
                    vm::execute(&code, input, 10_000_000).unwrap().outcome,
                    Outcome::Rejected,
                    "{name}, {input}, optimize={optimize}"
                );
            }
        }
    }
}

#[test]
fn marker_is_explicit_and_cannot_admit_local_or_wrong_sdk_layouts() {
    let fixture = Fixture::new();
    fixture.source("#[doc=\"__chialisp_oxide_wire_pair_v1\"] pub struct Cons<H,T>{pub first:H,pub rest:T} #[chialisp_oxide::puzzle]pub fn puzzle()->Cons<u8,u16>{Cons{first:1,rest:2}}");
    success(fixture.cargo(false));
    let output = fixture.cargo(true);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("marked SDK two-field Cons"));
    for declaration in [
        "#[doc=\"__chialisp_oxide_wire_pair_v1\"] pub struct Cons<H,T>{pub first:T,pub rest:H}",
        "#[doc=\"__chialisp_oxide_wire_pair_v1\"] #[repr(C)] pub struct Cons<H,T>{pub first:H,pub rest:T}",
        "#[doc=\"__chialisp_oxide_wire_pair_v1\"] pub struct Cons<H,T>{pub first:H,pub rest:T} impl<H,T> Drop for Cons<H,T>{fn drop(&mut self){}}",
        "#[doc=\"__chialisp_oxide_wire_pair_v2\"] pub struct Cons<H,T>{pub first:H,pub rest:T}",
        "#[doc=\"__chialisp_oxide_wire_pair_v1\"] pub struct Cons<H,T>{pub first:(H,),pub rest:T}",
        "#[doc=\"__chialisp_oxide_wire_pair_v1\"] #[doc=\"__chialisp_oxide_wire_pair_v1\"] pub struct Cons<H,T>{pub first:H,pub rest:T}",
    ] {
        let fixture = Fixture::new();
        let metadata = fixture.dependency_source("chialisp_oxide", true, declaration);
        fixture.source(
            "use sdk::Cons; #[doc=\"__oxide_puzzle_v1\"]pub fn puzzle(_value:Cons<u8,u16>)->u8{7}",
        );
        native_external(&fixture, &metadata);
        let output = fixture.check_external("sdk", &metadata);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("marked SDK two-field Cons"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn a_marked_pair_does_not_admit_generic_constructor_calls() {
    let fixture = Fixture::new();
    let metadata=fixture.dependency_source("chialisp_oxide",true,"#[doc=\"__chialisp_oxide_wire_pair_v1\"]pub struct Cons<H,T>{pub first:H,pub rest:T} pub fn construct<H,T>(first:H,rest:T)->Cons<H,T>{Cons{first,rest}}");
    fixture.source(
        "#[doc=\"__oxide_puzzle_v1\"]pub fn puzzle()->sdk::Cons<u8,u16>{sdk::construct(1,2)}",
    );
    native_external(&fixture, &metadata);
    let output = fixture.check_external("sdk", &metadata);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("generic call"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_valid_owned_components_do_not_gain_cons_admission() {
    for component in [
        "OwnedTree<'a>",
        "Box<OwnedTree<'a>>",
        "(u8,OwnedTree<'a>)",
        "&'a OwnedTree<'a>",
    ] {
        let fixture = Fixture::new();
        let manifest = fixture.0.join("Cargo.toml");
        let contents = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(
            &manifest,
            contents.replace(
                "package=\"chialisp-oxide\",",
                "package=\"chialisp-oxide\",features=[\"alloc\"],",
            ),
        )
        .unwrap();
        fixture.source(&format!("extern crate alloc; use alloc::boxed::Box; use chialisp_oxide::{{Cons,OwnedTree}}; #[chialisp_oxide::puzzle]pub fn puzzle<'a>(value:Cons<{component},u8>)->u8{{value.rest}}"));
        success(fixture.cargo(false));
        let output = fixture.cargo(true);
        assert!(!output.status.success(), "{component}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("owned trees/boxes are unsupported"),
            "{component}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn marker_metadata_scan_is_bounded_before_collecting_markers() {
    let fixture = Fixture::new();
    let source = format!(
        "{} #[doc=\"__chialisp_oxide_wire_pair_v1\"]pub struct Cons<H,T>{{pub first:H,pub rest:T}}",
        "#[doc=\"ordinary documentation\"]".repeat(256)
    );
    let metadata = fixture.dependency_source("chialisp_oxide", true, &source);
    fixture.source("#[doc=\"__oxide_puzzle_v1\"]pub fn puzzle(_value:sdk::Cons<u8,u16>)->u8{7}");
    native_external(&fixture, &metadata);
    let output = fixture.check_external("sdk", &metadata);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("metadata attribute limit exceeded"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

const SOURCE_ORDER: &str = r#"
use chialisp_oxide::Cons;
fn checked(n: u8) -> u8 {
    assert!(n != 7);
    n
}
fn diverges_for_seven(n: u8) -> u8 {
    if n == 7 { diverges_for_seven(n) } else { n }
}
#[chialisp_oxide::puzzle]
pub fn written_rest_first(n: u8) -> Cons<u8, u8> {
    Cons { rest: checked(n), first: diverges_for_seven(n) }
}
#[chialisp_oxide::puzzle]
pub fn written_first_first(n: u8) -> Cons<u8, u8> {
    Cons { first: diverges_for_seven(n), rest: checked(n) }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_native_written_order_and_all_finite_values() {
        for n in 0..=u8::MAX {
            if n != 7 {
                let a=written_rest_first(n); let b=written_first_first(n);
                assert_eq!((a.first,a.rest),(n,n));
                assert_eq!((b.first,b.rest),(n,n));
            }
        }
        assert!(std::panic::catch_unwind(||written_rest_first(7)).is_err());
        // written_first_first(7) deliberately diverges and is never run natively.
    }
}
"#;

#[test]
fn rust_written_field_order_precedes_final_pair_wire_order() {
    use chialisp_oxide_backend_experiment::FailureCode;
    let fixture = Fixture::new();
    fixture.source(SOURCE_ORDER);
    native_tests(&fixture);
    success(fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 2);
    let reference =
        chialisp_oxide_reference::compile("(mod (N) (if (= N 7) (x) (c N N)))").unwrap();
    for program in &programs {
        assert!(program.uses_pairs() && program.uses_runtime());
        let rest_first = program.name.ends_with("::written_rest_first");
        let optimized = chialisp_oxide_ir::optimize(program).unwrap();
        let transported: Program =
            serde_json::from_value(serde_json::to_value(&optimized).unwrap()).unwrap();
        assert!(transported.uses_pairs() && transported.uses_runtime());
        for candidate in [program, &transported] {
            for optimize in [false, true] {
                let code = chialisp_oxide_ir::compile(candidate, optimize).unwrap();
                for n in 0..=u8::MAX {
                    let input = format!("({n})");
                    if n == 7 && !rest_first {
                        // The first-written initializer really diverges. A VM
                        // limit is inconclusive and never a policy rejection.
                        let observed =
                            chialisp_oxide_backend_experiment::execute(&code, &input, 1_000_000)
                                .unwrap();
                        assert_eq!(observed.outcome, Outcome::Inconclusive);
                        assert_eq!(observed.failure.unwrap().code(), FailureCode::CostExceeded);
                    } else {
                        let observed = vm::execute(&code, &input, 1_000_000).unwrap();
                        assert_eq!(
                            observed.outcome,
                            vm::execute(&reference, &input, 1_000_000).unwrap().outcome,
                            "{}, n={n}, optimize={optimize}",
                            program.name,
                        );
                        if n == 7 {
                            assert_eq!(observed.failure.unwrap().code(), FailureCode::Raise);
                        }
                    }
                }
            }
        }
    }
}
