//! Version-8 standard Option entry/match semantics from actual Cargo Rust.
//! Native and independent Chialisp policies cover finite typed cases; complete
//! malformed-entry checks remain a separate lane, with no consensus claim.
mod support;
#[path = "support/vm.rs"]
mod vm;
use chialisp_oxide_backend_experiment::Outcome;
use std::process::Command;
use support::Fixture;

const SOURCE: &str = include_str!("fixtures/optional_inputs.rs");

fn native(fixture: &Fixture) {
    let output = Command::new(fixture.compiler().parent().unwrap().join("cargo"))
        .current_dir(&fixture.0)
        .env("RUSTC", fixture.compiler())
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CHIALISP_OXIDE_CARGO_MODE")
        .args(["test", "--lib", "--offline", "--target-dir"])
        .arg(fixture.0.join("native-build"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn reference(name: &str) -> &'static str {
    match name {
        "increment" => "(mod (V) (if (l V) (if (> (f V) 254) (x) (list (+ (f V) 1))) ()))",
        "forward" | "raw" => "(mod (V) V)",
        "scalar" => "(mod (V) (if (l V) (if (> (f V) 254) (x) (+ (f V) 1)) 7))",
        "ignored" | "array_input" => "(mod (V) 7)",
        "unit" => "(mod (V) (l V))",
        "nested" => "(mod (V) (if (l V) (if (l (f V)) (list 1 (f (f V))) (list 1 0)) (list 0 0)))",
        "tuple" => {
            "(mod (V) (if (l (f (r V))) (list (f V) (f (f (f (r V)))) (f (r (f (f (r V)))))) (list (f V) 0 0)))"
        }
        "pair" => "(mod (V) (list (f V) (if (l (r V)) (f (r V)) 0)))",
        "record" => {
            "(mod (V) (if (l V) (if (> (f (f V)) 254) (x) (list (list (+ (f (f V)) 1) (f (r (f V)))))) ()))"
        }
        "bytes" => "(mod (V) (if (l V) (f V) 0x0000))",
        "if_let" | "wrapped" => "(mod (V) (if (l V) (f V) 9))",
        "snapshot" => "(mod (V N) (list (list N) (if (l V) (f V) 0)))",
        "guarded" => "(mod (V D) (if (l V) (if D (/ (f V) D) (x)) 7))",
        "cast" => "(mod (V) (if (l V) (logand (f V) 255) 0))",
        "from_helper" => "(mod (C N) (if C N 9))",
        "strict" => "(mod (V D) (if D (if (l V) (f V) 7) (x)))",
        _ => panic!("{name}"),
    }
}
fn cases(name: &str) -> &'static [(&'static str, &'static str)] {
    match name {
        "increment" => &[("(())", "()"), ("((7))", "(8)"), ("((0))", "(1)")],
        "forward" => &[("(())", "()"), ("((0))", "(0)"), ("((255))", "(255)")],
        "scalar" => &[("(())", "7"), ("((7))", "8")],
        "ignored" => &[("(())", "7"), ("((255))", "7")],
        "unit" => &[("(())", "()"), ("((()))", "1")],
        "nested" => &[
            ("(())", "(() ())"),
            ("((()))", "(1 ())"),
            ("(((7)))", "(1 7)"),
        ],
        "tuple" => &[
            ("((513 ()))", "(513 () ())"),
            ("((513 ((7 1))))", "(513 7 1)"),
        ],
        "pair" => &[("((7))", "(7 ())"), ("((7 513))", "(7 513)")],
        "record" => &[("(())", "()"), ("(((7 513)))", "((8 513))")],
        "bytes" => &[("(())", "0x0000"), ("((0x0001))", "0x0001")],
        "if_let" | "wrapped" => &[("(())", "9"), ("((7))", "7")],
        "snapshot" => &[("(() 9)", "((9) ())"), ("((7) 9)", "((9) 7)")],
        "guarded" => &[("(() 0)", "7"), ("((9) 3)", "3")],
        "cast" => &[("(())", "()"), ("((513))", "1")],
        "from_helper" => &[("(() 7)", "9"), ("(1 7)", "7")],
        "array_input" => &[("((() (255)))", "7")],
        "strict" => &[("(() 3)", "7"), ("((9) 3)", "9")],
        "raw" => &[
            ("(())", "()"),
            ("((()))", "(())"),
            ("(((0x0001 . 0xff)))", "((0x0001 . 0xff))"),
        ],
        _ => panic!("{name}"),
    }
}
fn count_presence(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(fields) => {
            usize::from(
                fields.get("op").and_then(|value| value.as_str()) == Some("optional_is_some"),
            ) + fields.values().map(count_presence).sum::<usize>()
        }
        serde_json::Value::Array(values) => values.iter().map(count_presence).sum(),
        _ => 0,
    }
}

#[test]
fn actual_optional_entries_and_matches_keep_native_shapes_and_strictness() {
    let fixture = Fixture::with_sdk_alias("chialisp_oxide");
    fixture.source(SOURCE);
    native(&fixture);
    let output = fixture.cargo(true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let programs = fixture.exports();
    assert_eq!(programs.len(), 19);
    for program in programs {
        let name = program.name.rsplit("::").next().unwrap();
        program.validate().unwrap();
        assert!(program.uses_optional_access(), "{name}");
        assert_eq!(
            serde_json::to_value(&program).unwrap()["ir_version"],
            8,
            "{name}"
        );
        if name == "scalar" {
            assert_eq!(
                count_presence(&serde_json::to_value(&program).unwrap()),
                1,
                "a single source discriminant must capture its optional predicate once"
            );
        }
        let independent = chialisp_oxide_reference::compile(reference(name)).unwrap();
        for optimize in [false, true] {
            let code = chialisp_oxide_ir::compile(&program, optimize).unwrap();
            for (solution, expected) in cases(name) {
                let quoted =
                    chialisp_oxide_reference::compile(&format!("(mod () (q . {expected}))"))
                        .unwrap();
                let expected = vm::execute(&quoted, "()", 20_000_000).unwrap().outcome;
                assert_eq!(
                    vm::execute(&independent, solution, 20_000_000)
                        .unwrap()
                        .outcome,
                    expected,
                    "independent{name}/{solution}"
                );
                assert_eq!(
                    vm::execute(&code, solution, 20_000_000).unwrap().outcome,
                    expected,
                    "{name}/{optimize}/{solution}"
                );
            }
            let failures: &[&str] = match name {
                "scalar" | "increment" => &["((255))"],
                "record" => &["(((255 513)))"],
                "guarded" => &["((9) 0)"],
                "strict" => &["(() 0)", "((9) 0)"],
                _ => &[],
            };
            for solution in failures {
                let result = vm::execute(&code, solution, 20_000_000).unwrap();
                assert_eq!(result.outcome, Outcome::Rejected, "{name}/{solution}");
                assert_eq!(
                    format!("{:?}", result.failure.unwrap().code()),
                    "Raise",
                    "{name}/{solution}"
                );
                assert_eq!(
                    vm::execute(&independent, solution, 20_000_000)
                        .unwrap()
                        .outcome,
                    Outcome::Rejected
                );
            }
            if ["forward", "scalar", "ignored", "if_let", "wrapped"].contains(&name) {
                for bad in [
                    "()",
                    "(0x00)",
                    "(1)",
                    "((1 2))",
                    "((1 . 2))",
                    "((256))",
                    "((-1))",
                    "((0x0001))",
                    "(((1)))",
                    "(() ())",
                    "(() . 1)",
                ] {
                    assert_eq!(
                        vm::execute(&code, bad, 20_000_000).unwrap().outcome,
                        Outcome::Rejected,
                        "{name}/{optimize}/{bad}"
                    );
                }
            }
            let nested_malformed: &[&str] = match name {
                "array_input" => &["((() (0x0001)))", "((() (256)))", "((0x00 (1)))"],
                "pair" => &["((7 . 0x00))", "((7 65536))", "((7 0x0001))"],
                "tuple" => &["((513 ((7 2))))", "((513 ((0x0007 1))))"],
                _ => &[],
            };
            for bad in nested_malformed {
                assert_eq!(
                    vm::execute(&code, bad, 20_000_000).unwrap().outcome,
                    Outcome::Rejected,
                    "{name}/{optimize}/{bad}"
                );
            }
            if name == "record" {
                for bad in [
                    "(((7)))",
                    "(((7 513 1)))",
                    "(((7 . 513)))",
                    "(((0x0007 513)))",
                    "(((7 -1)))",
                ] {
                    assert_eq!(
                        vm::execute(&code, bad, 20_000_000).unwrap().outcome,
                        Outcome::Rejected,
                        "{name}/{bad}"
                    );
                }
            }
            if name == "unit" {
                for bad in ["(0x00)", "((0x00))", "((1))", "((() ()))"] {
                    assert_eq!(
                        vm::execute(&code, bad, 20_000_000).unwrap().outcome,
                        Outcome::Rejected,
                        "unit/{bad}"
                    );
                }
            }
            if name == "bytes" {
                for bad in ["((0x00))", "((0x000001))", "(((0 1)))"] {
                    assert_eq!(
                        vm::execute(&code, bad, 20_000_000).unwrap().outcome,
                        Outcome::Rejected,
                        "bytes/{bad}"
                    );
                }
            }
            if name == "nested" {
                for bad in ["((0x00))", "(((256)))", "(((0x0001)))", "(((7 8)))"] {
                    assert_eq!(
                        vm::execute(&code, bad, 20_000_000).unwrap().outcome,
                        Outcome::Rejected,
                        "nested/{bad}"
                    );
                }
            }
        }
    }
}

#[test]
fn optional_inputs_do_not_admit_methods_borrows_or_hidden_payloads() {
    let fixture = Fixture::with_sdk_alias("chialisp_oxide");
    for source in [
        "#[chialisp_oxide::puzzle]pub fn p(v:Option<u8>)->bool{v.is_some()}",
        "#[chialisp_oxide::puzzle]pub fn p(v:Option<u8>)->u8{v.unwrap()}",
        "#[chialisp_oxide::puzzle]pub fn p(v:&Option<u8>)->u8{match v{Some(x)=>*x,None=>0}}",
        "#[chialisp_oxide::puzzle]pub fn p(v:Option<u8>)->u8{match v{Some(ref x)=>*x,None=>0}}",
        "#[chialisp_oxide::puzzle]pub fn p(_:Option<[&'static u8;0]>)->u8{7}",
        "pub struct Dropped{value:u8}impl Drop for Dropped{fn drop(&mut self){}}#[chialisp_oxide::puzzle]pub fn p(_:Option<Dropped>)->u8{7}",
        "#[chialisp_oxide::puzzle]pub fn p(_:Option<Box<u8>>)->u8{7}",
        "#[chialisp_oxide::puzzle]pub fn p(_:Option<usize>)->u8{7}",
        "pub enum Option<T>{None,Some(T)}#[chialisp_oxide::puzzle]pub fn p(v:Option<u8>)->u8{match v{Option::Some(x)=>x,Option::None=>0}}",
        "const VALUE:Option<u8>=Some(7);#[chialisp_oxide::puzzle]pub fn p()->Option<u8>{VALUE}",
        "use chialisp_oxide::Tree; const fn choose<'a>(v:Option<&'a Tree<'a>>)->&'a Tree<'a>{match v{Some(tree)=>tree,None=>panic!()}} const NIL:Tree<'static>=Tree::Atom(&[]); const CHOSEN:&'static Tree<'static>=choose(Some(&NIL)); #[chialisp_oxide::puzzle]pub fn p()->&'static Tree<'static>{CHOSEN}",
    ] {
        fixture.source(source);
        let native = fixture.cargo(false);
        assert!(
            native.status.success(),
            "native {}",
            String::from_utf8_lossy(&native.stderr)
        );
        let output = fixture.cargo(true);
        assert!(!output.status.success(), "unexpected admission: {source}");
        assert!(fixture.exports().is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("internal compiler error"));
    }
}
