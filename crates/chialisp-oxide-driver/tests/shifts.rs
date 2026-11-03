//! Ordinary Cargo mixed-type shifts, independently checked against native Rust.
mod support;
#[path = "support/vm.rs"]
mod vm;
use chialisp_oxide_backend_experiment::Outcome;
use std::{path::Path, process::Command};
use support::Fixture;
#[test]
fn actual_cargo_shifts_keep_width_and_signedness() {
    let fixture = Fixture::with_sdk_alias("chialisp_oxide");
    fixture.source(
        r#"
#[chialisp_oxide::puzzle] pub fn left(a:i8,b:u32)->i8{a<<b}
#[chialisp_oxide::puzzle] pub fn right(a:i128,b:i16)->i128{a>>b}
#[chialisp_oxide::puzzle] pub fn narrow_count(a:i128,b:i8)->i128{a>>b}
#[chialisp_oxide::puzzle] pub fn unsigned(a:u128,b:u8)->u128{a>>b}
fn helper(a:i8,b:i16)->i8{a<<b}
#[chialisp_oxide::puzzle] pub fn unused(a:i8,b:i16)->i8{let _value=helper(a,b);7}
#[chialisp_oxide::puzzle] pub fn lazy(a:i8,b:i16,choose:bool)->i8{if choose{helper(a,b)}else{7}}
#[allow(unconditional_recursion)] fn spin()->i8{spin()}
#[chialisp_oxide::puzzle] pub fn guard_before_divergence(a:i8,b:i16)->i8{let checked=a<<b;let _ignored=spin();checked}
#[cfg(test)] mod tests {
 #[test] fn native_source(){
  assert_eq!(super::left(-128,1),0);assert_eq!(super::left(127,1),-2);
  assert_eq!(super::right(-1,127),-1);
  assert_eq!(super::narrow_count(i128::MIN,127),-1);
  assert!(std::panic::catch_unwind(||super::narrow_count(1,-128)).is_err());assert_eq!(super::unsigned(u128::MAX,127),1);
  assert!(std::panic::catch_unwind(||super::left(1,8)).is_err());
  assert!(std::panic::catch_unwind(||super::right(-1,-1)).is_err());
  assert!(std::panic::catch_unwind(||super::unused(1,8)).is_err());
  assert_eq!(super::lazy(1,-1,false),7);
 }
}
"#,
    );
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
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let out = fixture.cargo(true);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let programs = fixture.exports();
    assert_eq!(programs.len(), 7);
    for optimize in [false, true] {
        for (name, input, expected) in [
            ("guard_before_divergence", "(1 -1)", None),
            (
                "narrow_count",
                "(-170141183460469231731687303715884105728 127)",
                Some("81ff"),
            ),
            ("narrow_count", "(1 0)", Some("01")),
            ("narrow_count", "(1 -128)", None),
            ("left", "(-128 1)", Some("80")),
            ("left", "(127 1)", Some("81fe")),
            ("left", "(1 8)", None),
            ("left", "(1 4294967295)", None),
            ("right", "(-1 127)", Some("81ff")),
            ("right", "(-1 -1)", None),
            (
                "right",
                "(-170141183460469231731687303715884105728 127)",
                Some("81ff"),
            ),
            (
                "unsigned",
                "(340282366920938463463374607431768211455 127)",
                Some("01"),
            ),
            ("unused", "(1 -1)", None),
            ("unused", "(1 8)", None),
            ("lazy", "(1 -1 ())", Some("07")),
            ("lazy", "(1 -1 1)", None),
        ] {
            let p = programs
                .iter()
                .find(|p| p.name.ends_with(&format!("::{name}")))
                .unwrap();
            let code = chialisp_oxide_ir::compile(p, optimize).unwrap();
            let actual = vm::execute(&code, input, 10_000_000).unwrap().outcome;
            if let Some(hex) = expected {
                assert_eq!(
                    actual,
                    Outcome::Accepted {
                        output_hex: hex.into()
                    },
                    "{name} {input}"
                );
            } else {
                assert_eq!(actual, Outcome::Rejected, "{name} {input}");
            }
        }
        let narrow = programs
            .iter()
            .find(|p| p.name.ends_with("::narrow_count"))
            .unwrap();
        let code = chialisp_oxide_ir::compile(narrow, optimize).unwrap();
        for count in i8::MIN..=i8::MAX {
            let input = format!("(1 {count})");
            let actual = vm::execute(&code, &input, 10_000_000).unwrap().outcome;
            let expected = if count < 0 {
                Outcome::Rejected
            } else {
                Outcome::Accepted {
                    output_hex: if count == 0 { "01" } else { "80" }.into(),
                }
            };
            assert_eq!(actual, expected, "complete signed i8 count domain: {count}");
        }
    }
}

#[test]
fn user_casts_and_reused_or_different_width_count_metadata_are_not_admitted() {
    for source in [
        "#[chialisp_oxide::puzzle]pub fn puzzle(a:i8,b:i16)->i8{a<<(b as u16)}",
        "#[chialisp_oxide::puzzle]pub fn puzzle(a:i8,b:i16)->i8{a<<(b as u32)}",
        "#[chialisp_oxide::puzzle]pub fn puzzle(a:i8,b:i16)->(i8,u16){let count=b as u16;(a<<count,count)}",
        "#[chialisp_oxide::puzzle]pub fn puzzle(a:i8,b:i16)->i8{let count=b as u16;assert!(count<8);a<<count}",
    ] {
        let fixture = Fixture::with_sdk_alias("chialisp_oxide");
        fixture.source(source);
        let output = fixture.cargo(true);
        assert!(!output.status.success(), "general casts must remain closed");
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(errors.contains("unsupported MIR value"), "{errors}");
        assert!(fixture.exports().is_empty());
    }
}
