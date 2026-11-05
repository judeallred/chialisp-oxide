//! Existing array constructors retain their ABI; literal indexing uses closed
//! rustc metadata. Actual Cargo/native source and independent Chialisp below.
mod support;
#[path = "support/vm.rs"]
mod vm;
use chialisp_oxide_backend_experiment::Outcome;
use std::{path::Path, process::Command};
use support::Fixture;
const SOURCE: &str = r#"
use chialisp_oxide::{Tree,raw_clvm as raw};
#[derive(Copy,Clone)] pub struct Payment {pub opcode:u8,pub hash:[u8;2],pub amount:u64}
#[chialisp_oxide::puzzle] pub fn sum(values:[u64;2])->u64{values[0]+values[1]}
#[chialisp_oxide::puzzle] pub fn last(values:[u64;8])->u64{values[7]}
#[chialisp_oxide::puzzle] pub fn budgeted(values:[u64;64])->u64{values[63]}
#[chialisp_oxide::puzzle] pub fn nested(values:[[u16;2];2])->u16{values[1][0]}
#[chialisp_oxide::puzzle] pub fn selected(values:[bool;3])->bool{values[2]}
#[chialisp_oxide::puzzle] pub fn byte_first(values:[u8;1])->u8{values[0]}
#[chialisp_oxide::puzzle] pub fn byte_reference(values:&[u8;1])->u8{values[0]}
#[chialisp_oxide::puzzle] pub fn bytes(values:[u8;4])->(u8,u8,u8,u8){(values[0],values[1],values[2],values[3])}
#[chialisp_oxide::puzzle] pub fn byte_record(value:Payment)->u8{value.hash[1]}
fn byte_value(fail:bool)->[u8;2]{if fail{let _=raw::first(&Tree::Atom(&[]));}[128,255]}
#[chialisp_oxide::puzzle] pub fn unused_byte(fail:bool)->u64{let _byte=byte_value(fail)[1];17}
#[chialisp_oxide::puzzle] pub fn bytes_inside(values:[[u8;2];2])->[u8;2]{values[1]}
#[chialisp_oxide::puzzle] pub fn payments(amount:u64)->[Payment;2]{[Payment{opcode:51,hash:[0,255],amount},Payment{opcode:51,hash:[128,1],amount:7}]}
#[chialisp_oxide::puzzle] pub fn payment_amount(values:[Payment;2])->u64{values[1].amount}
#[chialisp_oxide::puzzle] pub fn repeat(value:u64)->[u64;3]{[value;3]}
fn left(fail:bool)->u64{if fail {let _=raw::first(&Tree::Atom(&[]));}7}
fn right(fail:bool)->u64{if fail {let _=raw::div(&Tree::Atom(&[1]),&Tree::Atom(&[]));}9}
#[chialisp_oxide::puzzle] pub fn repeat_call(fail:bool)->[u64;2]{[left(fail);2]}
fn repeat_helper(value:u64)->[u64;2]{[value;2]}
#[chialisp_oxide::puzzle] pub fn across(first:u64,second:u64)->[u64;2]{repeat_helper(first/second)}
#[chialisp_oxide::puzzle] pub fn repeat_div(first:u64,second:u64)->[u64;2]{[first/second;2]}
#[chialisp_oxide::puzzle] pub fn order(fail:bool)->[u64;2]{[left(fail),right(true)]}
#[chialisp_oxide::puzzle] pub fn snapshot(fail:bool)->([u64;2],u64){let mut value=left(fail);let values=[value;2];value=11;(values,value)}
fn assert_byte(fail:bool)->u8{assert!(!fail);7}
#[chialisp_oxide::puzzle] pub fn zero_byte(fail:bool)->[u8;0]{[assert_byte(fail);0]}
#[chialisp_oxide::puzzle] pub fn zero_div_byte(first:u8,second:u8)->[u8;0]{[first/second;0]}
#[chialisp_oxide::puzzle] pub fn zero_div(first:u64,second:u64)->[u64;0]{[first/second;0]}
fn spin(value:u8)->u8{spin(value)}
#[chialisp_oxide::puzzle] pub fn lazy_byte(run:bool)->[u8;0]{if run {[spin(0);0]}else{[]}}
#[chialisp_oxide::puzzle] pub fn ordered_byte(fail:bool)->[u8;0]{let _first=[assert_byte(fail);0];let _second=[spin(0);0];[]}
#[chialisp_oxide::puzzle] pub fn discarded(fail:bool)->u64{let _array=[left(fail);2];17}
#[chialisp_oxide::puzzle] pub fn reassign(first:u64,second:u64)->([u64;2],[u64;2]){let mut values=[first,second];let old=values;values=[second,first];(old,values)}
#[test]fn native_cases(){
assert_eq!(sum([7,9]),16);assert_eq!(last(std::array::from_fn(|i|i as u64)),7);assert_eq!(budgeted(std::array::from_fn(|i|i as u64)),63);assert_eq!(nested([[1,2],[3,4]]),3);assert!(selected([false,false,true]));
for value in 0u16..=255{assert_eq!(byte_first([value as u8]),value as u8);assert_eq!(byte_reference(&[value as u8]),value as u8);}
assert_eq!(bytes([0,127,128,255]),(0,127,128,255));assert_eq!(byte_record(Payment{opcode:51,hash:[0,255],amount:7}),255);assert_eq!(unused_byte(false),17);let error=std::panic::catch_unwind(||unused_byte(true)).expect_err("discarded byte initializer");assert!(!error.is::<chialisp_oxide::ResourceExhausted>());
assert_eq!(bytes_inside([[0,255],[128,1]]),[128,1]);let p=payments(12);assert_eq!((p[0].opcode,p[0].hash,p[0].amount),(51,[0,255],12));assert_eq!(payment_amount(p),7);
assert_eq!(repeat(5),[5;3]);assert_eq!(repeat_call(false),[7;2]);assert_eq!(repeat_div(21,3),[7;2]);assert_eq!(across(21,3),[7;2]);assert_eq!(snapshot(false),([7;2],11));
assert!(zero_byte(false).is_empty());assert!(zero_div_byte(21,3).is_empty());assert!(zero_div(21,3).is_empty());assert!(lazy_byte(false).is_empty());assert_eq!(discarded(false),17);assert_eq!(reassign(7,9),([7,9],[9,7]));
for f in [zero_byte as fn(bool)->[u8;0],ordered_byte]{let e=std::panic::catch_unwind(||f(true)).expect_err("strict zero-length initializer");assert!(!e.is::<chialisp_oxide::ResourceExhausted>());}
assert!(std::panic::catch_unwind(||zero_div_byte(1,0)).is_err());assert!(std::panic::catch_unwind(||zero_div(1,0)).is_err());
for fail in [false,true]{let e=std::panic::catch_unwind(||order(fail)).expect_err("ordered failing initializers");assert!(!e.is::<chialisp_oxide::ResourceExhausted>());}
}
"#;
fn actual_native(f: &Fixture) {
    let manifest = f.0.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    let dep = text
        .lines()
        .find(|line| line.starts_with("chialisp_oxide="))
        .unwrap()
        .replace('}', ",features=[\"vm\"]}");
    std::fs::write(&manifest, format!("{text}[dev-dependencies]\n{dep}\n")).unwrap();
    let out = Command::new(Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/cargo"))
        .current_dir(&f.0)
        .env("RUSTC", f.compiler())
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CHIALISP_OXIDE_CARGO_MODE")
        .args(["test", "--lib", "--offline", "--target-dir"])
        .arg(f.0.join("native-build"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn count(value: &serde_json::Value, key: &str, expected: &str) -> usize {
    match value {
        serde_json::Value::Object(fields) => {
            usize::from(fields.get(key).and_then(|v| v.as_str()) == Some(expected))
                + fields
                    .values()
                    .map(|v| count(v, key, expected))
                    .sum::<usize>()
        }
        serde_json::Value::Array(values) => values.iter().map(|v| count(v, key, expected)).sum(),
        _ => 0,
    }
}
#[test]
fn actual_fixed_arrays_preserve_native_outputs_strictness_and_byte_abi() {
    let f = Fixture::with_sdk_alias("chialisp_oxide");
    f.source(SOURCE);
    actual_native(&f);
    let out = f.cargo(true);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let programs = f.exports();
    assert_eq!(programs.len(), 26);
    let cases = [
        ("sum", "((7 9))", "16"),
        ("nested", "(((1 2) (3 4)))", "3"),
        ("selected", "((() () 1))", "1"),
        ("bytes_inside", "((0x00ff 0x8001))", "0x8001"),
        ("bytes", "(0x007f80ff)", "(0 127 128 255)"),
        ("byte_record", "((51 0x00ff 7))", "255"),
        ("unused_byte", "(())", "17"),
        ("payments", "(12)", "((51 0x00ff 12) (51 0x8001 7))"),
        ("payment_amount", "(((51 0x00ff 12) (51 0x8001 7)))", "7"),
        ("repeat", "(5)", "(5 5 5)"),
        ("repeat_call", "(())", "(7 7)"),
        ("repeat_div", "(21 3)", "(7 7)"),
        ("across", "(21 3)", "(7 7)"),
        ("snapshot", "(())", "((7 7) 11)"),
        ("zero_byte", "(())", "()"),
        ("zero_div_byte", "(21 3)", "()"),
        ("zero_div", "(21 3)", "()"),
        ("lazy_byte", "(())", "()"),
        ("discarded", "(())", "17"),
        ("reassign", "(7 9)", "((7 9) (9 7))"),
    ];
    for (name, environment, expected) in cases {
        let program = programs
            .iter()
            .find(|p| p.name.ends_with(&format!("::{name}")))
            .unwrap();
        let reference =
            chialisp_oxide_reference::compile(&format!("(mod ENV (q . {expected}))")).unwrap();
        for optimize in [false, true] {
            let bytes = chialisp_oxide_ir::compile(program, optimize).unwrap();
            assert_eq!(
                vm::execute(&bytes, environment, 20_000_000)
                    .unwrap()
                    .outcome,
                vm::execute(&reference, environment, 20_000_000)
                    .unwrap()
                    .outcome,
                "{name}/{optimize}"
            );
        }
    }
    for entry in ["byte_first", "byte_reference"] {
        let first = programs
            .iter()
            .find(|p| p.name.ends_with(&format!("::{entry}")))
            .unwrap();
        let reference =
            chialisp_oxide_reference::compile("(mod (BYTES) (logand 255 (substr BYTES 0 1)))")
                .unwrap();
        for optimize in [false, true] {
            let bytes = chialisp_oxide_ir::compile(first, optimize).unwrap();
            for value in 0u16..=255 {
                let input = format!("(0x{value:02x})");
                assert_eq!(
                    vm::execute(&bytes, &input, 20_000_000).unwrap().outcome,
                    vm::execute(&reference, &input, 20_000_000).unwrap().outcome,
                    "byte {value}/{optimize}"
                );
            }
            for input in ["()", "(())", "(0x0001)", "((1))", "(0x80 1)"] {
                assert_eq!(
                    vm::execute(&bytes, input, 20_000_000).unwrap().outcome,
                    Outcome::Rejected,
                    "byte ABI {input}"
                );
            }
        }
    }
    let four = programs
        .iter()
        .find(|p| p.name.ends_with("::bytes"))
        .unwrap();
    for optimize in [false, true] {
        let bytes = chialisp_oxide_ir::compile(four, optimize).unwrap();
        for input in ["(0x007f80)", "(0x007f80ff00)", "((0 127 128 255))"] {
            assert_eq!(
                vm::execute(&bytes, input, 20_000_000).unwrap().outcome,
                Outcome::Rejected
            );
        }
    }
    let last = programs
        .iter()
        .find(|p| p.name.ends_with("::last"))
        .unwrap();
    let environment = format!(
        "(({}))",
        (0..8).map(|i| i.to_string()).collect::<Vec<_>>().join(" ")
    );
    let reference = chialisp_oxide_reference::compile("(mod ENV (q . 7))").unwrap();
    for optimize in [false, true] {
        let bytes = chialisp_oxide_ir::compile(last, optimize).unwrap();
        assert_eq!(
            vm::execute(&bytes, &environment, 20_000_000)
                .unwrap()
                .outcome,
            vm::execute(&reference, &environment, 20_000_000)
                .unwrap()
                .outcome
        );
    }
    // Source/IR admission is not a promise that every typed environment fits
    // existing backend budgets. Keep this a compiler-resource control, never
    // VM rejection evidence; no compiler limit is raised for array authoring.
    let budgeted = programs
        .iter()
        .find(|p| p.name.ends_with("::budgeted"))
        .unwrap();
    for optimize in [false, true] {
        assert!(
            chialisp_oxide_ir::compile(budgeted, optimize)
                .unwrap_err()
                .to_string()
                .contains("CLVM expansion nesting limit")
        );
    }
    // These successful, total historical routes need no new runtime table.
    for name in ["repeat", "zero_div_byte", "zero_byte"] {
        assert!(
            programs
                .iter()
                .find(|p| p.name.ends_with(&format!("::{name}")))
                .unwrap()
                .functions
                .is_empty(),
            "unneeded array transition: {name}"
        );
    }
    for name in ["repeat_call", "repeat_div", "across"] {
        let p = programs
            .iter()
            .find(|p| p.name.ends_with(&format!("::{name}")))
            .unwrap();
        let function = p
            .functions
            .iter()
            .find(|f| f.name.ends_with(&format!("::{name}")))
            .expect("computed array initializer has strict bindings");
        let body = serde_json::to_value(&function.body).unwrap();
        assert_eq!(
            count(
                &body,
                if name == "repeat_call" { "op" } else { "kind" },
                if name == "repeat_call" { "call" } else { "div" }
            ),
            1,
            "initializer duplicated: {name}"
        );
    }
    for (name, input, code) in [
        ("repeat_call", "(1)", "InvalidOpArg"),
        ("unused_byte", "(1)", "InvalidOpArg"),
        ("order", "(1)", "InvalidOpArg"),
        ("order", "(())", "DivisionByZero"),
        ("zero_byte", "(1)", "Raise"),
        ("zero_div_byte", "(1 0)", "Raise"),
        ("zero_div", "(1 0)", "Raise"),
        ("ordered_byte", "(1)", "Raise"),
        ("discarded", "(1)", "InvalidOpArg"),
    ] {
        let p = programs
            .iter()
            .find(|p| p.name.ends_with(&format!("::{name}")))
            .unwrap();
        for optimize in [false, true] {
            let bytes = chialisp_oxide_ir::compile(p, optimize).unwrap();
            let result = vm::execute(&bytes, input, 20_000_000).unwrap();
            assert_eq!(result.outcome, Outcome::Rejected, "{name}");
            assert_eq!(
                format!("{:?}", result.failure.unwrap().code()),
                code,
                "{name}/{optimize}"
            );
        }
    }
    for name in ["lazy_byte", "ordered_byte"] {
        let p = programs
            .iter()
            .find(|p| p.name.ends_with(&format!("::{name}")))
            .unwrap();
        let input = if name == "lazy_byte" { "(1)" } else { "(())" };
        for optimize in [false, true] {
            let bytes = chialisp_oxide_ir::compile(p, optimize).unwrap();
            let result = chialisp_oxide_backend_experiment::execute(&bytes, input, 10_000).unwrap();
            assert_eq!(
                result.outcome,
                Outcome::Inconclusive,
                "divergent initializer removed: {name}"
            );
            assert_eq!(
                format!("{:?}", result.failure.unwrap().code()),
                "CostExceeded"
            );
        }
    }
    for name in ["sum", "nested", "selected", "payment_amount"] {
        let p = programs
            .iter()
            .find(|p| p.name.ends_with(&format!("::{name}")))
            .unwrap();
        for optimize in [false, true] {
            let bytes = chialisp_oxide_ir::compile(p, optimize).unwrap();
            for input in [
                "()",
                "(1)",
                "((1))",
                "((1 2 3 4))",
                "((1 . 2))",
                "((0x0001 2))",
            ] {
                assert_eq!(
                    vm::execute(&bytes, input, 20_000_000).unwrap().outcome,
                    Outcome::Rejected,
                    "{name}/{input}"
                );
            }
        }
    }
}
#[test]
fn dynamic_mutating_reference_byte_and_out_of_bounds_indexing_remain_closed() {
    for (name, source) in [
        (
            "dynamic",
            "#[chialisp_oxide::puzzle]pub fn entry(a:[u64;2],i:usize)->u64{a[i]}",
        ),
        (
            "cast",
            "#[chialisp_oxide::puzzle]pub fn entry(a:[u64;2],i:u8)->u64{a[i as usize]}",
        ),
        (
            "scratch",
            "#[chialisp_oxide::puzzle]pub fn entry(a:[u64;2])->u64{let mut i=0;i=1;a[i]}",
        ),
        (
            "forward",
            "#[chialisp_oxide::puzzle]pub fn entry(a:[u64;2])->u64{let i=1;a[i]}",
        ),
        (
            "mutation",
            "#[chialisp_oxide::puzzle]pub fn entry(mut a:[u64;2])->[u64;2]{a[0]=7;a}",
        ),
        (
            "borrow",
            "#[chialisp_oxide::puzzle]pub fn entry(a:[u64;2])->u64{let value=&a[0];*value}",
        ),
        (
            "bytes",
            "#[chialisp_oxide::puzzle]pub fn entry(a:[u8;2])->u8{let index=1;a[index]}",
        ),
        (
            "dynamic-slice",
            "#[chialisp_oxide::puzzle]pub fn entry(a:&[u8])->u8{a[0]}",
        ),
        (
            "mutable-byte-reference",
            "#[chialisp_oxide::puzzle]pub fn entry(a:&mut[u8;2])->u8{a[0]}",
        ),
        (
            "byte-element-reference",
            "#[chialisp_oxide::puzzle]pub fn entry(a:[u8;2])->u8{let byte=&a[0];*byte}",
        ),
        (
            "out-of-bounds",
            "#![allow(unconditional_panic)]\n#[chialisp_oxide::puzzle]pub fn entry(a:[u64;2])->u64{a[2]}",
        ),
        (
            "tree",
            "use chialisp_oxide::Tree;#[chialisp_oxide::puzzle]pub fn entry(a:[Tree<'static>;2])->Tree<'static>{a[0]}",
        ),
        (
            "optional",
            "#[chialisp_oxide::puzzle]pub fn entry()->Option<u64>{let a=[Some(7),None];a[0]}",
        ),
    ] {
        let f = Fixture::with_sdk_alias("chialisp_oxide");
        f.source(source);
        let native = f.cargo(false);
        assert!(
            native.status.success(),
            "native {name}: {}",
            String::from_utf8_lossy(&native.stderr)
        );
        let result = f.cargo(true);
        assert!(!result.status.success(), "accepted {name}");
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(
            !error.contains("internal compiler error"),
            "{name}: {error}"
        );
        assert!(f.exports().is_empty(), "partial export {name}");
    }
}
