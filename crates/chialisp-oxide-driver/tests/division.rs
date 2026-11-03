//! Actual Cargo source extraction, native source tests and both compiled modes.
mod support;
#[path = "support/vm.rs"]
mod vm;
use chialisp_oxide_backend_experiment::Outcome;
use std::{path::Path, process::Command};
use support::Fixture;
#[test]
fn actual_rust_division_remainder_and_strict_helpers() {
    let fixture = Fixture::with_sdk_alias("chialisp_oxide");
    fixture.source(
        r#"
#[chialisp_oxide::puzzle]
pub fn signed(a:i64,b:i64)->(i64,i64) {(a/b,a%b)}
#[chialisp_oxide::puzzle]
pub fn unsigned(a:u128,b:u128)->(u128,u128) {(a/b,a%b)}
fn helper(a:i64,b:i64)->i64 {a%b}
#[chialisp_oxide::puzzle]
pub fn unused(a:i64,b:i64)->i64 {let _value=helper(a,b);7}
#[chialisp_oxide::puzzle]
pub fn lazy(a:i64,b:i64,choose:bool)->i64 {if choose {helper(a,b)} else {7}}
#[chialisp_oxide::puzzle]
pub fn recursive(v:u8)->u8 {if v==0 {1} else {recursive(v-1)/v}}
#[cfg(test)] mod tests {
 #[test] fn native_source() {
   for a in [-7,-6,-1,0,1,6,7,i64::MIN,i64::MAX] {
     for b in [-3,-2,-1,0,1,2,3] {
       let checked=a.checked_div(b).zip(a.checked_rem(b));
       assert_eq!(std::panic::catch_unwind(||super::signed(a,b)).ok(),checked);
     }
   }
   assert_eq!(super::unsigned(u128::MAX,3),(u128::MAX/3,u128::MAX%3));
   assert!(std::panic::catch_unwind(||super::unused(1,0)).is_err());
   assert!(std::panic::catch_unwind(||super::unused(i64::MIN,-1)).is_err());
   assert_eq!(super::lazy(1,0,false),7);
   assert_eq!(super::recursive(0),1);
   assert_eq!(super::recursive(1),1);
   assert_eq!(super::recursive(8),0);
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
    let output = fixture.cargo(true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let programs = fixture.exports();
    assert_eq!(programs.len(), 5);
    for optimize in [false, true] {
        for (name, input, expected) in [
            ("recursive", "(0)", Some("01")),
            ("recursive", "(1)", Some("01")),
            ("recursive", "(8)", Some("80")),
            ("signed", "(-7 3)", Some("ff81feff81ff80")),
            ("signed", "(7 -3)", Some("ff81feff0180")),
            ("signed", "(-6 -3)", Some("ff02ff8080")),
            ("signed", "(0 0)", None),
            ("signed", "(-9223372036854775808 -1)", None),
            ("unused", "(1 0)", None),
            ("unused", "(-9223372036854775808 -1)", None),
            ("lazy", "(1 0 ())", Some("07")),
            ("lazy", "(1 0 1)", None),
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
                assert!(
                    matches!(actual, Outcome::Rejected),
                    "{name} {input}: {actual:?}"
                );
            }
        }
    }
}
