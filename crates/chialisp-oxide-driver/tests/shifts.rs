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
fn explicit_user_integer_casts_remain_distinct_from_compiler_shift_metadata() {
    for (source, tuple) in [
        (
            "#[chialisp_oxide::puzzle]pub fn puzzle(a:i8,b:i16)->i8{a<<(b as u16)}",
            false,
        ),
        (
            "#[chialisp_oxide::puzzle]pub fn puzzle(a:i8,b:i16)->i8{a<<(b as u32)}",
            false,
        ),
        (
            "#[chialisp_oxide::puzzle]pub fn puzzle(a:i8,b:i16)->(i8,u16){let count=b as u16;(a<<count,count)}",
            true,
        ),
        (
            "#[chialisp_oxide::puzzle]pub fn puzzle(a:i8,b:i16)->i8{let count=b as u16;assert!(count<8);a<<count}",
            false,
        ),
    ] {
        let fixture = Fixture::with_sdk_alias("chialisp_oxide");
        fixture.source(source);
        let output = fixture.cargo(true);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let programs = fixture.exports();
        assert_eq!(programs.len(), 1);
        assert!(programs[0].uses_casts());
        assert_eq!(serde_json::to_value(&programs[0]).unwrap()["ir_version"], 7);
        for optimize in [false, true] {
            let code = chialisp_oxide_ir::compile(&programs[0], optimize).unwrap();
            assert_eq!(
                vm::execute(&code, "(127 1)", 10_000_000).unwrap().outcome,
                Outcome::Accepted {
                    output_hex: if tuple { "ff81feff0180" } else { "81fe" }.into()
                }
            );
            assert_eq!(
                vm::execute(&code, "(127 0)", 10_000_000).unwrap().outcome,
                Outcome::Accepted {
                    output_hex: if tuple { "ff7fff8080" } else { "7f" }.into()
                }
            );
            for input in ["(1 -1)", "(1 8)", "(1 257)"] {
                let rejected = vm::execute(&code, input, 10_000_000).unwrap();
                assert_eq!(rejected.outcome, Outcome::Rejected);
                assert_eq!(format!("{:?}", rejected.failure.unwrap().code()), "Raise");
            }
        }
    }
}

#[test]
fn previously_emitted_signed_shift_keeps_both_exact_artifacts_without_cast_capability() {
    let output = Command::new(env!("CARGO_BIN_EXE_chialisp-oxide-driver"))
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/signed_shift_frozen.rs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let program: chialisp_oxide_ir::Program = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!program.uses_casts());
    assert!(!program.uses_runtime());
    // Captured before this change with the retained local driver+CLI. Receipt
    // target/primitive-casts49/signedshift-before records tool identities; this
    // fixed regression is not a compiler-preservation or release-auth claim.
    for (optimize, bytes, expected_hash) in [
        (
            false,
            12162,
            "632a294dcd8c612040543324978ef4f35478504f1b980c0bb4218056528ab2a1",
        ),
        (
            true,
            1867,
            "fa15f35554f2933e64c9dd54a6f3ea4cb7073f2a12004542b883e3decdbb1130",
        ),
    ] {
        let code = chialisp_oxide_ir::compile(&program, optimize).unwrap();
        assert_eq!(code.len(), bytes);
        let mut allocator = clvmr_sdk::Allocator::new();
        let node = clvmr_sdk::serde::node_from_bytes(&mut allocator, &code).unwrap();
        let actual = tree_hash(&allocator, node);
        assert_eq!(actual, expected_hash);
    }
}

fn tree_hash(allocator: &clvmr_sdk::Allocator, node: clvmr_sdk::NodePtr) -> String {
    use sha2::{Digest, Sha256};
    fn digest(allocator: &clvmr_sdk::Allocator, node: clvmr_sdk::NodePtr) -> [u8; 32] {
        match allocator.sexp(node) {
            clvmr_sdk::allocator::SExp::Atom => {
                let mut hash = Sha256::new();
                hash.update([1]);
                hash.update(allocator.atom(node).as_ref());
                hash.finalize().into()
            }
            clvmr_sdk::allocator::SExp::Pair(first, rest) => {
                let mut hash = Sha256::new();
                hash.update([2]);
                hash.update(digest(allocator, first));
                hash.update(digest(allocator, rest));
                hash.finalize().into()
            }
        }
    }
    digest(allocator, node)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
