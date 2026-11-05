//! Local records compose with the existing v5 pair/v6 output-only Option ABI.
//! Actual Cargo/native functions and independent finite Chialisp policies;
//! no optional input, consensus, or compiler-preservation claim.
mod support;
#[path = "support/vm.rs"]
mod vm;
use chialisp_oxide_backend_experiment::Outcome;
use std::process::Command;
use support::Fixture;

const SOURCE: &str = r#"
use chialisp_oxide::Cons;
pub struct Record { pub small: u8, pub wide: u64, pub flag: bool }
pub struct Zero { pub value: () }
mod local {
    pub struct Cons { pub rest: u16, pub first: u8 }
}

#[chialisp_oxide::puzzle]
pub fn record_pair(value: Cons<Record, u16>) -> Cons<Record, u16> { value }
#[chialisp_oxide::puzzle]
pub fn pair_fields(value: Cons<Record, u16>) -> (u8, u64, bool, u16) {
    (value.first.small, value.first.wide, value.first.flag, value.rest)
}
#[chialisp_oxide::puzzle]
pub fn ignored_pair(value: Cons<Record, u16>) -> u8 { let _ = value; 7 }
#[chialisp_oxide::puzzle]
pub fn maybe_record(choose: bool, n: u8) -> Option<Record> {
    if choose { Some(Record { flag: true, wide: 513, small: n }) } else { None }
}
#[chialisp_oxide::puzzle]
pub fn optional_tail(choose: bool, n: u8) -> Cons<u8, Option<Record>> {
    Cons { first: 5, rest: if choose {
        Some(Record { small: n, wide: 513, flag: true })
    } else { None } }
}
#[chialisp_oxide::puzzle]
pub fn optional_pair(choose: bool, n: u8) -> Option<Cons<Record, u16>> {
    if choose { Some(Cons { first: Record { small: n, wide: 513, flag: true }, rest: 9 }) }
    else { None }
}
#[chialisp_oxide::puzzle]
pub fn unused_some(divisor: u8) -> u8 {
    let _unused = Some(Record { small: 9 / divisor, wide: 513, flag: true });
    7
}
#[chialisp_oxide::puzzle]
pub fn source_order(n: u8) -> Option<Record> {
    let mut cursor = n;
    Some(Record {
        wide: { cursor = cursor + 1; 513 },
        flag: { cursor = cursor + 1; true },
        small: cursor,
    })
}
#[chialisp_oxide::puzzle]
pub fn snapshots(mut record: Record) -> (Option<Record>, Cons<Record, u16>) {
    let captured = Some(Record {
        small: record.small, wide: record.wide, flag: record.flag,
    });
    record.small = record.small + 1;
    (captured, Cons { first: record, rest: 9 })
}
#[chialisp_oxide::puzzle]
pub fn local_cons(value: Cons<local::Cons, u8>) -> Cons<local::Cons, u8> { value }
#[chialisp_oxide::puzzle]
pub fn maybe_zero(choose: bool) -> Option<Zero> {
    if choose { Some(Zero { value: () }) } else { None }
}

#[cfg(test)] mod tests {
    use super::*;
    fn record() -> Record { Record { small: 7, wide: 513, flag: true } }
    fn fields(value: Record) -> (u8,u64,bool) { (value.small,value.wide,value.flag) }
    fn failure<T>(run: impl FnOnce()->T + std::panic::UnwindSafe) {
        let error=std::panic::catch_unwind(run).err().expect("policy must fail");
        assert!(!error.is::<chialisp_oxide::ResourceExhausted>());
        assert!(error.is::<String>() || error.is::<&str>());
    }
    #[test] fn actual_native_compositions() {
        let value=record_pair(Cons{first:record(),rest:9});
        assert_eq!((fields(value.first),value.rest),((7,513,true),9));
        assert_eq!(pair_fields(Cons{first:record(),rest:9}),(7,513,true,9));
        assert_eq!(ignored_pair(Cons{first:record(),rest:9}),7);
        assert_eq!(fields(maybe_record(true,7).unwrap()),(7,513,true));
        assert!(maybe_record(false,255).is_none());
        let value=optional_tail(true,7);
        assert_eq!(value.first,5);assert_eq!(fields(value.rest.unwrap()),(7,513,true));
        let value=optional_tail(false,255);assert_eq!(value.first,5);assert!(value.rest.is_none());
        let value=optional_pair(true,7).unwrap();
        assert_eq!((fields(value.first),value.rest),((7,513,true),9));
        assert!(optional_pair(false,255).is_none());
        assert_eq!(unused_some(3),7);failure(||unused_some(0));
        for (input,small) in [(0,2),(7,9),(253,255)] {
            assert_eq!(fields(source_order(input).unwrap()),(small,513,true));
        }
        failure(||source_order(254));failure(||source_order(255));
        let (old,new)=snapshots(record());
        assert_eq!(fields(old.unwrap()),(7,513,true));
        assert_eq!((fields(new.first),new.rest),((8,513,true),9));
        failure(||snapshots(Record{small:255,wide:513,flag:true}));
        let value=local_cons(Cons{first:local::Cons{rest:513,first:7},rest:9});
        assert_eq!((value.first.rest,value.first.first,value.rest),(513,7,9));
        assert_eq!(maybe_zero(true).unwrap().value,());assert!(maybe_zero(false).is_none());
    }
}
"#;

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
    // These independently written policies are compared on the selected valid
    // typed inputs. Rust's complete malformed-ABI checks are a separate lane.
    match name {
        "record_pair" | "local_cons" => "(mod (V) V)",
        "pair_fields" => "(mod (V) (list (f (f V)) (f (r (f V))) (f (r (r (f V)))) (r V)))",
        "ignored_pair" => "(mod (V) 7)",
        "maybe_record" => "(mod (C N) (if C (list (list N 513 1)) ()))",
        "optional_tail" => "(mod (C N) (if C (list 5 (list N 513 1)) (list 5)))",
        "optional_pair" => "(mod (C N) (if C (list (c (list N 513 1) 9)) ()))",
        "unused_some" => "(mod (D) (if D 7 (x)))",
        "source_order" => "(mod (N) (if (> N 253) (x) (list (list (+ N 2) 513 1))))",
        "snapshots" => {
            "(mod (R) (if (> (f R) 254) (x) (list (list R) (c (list (+ (f R) 1) (f (r R)) (f (r (r R)))) 9))))"
        }
        "maybe_zero" => "(mod (C) (if C (list (list ())) ()))",
        _ => panic!("unknown export {name}"),
    }
}

fn cases(name: &str) -> &'static [(&'static str, &'static str)] {
    match name {
        "record_pair" => &[("(((7 513 1) . 9))", "((7 513 1) . 9)")],
        "pair_fields" => &[("(((7 513 1) . 9))", "(7 513 1 9)")],
        "ignored_pair" => &[("(((7 513 1) . 9))", "7")],
        "maybe_record" => &[("(1 7)", "((7 513 1))"), ("(() 255)", "()")],
        "optional_tail" => &[("(1 7)", "(5 (7 513 1))"), ("(() 255)", "(5)")],
        "optional_pair" => &[("(1 7)", "(((7 513 1) . 9))"), ("(() 255)", "()")],
        "unused_some" => &[("(3)", "7"), ("(255)", "7")],
        "source_order" => &[
            ("(0)", "((2 513 1))"),
            ("(7)", "((9 513 1))"),
            ("(253)", "((255 513 1))"),
        ],
        "snapshots" => &[("((7 513 1))", "(((7 513 1)) ((8 513 1) . 9))")],
        "local_cons" => &[("(((513 7) . 9))", "((513 7) . 9)")],
        "maybe_zero" => &[("(1)", "((()))"), ("(())", "()")],
        _ => panic!("unknown export {name}"),
    }
}

#[test]
fn actual_records_compose_with_pairs_and_optional_outputs_in_both_modes() {
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
    assert_eq!(programs.len(), 11);
    for program in programs {
        let name = program.name.rsplit("::").next().unwrap();
        program.validate().unwrap();
        let version =
            if ["record_pair", "pair_fields", "ignored_pair", "local_cons"].contains(&name) {
                5
            } else {
                6
            };
        assert_eq!(
            serde_json::to_value(&program).unwrap()["ir_version"],
            version,
            "{name}"
        );
        let reference = chialisp_oxide_reference::compile(reference(name)).unwrap();
        for optimize in [false, true] {
            let code = chialisp_oxide_ir::compile(&program, optimize).unwrap();
            for (solution, expected) in cases(name) {
                let quoted =
                    chialisp_oxide_reference::compile(&format!("(mod () (q . {expected}))"))
                        .unwrap();
                let expected = vm::execute(&quoted, "()", 20_000_000).unwrap().outcome;
                let independent = vm::execute(&reference, solution, 20_000_000)
                    .unwrap()
                    .outcome;
                assert_eq!(independent, expected, "reference {name}/{solution}");
                assert_eq!(
                    vm::execute(&code, solution, 20_000_000).unwrap().outcome,
                    independent,
                    "{name}/{optimize}/{solution}"
                );
            }
            let failures: &[&str] = match name {
                "unused_some" => &["(0)"],
                "source_order" => &["(254)", "(255)"],
                "snapshots" => &["((255 513 1))"],
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
                    vm::execute(&reference, solution, 20_000_000)
                        .unwrap()
                        .outcome,
                    Outcome::Rejected
                );
            }
            if ["record_pair", "pair_fields", "ignored_pair"].contains(&name) {
                for malformed in [
                    "()",
                    "(((7 513 1) . 9) ())",
                    "(((7 513 1) . 9) . 2)",
                    "((7 . 9))",
                    "(((7 513) . 9))",
                    "(((7 513 1 2) . 9))",
                    "(((7 513 . 1) . 9))",
                    "(((0x0007 513 1) . 9))",
                    "(((256 513 1) . 9))",
                    "(((7 -1 1) . 9))",
                    "(((7 18446744073709551616 1) . 9))",
                    "(((7 513 2) . 9))",
                    "(((7 513 0x0001) . 9))",
                    "(((7 513 1) . -1))",
                    "(((7 513 1) . 65536))",
                    "(((7 513 1) . 0x0009))",
                ] {
                    assert_eq!(
                        vm::execute(&code, malformed, 20_000_000).unwrap().outcome,
                        Outcome::Rejected,
                        "{name}/{optimize}/{malformed}"
                    );
                }
            }
            if ["maybe_record", "optional_tail", "optional_pair"].contains(&name) {
                // None branches still validate every typed input before body execution.
                for malformed in ["(() 256)", "(() 0x0007)", "(2 7)", "(() 7 9)", "(() . 7)"] {
                    assert_eq!(
                        vm::execute(&code, malformed, 20_000_000).unwrap().outcome,
                        Outcome::Rejected,
                        "{name}/{optimize}/{malformed}"
                    );
                }
            }
        }
    }
}

#[test]
fn composed_records_do_not_hide_forbidden_fields() {
    // Reuse dependency builds; each distinct source is checked natively before
    // extraction, and no successful export exists in this negative-only fixture.
    let fixture = Fixture::with_sdk_alias("chialisp_oxide");
    for fields in [
        "pub struct Bad {pub value:&'static u8}",
        "pub struct Bad {pub value:[&'static u8;0]}",
        "pub struct Bad {pub value:u8} impl Drop for Bad {fn drop(&mut self){}}",
        "pub struct Bad {pub value:Option<u8>}",
        "pub struct Bad {pub value:chialisp_oxide::Cons<u8,u8>}",
        "pub struct Bad {pub value:std::boxed::Box<u8>}",
    ] {
        for export in [
            "#[chialisp_oxide::puzzle]pub fn p()->Option<Bad>{None}",
            "#[chialisp_oxide::puzzle]pub fn p(_:chialisp_oxide::Cons<Bad,u8>)->u8{7}",
        ] {
            fixture.source(&format!("{fields}\n{export}"));
            let native = fixture.cargo(false);
            assert!(
                native.status.success(),
                "native {}",
                String::from_utf8_lossy(&native.stderr)
            );
            let output = fixture.cargo(true);
            assert!(!output.status.success(), "accepted {fields}: {export}");
            assert!(fixture.exports().is_empty());
            assert!(!String::from_utf8_lossy(&output.stderr).contains("internal compiler error"));
        }
    }
}
