//! Source-derived runtime calls; native recursion is exercised only at finite,
//! small depths. Nontermination controls run exclusively in the cost-bound VM.
mod support;
#[path = "support/vm.rs"]
mod vm;

use std::{path::Path, process::Command};

use chialisp_oxide_backend_experiment::{FailureCode, Outcome};
use chialisp_oxide_ir::Program;
use support::Fixture;

const SOURCE: &str = r#"
use chialisp_oxide::{Tree,quote};

#[chialisp_oxide::puzzle(environment="raw")]
pub fn rightmost<'a>(value:&'a Tree<'a>)->&'a Tree<'a> {
    match value { Tree::Pair(_,rest)=>rightmost(rest),Tree::Atom(_)=>value }
}
fn even<'a>(value:&'a Tree<'a>)->&'a Tree<'a> {
    match value { Tree::Pair(_,rest)=>odd(rest),Tree::Atom(_)=>value }
}
fn odd<'a>(value:&'a Tree<'a>)->&'a Tree<'a> {
    match value { Tree::Pair(_,rest)=>even(rest),Tree::Atom(_)=>value }
}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn mutual<'a>(value:&'a Tree<'a>)->&'a Tree<'a> {even(value)}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn quoted<'a>(value:&'a Tree<'a>)->Tree<'a> {
    match value {Tree::Pair(_,rest)=>quoted(rest),Tree::Atom(_)=>quote(value)}
}
#[chialisp_oxide::puzzle]
pub fn sum(value:u8)->u8 {if value==0 {0} else {value+sum(value-1)}}
#[chialisp_oxide::puzzle]
pub fn unused(value:u8)->u8 {let _=sum(value);7}
#[chialisp_oxide::puzzle]
pub fn lazy(choose:bool,value:u8)->u8 {if choose {sum(value)}else{7}}
fn validate(value:u8) {assert!(value<=10);}
#[chialisp_oxide::puzzle]
pub fn checked(value:u8)->u8 {validate(value);sum(value)}
#[allow(unconditional_recursion)]
fn spin(value:u8)->u8 {spin(value)}
fn reject(_:u8)->u8 {panic!()}
fn first(value:u8,_:u8)->u8 {value}
#[chialisp_oxide::puzzle]
pub fn ignored_spin(value:u8,_unused:bool)->u8 {let _=spin(value);7}
#[chialisp_oxide::puzzle]
pub fn spin_before_reject(value:u8)->u8 {first(spin(value),reject(value))}
#[chialisp_oxide::puzzle]
pub fn reject_before_spin(value:u8)->u8 {first(reject(value),spin(value))}
#[chialisp_oxide::puzzle]
pub fn legacy(value:u8)->u8 {value}
mod core {pub mod panicking {
    pub fn panic(value:u8)->u8 {value}
    pub fn panic_fmt(value:u8)->u8 {value}
    pub fn panic_explicit(value:u8)->u8 {value}
}}
#[chialisp_oxide::puzzle]
pub fn legacy_named_panic(value:u8)->u8 {
    core::panicking::panic_explicit(core::panicking::panic_fmt(core::panicking::panic(value)))
}
#[chialisp_oxide::puzzle]
pub fn recursive_named_panic(value:u8)->u8 {
    if value==0 {core::panicking::panic(7)} else {recursive_named_panic(value-1)}
}
fn scoped_field<'a>(value:&'a Tree<'a>,bytes:[u8;2])->u8 {
    let [_,selected]=bytes;
    match value {Tree::Pair(_,rest)=>scoped_field(rest,bytes),Tree::Atom(_)=>selected}
}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn helper_byte_scope<'a>(value:&'a Tree<'a>)->u8 {scoped_field(value,[0,128])}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn finite_native_recursion_has_the_expected_values_and_panics() {
        let mut value:&'static Tree<'static>=Box::leak(Box::new(Tree::Atom(&[0,0,1,255])));
        for _ in 0..128 {value=Box::leak(Box::new(Tree::Pair(&chialisp_oxide::QUOTE,value)));}
        assert_eq!(rightmost(value),&Tree::Atom(&[0,0,1,255]));
        assert_eq!(mutual(value),rightmost(value));
        assert_eq!(quoted(value),Tree::Pair(&chialisp_oxide::QUOTE,rightmost(value)));
        for n in 0..=22 {assert_eq!(sum(n),((n as u16)*(n as u16+1)/2)as u8);}
        assert_eq!(unused(22),7);
        assert_eq!(lazy(false,255),7);
        assert_eq!(checked(10),55);
        assert_eq!(legacy_named_panic(42),42);
        assert_eq!(recursive_named_panic(5),7);
        assert_eq!(helper_byte_scope(value),128);
        assert!(std::panic::catch_unwind(||sum(23)).is_err());
        assert!(std::panic::catch_unwind(||unused(23)).is_err());
        assert!(std::panic::catch_unwind(||checked(11)).is_err());
        // spin/argument-order controls are deliberately never called natively.
    }
}
"#;

fn success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn native_tests(fixture: &Fixture) {
    let compiler = fixture.compiler();
    success(
        &Command::new(compiler.parent().unwrap().join("cargo"))
            .args(["test", "--lib", "--offline", "--target-dir"])
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

fn expected(input: &str) -> Outcome {
    vm::execute(&[1], input, 1_000_000).unwrap().outcome
}

#[test]
fn cargo_checked_self_mutual_and_non_tail_recursion_preserve_native_semantics() {
    let fixture = Fixture::new();
    fixture.source(SOURCE);
    native_tests(&fixture);
    success(&fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 14);
    for program in &programs {
        program.validate().unwrap();
        let wire = serde_json::to_value(program).unwrap();
        if program.name.ends_with("::legacy") || program.name.ends_with("::legacy_named_panic") {
            assert!(program.functions.is_empty());
            assert!(wire.get("ir_version").is_none());
        } else {
            assert!(!program.functions.is_empty());
            assert_eq!(wire["ir_version"], 4);
            assert_eq!(program.functions[0].name, program.name);
            for (index, function) in program.functions.iter().enumerate() {
                assert_eq!(function.id, index);
            }
        }
    }
    for optimize in [false, true] {
        for name in ["rightmost", "mutual", "quoted"] {
            let bytes = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            for depth in [0, 1, 2, 8, 64, 128] {
                let mut input = "0x000001ff".to_string();
                for _ in 0..depth {
                    input = format!("(0x8000 . {input})");
                }
                let value = if name == "quoted" {
                    "(1 . 0x000001ff)"
                } else {
                    "0x000001ff"
                };
                assert_eq!(
                    vm::execute(&bytes, &input, 10_000_000).unwrap().outcome,
                    expected(value),
                    "{name}, optimize={optimize}, depth={depth}"
                );
            }
            assert!(
                bytes.len() < 16_384,
                "source recursion must not unroll to the input depth"
            );
        }
        let scoped =
            chialisp_oxide_ir::compile(find(&programs, "helper_byte_scope"), optimize).unwrap();
        assert_eq!(
            vm::execute(&scoped, "(1 . (2 . 3))", 1_000_000)
                .unwrap()
                .outcome,
            expected("128")
        );
        let sum = chialisp_oxide_ir::compile(find(&programs, "sum"), optimize).unwrap();
        for value in 0u16..=22 {
            assert_eq!(
                vm::execute(&sum, &format!("({value})"), 10_000_000)
                    .unwrap()
                    .outcome,
                expected(&(value * (value + 1) / 2).to_string())
            );
        }
        for (name, solution, wanted) in [
            ("sum", "(23)", None),
            ("unused", "(23)", None),
            ("checked", "(11)", None),
            ("unused", "(22)", Some("7")),
            ("checked", "(10)", Some("55")),
            ("lazy", "(() 255)", Some("7")),
            ("lazy", "(1 23)", None),
            ("legacy_named_panic", "(42)", Some("42")),
            ("recursive_named_panic", "(5)", Some("7")),
        ] {
            let bytes = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            let actual = vm::execute(&bytes, solution, 10_000_000).unwrap();
            if let Some(wanted) = wanted {
                assert_eq!(actual.outcome, expected(wanted));
            } else {
                assert_eq!(actual.outcome, Outcome::Rejected);
                assert_eq!(actual.failure.unwrap().code(), FailureCode::Raise);
            }
        }
        for name in ["ignored_spin", "spin_before_reject", "reject_before_spin"] {
            let bytes = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            // These intentional nontermination cases bypass the conclusive
            // helper: cost exhaustion is the expected Resource outcome.
            let solution = if name == "ignored_spin" {
                "(1 1)"
            } else {
                "(1)"
            };
            let actual =
                chialisp_oxide_backend_experiment::execute(&bytes, solution, 100_000).unwrap();
            if name == "reject_before_spin" {
                assert_eq!(actual.outcome, Outcome::Rejected);
                assert_eq!(actual.failure.unwrap().code(), FailureCode::Raise);
            } else {
                assert_eq!(actual.outcome, Outcome::Inconclusive);
                assert_eq!(actual.failure.unwrap().code(), FailureCode::CostExceeded);
            }
            if name == "ignored_spin" {
                for malformed in ["(1 2)", "(1 0x00)", "(1 (1))", "(1 . 1)", "(1)", "(1 1 1)"] {
                    assert_eq!(
                        vm::execute(&bytes, malformed, 100_000).unwrap().outcome,
                        Outcome::Rejected,
                        "unused typed argument must reject before spinning: {malformed}"
                    );
                }
            }
        }
    }
}

#[test]
fn recursive_functions_keep_unsupported_features_and_borrow_errors_fail_closed() {
    for (source, diagnostic) in [
        (
            "fn rec(v:u8)->(u8,u8){let mut pair=(0,0);pair.0=v;if v==0{pair}else{rec(v-1)}} #[chialisp_oxide::puzzle]pub fn invalid(v:u8)->(u8,u8){rec(v)}",
            "projected assignment",
        ),
        (
            "#[chialisp_oxide::puzzle]pub fn invalid(v:u8)->u8{if v==0 {let mut n=0;while n<2{n+=1;}n}else{invalid(v-1)}}",
            "loops are unsupported",
        ),
        (
            "#[chialisp_oxide::puzzle]pub fn invalid(v:u8)->u8{let function:fn(u8)->u8=invalid;function(v)}",
            "indirect function calls",
        ),
        (
            "fn rec<T>(v:T)->T{rec(v)} #[chialisp_oxide::puzzle]pub fn invalid(v:u8)->u8{rec(v)}",
            "generic call",
        ),
        (
            "#[chialisp_oxide::puzzle]pub fn invalid(v:u8)->u8{if v==0{1}else{invalid(v-1)/v}}",
            "unsupported integer operation",
        ),
        (
            "#[chialisp_oxide::puzzle]pub fn invalid(v:u8)->u8{unsafe{invalid(v)}}",
            "usage of an `unsafe` block",
        ),
        (
            "use chialisp_oxide::Tree; #[chialisp_oxide::puzzle(environment=\"raw\")]pub fn invalid<'a>(v:&'a Tree<'a>)->&'a Tree<'a>{let local=*v;if false{invalid(v)}else{&local}}",
            "cannot return reference to local variable",
        ),
    ] {
        let fixture = Fixture::new();
        fixture.source(&format!(
            "#[chialisp_oxide::puzzle]pub fn valid()->u8{{7}}\n{source}"
        ));
        let output = fixture.cargo(true);
        assert!(!output.status.success(), "accepted {source}");
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(errors.contains(diagnostic), "{diagnostic}: {errors}");
        assert!(!errors.contains("internal compiler error"), "{errors}");
        assert!(
            fixture.exports().is_empty(),
            "must not emit partial exports"
        );
    }
}

#[test]
fn runtime_sdk_calls_use_actual_helper_and_promoted_constant_mir() {
    let fixture = Fixture::new();
    let metadata = fixture.dependency_source(
        "chialisp_oxide",
        true,
        r#"
        #[derive(Clone,Copy)]pub enum Tree<'a>{Atom(&'a [u8]),Pair(&'a Tree<'a>,&'a Tree<'a>)}
        pub const QUOTE:Tree<'static>=Tree::Atom(&[2]);
        pub fn quote<'a>(value:&'a Tree<'a>)->Tree<'a>{Tree::Pair(&QUOTE,value)}
    "#,
    );
    fixture.source(r#"
        #[doc="__chialisp_oxide_puzzle_v3_environment_raw"]
        pub fn entry<'a>(value:&'a fake::Tree<'a>)->fake::Tree<'a>{
            match value{fake::Tree::Pair(_,rest)=>entry(rest),fake::Tree::Atom(_)=>fake::quote(value)}
        }
    "#);
    success(&fixture.check_external("fake", &metadata));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 1);
    assert!(!programs[0].functions.is_empty());
    for optimize in [false, true] {
        let bytes = chialisp_oxide_ir::compile(&programs[0], optimize).unwrap();
        assert_eq!(
            vm::execute(&bytes, "(7 . (8 . 0x0001))", 1_000_000)
                .unwrap()
                .outcome,
            expected("(2 . 0x0001)")
        );
    }
    std::fs::remove_dir_all(fixture.0.join("exports")).unwrap();
    let altered = fixture.dependency_source(
        "chialisp_oxide",
        true,
        "pub enum Tree<'a>{Atom(&'a [u8]),Pair(&'a Tree<'a>,u8)}",
    );
    fixture.source(r#"#[doc="__chialisp_oxide_puzzle_v3_environment_raw"] pub fn invalid(v:&fake::Tree<'_>)->u8{invalid(v)}"#);
    let output = fixture.check_external("fake", &altered);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("borrowed trees require exactly"));
    assert!(fixture.exports().is_empty());
}

#[test]
fn standalone_recursive_fixture_is_finite_code_and_a_resource_outcome() {
    let output = Command::new(env!("CARGO_BIN_EXE_chialisp-oxide-driver"))
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/recursion.rs"))
        .output()
        .unwrap();
    success(&output);
    let program: Program = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(program.functions.len(), 1);
    for optimize in [false, true] {
        let bytes = chialisp_oxide_ir::compile(&program, optimize).unwrap();
        assert!(bytes.len() < 4096);
        let actual = chialisp_oxide_backend_experiment::execute(&bytes, "(1)", 100_000).unwrap();
        assert_eq!(actual.outcome, Outcome::Inconclusive);
        assert_eq!(actual.failure.unwrap().code(), FailureCode::CostExceeded);
    }
}
