//! Actual Cargo source, actual native outputs, and both compiled modes.
mod support;
#[path = "support/vm.rs"]
mod vm;
use chialisp_oxide_backend_experiment::Outcome;
use chialisp_oxide_ir::Type;
use num_bigint::BigInt;
use std::{collections::BTreeMap, path::Path, process::Command};
use support::Fixture;
struct Case {
    name: String,
    from: &'static str,
    to: &'static str,
    source: Type,
    target: Type,
    values: Vec<String>,
}
fn integer(bits: u16, signed: bool) -> Type {
    Type::Int { bits, signed }
}
#[allow(
    clippy::unnecessary_cast,
    reason = "Source-case generation deliberately instantiates all ten primitive native integer types"
)]
fn cases() -> Vec<Case> {
    let mut rows = vec![];
    macro_rules! source {
        ($from:ty,$bits:literal,$signed:literal) => {{
            for (to, tb, ts) in [
                ("u8", 8, false),
                ("i8", 8, true),
                ("u16", 16, false),
                ("i16", 16, true),
                ("u32", 32, false),
                ("i32", 32, true),
                ("u64", 64, false),
                ("i64", 64, true),
                ("u128", 128, false),
                ("i128", 128, true),
            ] {
                let mut values: Vec<$from> = if $bits == 8 {
                    (<$from>::MIN as i16..=<$from>::MAX as i16)
                        .map(|n| n as $from)
                        .collect()
                } else {
                    vec![
                        <$from>::MIN,
                        <$from>::MIN + 1,
                        <$from>::MIN / 2,
                        0,
                        1,
                        2,
                        <$from>::MAX / 2,
                        <$from>::MAX - 1,
                        <$from>::MAX,
                        !0,
                        (-2i16) as $from,
                    ]
                };
                for width in [usize::from(tb - 1), usize::from(tb)] {
                    let boundary = BigInt::from(1u8) << width;
                    for sign in [-1i8, 1] {
                        for delta in [-1i8, 0, 1] {
                            let candidate: BigInt =
                                &boundary * BigInt::from(sign) + BigInt::from(delta);
                            if let Ok(value) = candidate.to_string().parse::<$from>() {
                                values.push(value);
                            }
                        }
                    }
                }
                let mut values = values
                    .into_iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>();
                values.sort();
                values.dedup();
                rows.push(Case {
                    name: format!("cast_{}_to_{to}", stringify!($from)),
                    from: stringify!($from),
                    to,
                    source: integer($bits, $signed),
                    target: integer(tb, ts),
                    values,
                });
            }
        }};
    }
    source!(u8, 8, false);
    source!(i8, 8, true);
    source!(u16, 16, false);
    source!(i16, 16, true);
    source!(u32, 32, false);
    source!(i32, 32, true);
    source!(u64, 64, false);
    source!(i64, 64, true);
    source!(u128, 128, false);
    source!(i128, 128, true);
    rows
}
fn library(rows: &[Case]) -> String {
    let mut text = String::from("use chialisp_oxide::{Tree,raw_clvm as raw};\n");
    for row in rows {
        text.push_str(&format!(
            "#[chialisp_oxide::puzzle]pub fn {}(value:{})->{}{{value as {}}}\n",
            row.name, row.from, row.to, row.to
        ));
    }
    text.push_str(r#"
fn quotient(a:i16,b:i16)->i16{a/b}
fn reject_zero(value:i16)->i16{assert!(value!=0);value}
#[chialisp_oxide::puzzle]pub fn unused(a:i16,b:i16)->u8{let _cast=quotient(a,b)as u8;7}
#[chialisp_oxide::puzzle]pub fn lazy(a:i16,b:i16,choose:bool)->u8{if choose{quotient(a,b)as u8}else{7}}
#[chialisp_oxide::puzzle]pub fn cast_before_raw(value:i16)->Tree<'static>{let _cast=reject_zero(value)as u8;raw::div(&Tree::Atom(&[1]),&Tree::Atom(&[]))}
#[chialisp_oxide::puzzle]pub fn raw_before_cast(value:i16)->u8{let _raw=raw::div(&Tree::Atom(&[1]),&Tree::Atom(&[]));reject_zero(value)as u8}
#[chialisp_oxide::puzzle]pub fn convert(value:i16)->(i8,u8,i32,u32){(value as i8,value as u8,value as i32,value as u32)}
#[chialisp_oxide::puzzle]pub fn snapshot(value:i16)->(i8,i16){let before=value as i8;let value=7;(before,value)}
#[cfg(test)]mod native{use super::*;#[test]fn observations(){
"#);
    for row in rows {
        text.push_str(&format!(
            "for value in [{}] {{println!(\"CAST|{}|{{}}|{{}}\",value,{}(value));}}\n",
            row.values
                .iter()
                .map(|v| format!("{v}_{}", row.from))
                .collect::<Vec<_>>()
                .join(","),
            row.name,
            row.name
        ));
    }
    text.push_str(r#"
for value in [-1,128,256]{let out=convert(value);println!("TUPLE|{value}|{}|{}|{}|{}",out.0,out.1,out.2,out.3);}
assert_eq!(snapshot(128),(-128,7));
assert!(std::panic::catch_unwind(||unused(1,0)).is_err());
assert!(std::panic::catch_unwind(||unused(i16::MIN,-1)).is_err());
assert_eq!(lazy(1,0,false),7);
for value in [0,1]{for f in [cast_before_raw as fn(i16)->Tree<'static>]{let e=std::panic::catch_unwind(||f(value)).expect_err("strict failing initializer");assert!(!e.is::<chialisp_oxide::ResourceExhausted>());}}
let e=std::panic::catch_unwind(||raw_before_cast(0)).expect_err("raw failure");assert!(!e.is::<chialisp_oxide::ResourceExhausted>());
}}
"#);
    text
}
fn wire(value: &str) -> Vec<u8> {
    let n: BigInt = value.parse().unwrap();
    let bytes = if n == BigInt::from(0) {
        vec![]
    } else {
        n.to_signed_bytes_be()
    };
    match bytes.as_slice() {
        [] => vec![0x80],
        [b] if *b < 128 => vec![*b],
        _ => {
            let mut output = vec![0x80 | bytes.len() as u8];
            output.extend(bytes);
            output
        }
    }
}
fn wire_list(values: &[&str]) -> String {
    let mut bytes = vec![0x80];
    for value in values.iter().rev() {
        let mut next = vec![0xff];
        next.extend(wire(value));
        next.extend(bytes);
        bytes = next;
    }
    hex(&bytes)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|n| format!("{n:02x}")).collect()
}
fn reference(target: &Type) -> Vec<u8> {
    let Type::Int { bits, signed } = target else {
        panic!()
    };
    let modulus = BigInt::from(1u8) << usize::from(*bits);
    let source = if *signed {
        format!(
            "(mod(X)(defun interpret(N)(if(> N {})(- N {modulus})N))(interpret(r(divmod X {modulus}))))",
            (&modulus >> 1usize) - 1
        )
    } else {
        format!("(mod(X)(r(divmod X {modulus})))")
    };
    chialisp_oxide_reference::compile(&source).unwrap()
}
#[test]
fn actual_cargo_casts_match_actual_native_observations_and_reference_in_both_modes() {
    let rows = cases();
    let fixture = Fixture::with_sdk_alias("chialisp_oxide");
    fixture.source(&library(&rows));
    let manifest = fixture.0.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    let dep = text
        .lines()
        .find(|l| l.starts_with("chialisp_oxide="))
        .unwrap()
        .replace('}', ",features=[\"vm\"]}");
    std::fs::write(&manifest, format!("{text}[dev-dependencies]\n{dep}\n")).unwrap();
    let native = Command::new(Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/cargo"))
        .current_dir(&fixture.0)
        .env("RUSTC", fixture.compiler())
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CHIALISP_OXIDE_CARGO_MODE")
        .args(["test", "--lib", "--offline", "--target-dir"])
        .arg(fixture.0.join("native-build"))
        .args(["--", "--nocapture"])
        .output()
        .unwrap();
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let stdout = String::from_utf8(native.stdout).unwrap();
    let mut observations = BTreeMap::new();
    let mut tuples = BTreeMap::new();
    for line in stdout.lines() {
        if let Some(start) = line.find("CAST|") {
            let fields = line[start..].split('|').collect::<Vec<_>>();
            assert_eq!(fields.len(), 4);
            assert!(
                observations
                    .insert(
                        (fields[1].to_owned(), fields[2].to_owned()),
                        fields[3].to_owned()
                    )
                    .is_none()
            );
        }
        if let Some(start) = line.find("TUPLE|") {
            let fields = line[start..].split('|').collect::<Vec<_>>();
            assert_eq!(fields.len(), 6);
            tuples.insert(
                fields[1].to_owned(),
                fields[2..]
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>(),
            );
        }
    }
    assert_eq!(
        observations.len(),
        rows.iter().map(|r| r.values.len()).sum::<usize>()
    );
    assert_eq!(tuples.len(), 3);
    let output = fixture.cargo(true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let programs = fixture.exports();
    assert_eq!(programs.len(), 106);
    for row in &rows {
        let program = programs
            .iter()
            .find(|p| p.name.ends_with(&format!("::{}", row.name)))
            .unwrap();
        if row.source != row.target {
            assert!(program.uses_casts());
            assert!(
                !program.functions.is_empty(),
                "source casts require real strict MIR bindings"
            );
            assert_eq!(serde_json::to_value(program).unwrap()["ir_version"], 7);
        }
        let reference = reference(&row.target);
        for optimize in [false, true] {
            let code = chialisp_oxide_ir::compile(program, optimize).unwrap();
            for value in &row.values {
                let expected = hex(&wire(&observations[&(row.name.clone(), value.clone())]));
                let input = format!("({value})");
                let actual = vm::execute(&code, &input, 100_000_000).unwrap().outcome;
                let canonical = vm::execute(&reference, &input, 100_000_000)
                    .unwrap()
                    .outcome;
                assert_eq!(
                    actual,
                    Outcome::Accepted {
                        output_hex: expected.clone()
                    },
                    "{}({value}) optimized={optimize}",
                    row.name
                );
                assert_eq!(canonical, actual);
            }
        }
    }
    for optimize in [false, true] {
        let code = |name: &str| {
            chialisp_oxide_ir::compile(
                programs
                    .iter()
                    .find(|p| p.name.ends_with(&format!("::{name}")))
                    .unwrap(),
                optimize,
            )
            .unwrap()
        };
        for (value, outputs) in &tuples {
            assert_eq!(
                vm::execute(&code("convert"), &format!("({value})"), 100_000_000)
                    .unwrap()
                    .outcome,
                Outcome::Accepted {
                    output_hex: wire_list(&outputs.iter().map(String::as_str).collect::<Vec<_>>())
                }
            );
        }
        assert_eq!(
            vm::execute(&code("snapshot"), "(128)", 100_000_000)
                .unwrap()
                .outcome,
            Outcome::Accepted {
                output_hex: wire_list(&["-128", "7"])
            }
        );
        for (name, input, failure) in [
            ("unused", "(1 0)", "Raise"),
            ("unused", "(-32768 -1)", "Raise"),
            ("lazy", "(1 0 1)", "Raise"),
            ("cast_before_raw", "(0)", "Raise"),
            ("cast_before_raw", "(1)", "DivisionByZero"),
            ("raw_before_cast", "(0)", "DivisionByZero"),
        ] {
            let result = vm::execute(&code(name), input, 100_000_000).unwrap();
            assert_eq!(result.outcome, Outcome::Rejected);
            assert_eq!(
                format!("{:?}", result.failure.unwrap().code()),
                failure,
                "{name} {input}"
            );
        }
        assert_eq!(
            vm::execute(&code("lazy"), "(1 0 ())", 100_000_000)
                .unwrap()
                .outcome,
            Outcome::Accepted {
                output_hex: "07".into()
            }
        );
    }
}

#[test]
fn native_valid_nonprimitive_casts_remain_outside_the_frontend() {
    for (source, call, expected) in [
        (
            "#[chialisp_oxide::puzzle]pub fn puzzle(value:bool)->u8{value as u8}",
            "puzzle(true)",
            "1",
        ),
        (
            "#[chialisp_oxide::puzzle]pub fn puzzle(value:char)->u8{value as u8}",
            "puzzle('A')",
            "65",
        ),
        (
            "#[chialisp_oxide::puzzle]pub fn puzzle(value:f32)->u8{value as u8}",
            "puzzle(257.0)",
            "255",
        ),
        (
            "#[chialisp_oxide::puzzle]pub fn puzzle(value:u8)->char{value as char}",
            "puzzle(65)",
            "'A'",
        ),
        (
            "#[chialisp_oxide::puzzle]pub fn puzzle(value:usize)->u8{value as u8}",
            "puzzle(257)",
            "1",
        ),
        (
            "#[chialisp_oxide::puzzle]pub fn puzzle(value:isize)->i8{value as i8}",
            "puzzle(-129)",
            "127",
        ),
        (
            "#[chialisp_oxide::puzzle]pub fn puzzle(value:u8)->usize{value as usize}",
            "puzzle(255)",
            "255",
        ),
        (
            "#[chialisp_oxide::puzzle]pub fn puzzle(value:i8)->isize{value as isize}",
            "puzzle(-1)",
            "-1",
        ),
        (
            "enum Flag{A,B} #[chialisp_oxide::puzzle]pub fn puzzle(value:bool)->u8{let flag=if value{Flag::A}else{Flag::B};flag as u8}",
            "puzzle(true)",
            "0",
        ),
    ] {
        let fixture = Fixture::with_sdk_alias("chialisp_oxide");
        fixture.source(&format!("{source}\n#[cfg(test)]mod native{{use super::*;#[test]fn positive(){{assert_eq!({call},{expected});}}}}"));
        let native = Command::new(Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/cargo"))
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
            native.status.success(),
            "native control {source}: {}",
            String::from_utf8_lossy(&native.stderr)
        );
        let output = fixture.cargo(true);
        assert!(
            !output.status.success(),
            "nonprimitive source cast unexpectedly admitted: {source}"
        );
        assert!(fixture.exports().is_empty());
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(
            errors.contains("unsupported")
                || errors.contains("target-dependent")
                || (source.starts_with("enum Flag")
                    && errors.contains("borrowed trees require exactly Atom")),
            "{source}: {errors}"
        );
    }
}
