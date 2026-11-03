//! Real local Rust records, native checks, strict table IR and exact VM outputs.
mod support;
#[path = "support/vm.rs"]
mod vm;
use chialisp_oxide_backend_experiment::Outcome;
use chialisp_oxide_ir::Type;
use std::process::Command;
use support::Fixture;

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
fn expected(value: &str) -> Outcome {
    let code = chialisp_oxide_reference::compile(&format!("(mod () (q . {value}))")).unwrap();
    vm::execute(&code, "()", 20_000_000).unwrap().outcome
}

#[test]
fn actual_native_records_preserve_declared_order_moves_updates_and_strict_values() {
    let fixture = Fixture::with_sdk_alias("chialisp_oxide");
    fixture.source(include_str!("fixtures/named_structs.rs"));
    native(&fixture);
    let output = fixture.cargo(true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let programs = fixture.exports();
    assert_eq!(programs.len(), 8);
    for program in programs {
        let name = program.name.rsplit("::").next().unwrap();
        assert!(
            !program.functions.is_empty(),
            "{name} must retain strict runtime captures"
        );
        let (solution, value) = match name {
            "roundtrip" => ("((7 513 1))", "(7 513 1)"),
            "construct" => ("(7 513 1)", "(7 513 1)"),
            "nested" => (
                "(((7 513 1) (-129 () 0x0080ff) 99))",
                "((7 513 1) (-129 () 0x0080ff) 99)",
            ),
            "update" => (
                "(((7 513 1) (-129 () 0x0080ff) 99) 2)",
                "(((9 513 1) (-130 () 0x0080ff) 101) 7)",
            ),
            "moved" => ("((7 513 1) 2)", "((7 513 1) (2 514 ()))"),
            "ignored" => ("((7 513 1))", "7"),
            "strict" => ("(3)", "7"),
            "same_name" => ("((513 7))", "(513 7)"),
            _ => panic!("{name}"),
        };
        if name == "roundtrip" {
            assert_eq!(
                program.parameters[0].ty,
                Type::Tuple {
                    items: vec![
                        Type::Int {
                            bits: 8,
                            signed: false
                        },
                        Type::Int {
                            bits: 64,
                            signed: false
                        },
                        Type::Bool,
                    ]
                }
            );
        }
        for optimize in [false, true] {
            let code = chialisp_oxide_ir::compile(&program, optimize).unwrap();
            assert_eq!(
                vm::execute(&code, solution, 20_000_000).unwrap().outcome,
                expected(value),
                "{name}/{optimize}"
            );
            if name == "strict" {
                assert_eq!(
                    vm::execute(&code, "(0)", 20_000_000).unwrap().outcome,
                    Outcome::Rejected
                );
            }
            if name == "roundtrip" || name == "ignored" {
                for malformed in [
                    "()",
                    "((7 513))",
                    "((7 513 1 2))",
                    "((7 513 . 1))",
                    "((0x0007 513 1))",
                    "((256 513 1))",
                    "((7 -1 1))",
                    "((7 18446744073709551616 1))",
                    "((7 513 2))",
                    "((7 (513) 1))",
                    "((7 513 1) ())",
                ] {
                    assert_eq!(
                        vm::execute(&code, malformed, 20_000_000).unwrap().outcome,
                        Outcome::Rejected,
                        "{name}/{optimize}/{malformed}"
                    );
                }
            }
            if name == "update" {
                for bad in [
                    "(((255 513 1) (-129 () 0x0080ff) 99) 1)",
                    "(((7 513 1) (-32768 () 0x0080ff) 99) 2)",
                    "(((7 513 1) (-129 () 0x0080ff) 18446744073709551615) 2)",
                ] {
                    assert_eq!(
                        vm::execute(&code, bad, 20_000_000).unwrap().outcome,
                        Outcome::Rejected
                    );
                }
            }
        }
    }
}

#[test]
fn native_valid_unsupported_records_fail_closed_without_partial_exports() {
    for (label, source) in [
        (
            "generic",
            "pub struct Record<T>{pub item:T} #[chialisp_oxide::puzzle] pub fn p(v:Record<u8>)->Record<u8>{v}",
        ),
        (
            "drop",
            "pub struct Record{pub item:u8} impl Drop for Record{fn drop(&mut self){}} #[chialisp_oxide::puzzle] pub fn p(v:Record)->Record{v}",
        ),
        (
            "nested-drop",
            "pub struct Inner{pub item:u8} impl Drop for Inner{fn drop(&mut self){}} pub struct Record{pub item:Inner} #[chialisp_oxide::puzzle] pub fn p(v:Record)->Record{v}",
        ),
        (
            "reference",
            "pub struct Record{pub item:&'static u8} #[chialisp_oxide::puzzle] pub fn p(v:Record)->Record{v}",
        ),
        (
            "zero-reference",
            "pub struct Record{pub item:[&'static u8;0]} #[chialisp_oxide::puzzle] pub fn p(v:Record)->Record{v}",
        ),
        (
            "option",
            "pub struct Record{pub item:Option<u8>} #[chialisp_oxide::puzzle] pub fn p(v:Record)->Record{v}",
        ),
        (
            "pair",
            "pub struct Record{pub item:chialisp_oxide::Cons<u8,u8>} #[chialisp_oxide::puzzle] pub fn p(v:Record)->Record{v}",
        ),
        (
            "external",
            "pub struct Record{pub item:std::ops::Range<u8>} #[chialisp_oxide::puzzle] pub fn p(v:Record)->Record{v}",
        ),
        (
            "enum",
            "pub enum Inner{A(u8),B} pub struct Record{pub item:Inner} #[chialisp_oxide::puzzle] pub fn p(v:Record)->Record{v}",
        ),
        (
            "platform",
            "pub struct Record{pub item:usize} #[chialisp_oxide::puzzle] pub fn p(v:Record)->Record{v}",
        ),
        (
            "repr-c",
            "#[repr(C)] pub struct Record{pub item:u8} #[chialisp_oxide::puzzle] pub fn p(v:Record)->Record{v}",
        ),
        (
            "repr-packed",
            "#[repr(packed)] pub struct Record{pub item:u8} #[chialisp_oxide::puzzle] pub fn p(v:Record)->Record{v}",
        ),
        (
            "repr-align",
            "#[repr(align(16))] pub struct Record{pub item:u8} #[chialisp_oxide::puzzle] pub fn p(v:Record)->Record{v}",
        ),
        (
            "macro",
            "macro_rules! record{()=>{pub struct Record{pub item:u8}}} record!(); #[chialisp_oxide::puzzle] pub fn p(v:Record)->Record{v}",
        ),
        (
            "empty",
            "pub struct Record{} #[chialisp_oxide::puzzle] pub fn p(v:Record)->Record{v}",
        ),
    ] {
        let fixture = Fixture::with_sdk_alias("chialisp_oxide");
        fixture.source(source);
        assert!(
            fixture.cargo(false).status.success(),
            "native-valid control {label}"
        );
        let output = fixture.cargo(true);
        assert!(!output.status.success(), "accepted {label}");
        assert!(fixture.exports().is_empty(), "partial export {label}");
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(
            !errors.contains("internal compiler error"),
            "{label}: {errors}"
        );
        assert!(
            errors.contains("struct") || errors.contains("transparent"),
            "{label}: {errors}"
        );
    }
}

#[test]
fn rustc_keeps_struct_nominality_before_erasing_the_wire_shape() {
    let fixture = Fixture::with_sdk_alias("chialisp_oxide");
    fixture.source("pub struct A{pub value:u8} pub struct B{pub value:u8} #[chialisp_oxide::puzzle] pub fn p(value:A)->B{value}");
    let output = fixture.cargo(true);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("E0308"));
    assert!(fixture.exports().is_empty());
}

#[test]
fn actual_record_graphs_are_bounded_before_logical_expansion() {
    for (label, source) in [
        ("fields", format!("pub struct Record{{{}}} #[chialisp_oxide::puzzle] pub fn p(v:Record)->Record{{v}}", (0..257).map(|i| format!("pub f{i}:u8,")).collect::<String>())),
        ("depth", format!("{} #[chialisp_oxide::puzzle] pub fn p(v:R32)->R32{{v}}", (0..=32).map(|i| if i==0 {"pub struct R0{pub value:u8}".to_string()} else {format!("pub struct R{i}{{pub value:R{}}}", i-1)}).collect::<String>())),
        ("nodes", "pub struct Record{pub value:[[(u8,u8);128];32]} #[chialisp_oxide::puzzle] pub fn p(v:Record)->Record{v}".to_string()),
    ] {
        let fixture = Fixture::with_sdk_alias("chialisp_oxide");
        fixture.source(&source);
        assert!(fixture.cargo(false).status.success(), "native {label}");
        let output = fixture.cargo(true);
        assert!(!output.status.success(), "accepted {label}");
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(errors.contains("limit") || errors.contains("256 fields"), "{label}: {errors}");
        assert!(!errors.contains("internal compiler error"), "{label}: {errors}");
        assert!(fixture.exports().is_empty());
    }
}

#[test]
fn wide_deep_updates_are_bounded_before_copying_projection_chains() {
    // Each individual record/type is small and native-valid. Updating the
    // deepest field would nevertheless duplicate too many projection chains.
    let fields = (0..63).map(|i| format!("pub f{i}:u8,")).collect::<String>();
    let mut source = format!("pub struct R0{{{fields}pub value:u8}}");
    for depth in 1..20 {
        source.push_str(&format!(
            "pub struct R{depth}{{{fields}pub next:R{}}}",
            depth - 1
        ));
    }
    source.push_str(&format!(
        "#[chialisp_oxide::puzzle] pub fn p(mut value:R19)->R19{{value.{}value=1;value}}",
        "next.".repeat(19)
    ));
    let fixture = Fixture::with_sdk_alias("chialisp_oxide");
    fixture.source(&source);
    assert!(fixture.cargo(false).status.success());
    let output = fixture.cargo(true);
    assert!(!output.status.success());
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(
        errors.contains("named struct update expression expansion limit exceeded"),
        "{errors}"
    );
    assert!(!errors.contains("internal compiler error"), "{errors}");
    assert!(fixture.exports().is_empty());
}
