//! Real primitive Rust words, not a blanket translation of operator traits.
mod support;
#[path = "support/vm.rs"]
mod vm;
use chialisp_oxide_backend_experiment::{FailureCode, FailureKind, Outcome};
use std::{collections::BTreeMap, path::Path, process::Command};
use support::Fixture;

struct Case {
    ty: &'static str,
    left: String,
    right: String,
    values: Vec<String>,
    assigned: Vec<String>,
}
fn cases() -> Vec<Case> {
    let mut result = Vec::new();
    macro_rules! unsigned {($($ty:ty),*) => {$({
        for (a,b) in [(0,0),(0,<$ty>::MAX),(<$ty>::MAX,0),(<$ty>::MAX,<$ty>::MAX),(1,<$ty>::MAX),(<$ty>::MAX/3,<$ty>::MAX/5),(1 << (<$ty>::BITS-1),1)] {
            let (a,b):($ty,$ty)=(a,b); let mut assigned=a; assigned&=b; assigned|=a; assigned^=b;
            result.push(Case{ty:stringify!($ty),left:a.to_string(),right:b.to_string(),values:vec![(a&b).to_string(),(a|b).to_string(),(a^b).to_string(),(!a).to_string()],assigned:vec![assigned.to_string(),(!assigned).to_string()]});
        }
    })*};}
    macro_rules! signed {($($ty:ty),*) => {$({
        for (a,b) in [(<$ty>::MIN,<$ty>::MAX),(<$ty>::MIN,<$ty>::MIN),(<$ty>::MAX,<$ty>::MAX),(-1,0),(0,-1),(-1,-1),(-2,1),(<$ty>::MIN/3,<$ty>::MAX/5)] {
            let (a,b):($ty,$ty)=(a,b); let mut assigned=a; assigned&=b; assigned|=a; assigned^=b;
            result.push(Case{ty:stringify!($ty),left:a.to_string(),right:b.to_string(),values:vec![(a&b).to_string(),(a|b).to_string(),(a^b).to_string(),(!a).to_string()],assigned:vec![assigned.to_string(),(!assigned).to_string()]});
        }
    })*};}
    unsigned!(u8, u16, u32, u64, u128);
    signed!(i8, i16, i32, i64, i128);
    result
}
fn source(cases: &[Case]) -> String {
    let mut source = include_str!("fixtures/bitwise.rs").to_string();
    for ty in [
        "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128",
    ] {
        source.push_str(&format!("\n#[chialisp_oxide::puzzle] pub fn bits_{ty}(a:{ty},b:{ty})->({ty},{ty},{ty},{ty}) {{(a&b,a|b,a^b,!a)}}\n#[chialisp_oxide::puzzle] pub fn assign_{ty}(a:{ty},b:{ty})->({ty},{ty}) {{let mut value=a;value&=b;value|=a;value^=b;(value,!value)}}\n"));
    }
    source.push_str("#[cfg(test)] mod native_words {use super::*; #[test] fn actual_fixed_width_operations(){\n");
    for case in cases {
        let tuple = |values: &[String]| {
            values
                .iter()
                .map(|v| format!("{v}{}", case.ty))
                .collect::<Vec<_>>()
                .join(",")
        };
        source.push_str(&format!(
            "assert_eq!(bits_{}({}{},{}{}),({}));assert_eq!(assign_{}({}{},{}{}),({}));\n",
            case.ty,
            case.left,
            case.ty,
            case.right,
            case.ty,
            tuple(&case.values),
            case.ty,
            case.left,
            case.ty,
            case.right,
            case.ty,
            tuple(&case.assigned)
        ));
    }
    source.push_str("}}\n");
    source
}
fn success(output: std::process::Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn native(fixture: &Fixture) {
    success(
        Command::new(Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/cargo"))
            .current_dir(&fixture.0)
            .env("RUSTC", fixture.compiler())
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER")
            .env_remove("CHIALISP_OXIDE_CARGO_MODE")
            .args(["test", "--lib", "--offline", "--target-dir"])
            .arg(fixture.0.join("native-build"))
            .output()
            .unwrap(),
    );
}
fn expected(text: &str) -> Outcome {
    vm::execute(&[1], text, 10_000_000).unwrap().outcome
}
fn tuple(values: &[String]) -> Outcome {
    expected(&format!("({})", values.join(" ")))
}
fn bit(value: bool) -> &'static str {
    if value { "1" } else { "()" }
}

#[test]
fn all_primitive_words_booleans_and_masks_match_native_and_independent_chialisp() {
    let cases = cases();
    let fixture = Fixture::new();
    fixture.source(&source(&cases));
    native(&fixture);
    success(fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 43);
    let mut references = BTreeMap::new();
    for (ty, mask) in [
        ("u8", u8::MAX.to_string()),
        ("u16", u16::MAX.to_string()),
        ("u32", u32::MAX.to_string()),
        ("u64", u64::MAX.to_string()),
        ("u128", u128::MAX.to_string()),
        ("i8", "-1".into()),
        ("i16", "-1".into()),
        ("i32", "-1".into()),
        ("i64", "-1".into()),
        ("i128", "-1".into()),
    ] {
        references.insert(
            ty,
            chialisp_oxide_reference::compile(&format!(
                "(mod (A B) (list (logand A B) (logior A B) (logxor A B) (logxor A {mask})))"
            ))
            .unwrap(),
        );
    }
    let odd_reference =
        chialisp_oxide_reference::compile("(mod (X) (defun odd (X) (logand X 1)) (odd X))")
            .unwrap();
    for optimize in [false, true] {
        let compiled = programs
            .iter()
            .map(|p| {
                (
                    p.name.rsplit("::").next().unwrap(),
                    chialisp_oxide_ir::compile(p, optimize).unwrap(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        for case in &cases {
            let input = format!("({} {})", case.left, case.right);
            let wanted = tuple(&case.values);
            assert_eq!(
                vm::execute(
                    &compiled[format!("bits_{}", case.ty).as_str()],
                    &input,
                    10_000_000
                )
                .unwrap()
                .outcome,
                wanted,
                "{} {input} optimize={optimize}",
                case.ty
            );
            assert_eq!(
                vm::execute(&references[case.ty], &input, 10_000_000)
                    .unwrap()
                    .outcome,
                wanted,
                "Chialisp {} {input}",
                case.ty
            );
            assert_eq!(
                vm::execute(
                    &compiled[format!("assign_{}", case.ty).as_str()],
                    &input,
                    10_000_000
                )
                .unwrap()
                .outcome,
                tuple(&case.assigned)
            );
        }
        for a in [false, true] {
            for b in [false, true] {
                let input = format!("({} {})", bit(a), bit(b));
                let wanted = [a && b, a || b, a != b, !a].map(|v| bit(v).to_string());
                assert_eq!(
                    vm::execute(&compiled["bool_ops"], &input, 10_000_000)
                        .unwrap()
                        .outcome,
                    tuple(&wanted)
                );
            }
        }
        for flags in 0..=u8::MAX {
            let run = vm::execute(
                &compiled["validate_flags"],
                &format!("({flags})"),
                10_000_000,
            )
            .unwrap();
            if flags <= 3 {
                assert_eq!(
                    run.outcome,
                    expected(bit(flags == 2 || flags == 3)),
                    "validate_flags flags={flags} optimize={optimize}"
                );
            } else {
                assert_eq!(
                    run.outcome,
                    Outcome::Rejected,
                    "validate_flags flags={flags} optimize={optimize}"
                );
                assert_eq!(
                    run.failure.unwrap().code(),
                    FailureCode::Raise,
                    "validate_flags flags={flags} optimize={optimize}"
                );
            }
            let write = flags & 2 == 2;
            let read = flags & 1 == 1;
            for (name, wanted) in [
                ("comparison_is_true", expected(bit(write))),
                (
                    "comparison_tuple",
                    tuple(&[bit(write).into(), bit(read).into()]),
                ),
                ("comparison_nested", expected(bit(write && !read))),
            ] {
                assert_eq!(
                    vm::execute(&compiled[name], &format!("({flags})"), 10_000_000)
                        .unwrap()
                        .outcome,
                    wanted,
                    "{name} flags={flags} optimize={optimize}"
                );
            }
        }
        for value in [i64::MIN, -3, -2, -1, 0, 1, 2, i64::MAX] {
            let input = format!("({value})");
            let wanted = expected(&(value & 1).to_string());
            assert_eq!(
                vm::execute(&compiled["odd"], &input, 10_000_000)
                    .unwrap()
                    .outcome,
                wanted
            );
            assert_eq!(
                vm::execute(&odd_reference, &input, 10_000_000)
                    .unwrap()
                    .outcome,
                wanted
            );
        }
        for bad in [
            "()",
            "(1)",
            "(1 2 3)",
            "(1 . 2)",
            "(0x00 1)",
            "(0x0001 1)",
            "(256 1)",
            "(-1 1)",
            "((1 . 2) 1)",
        ] {
            assert_eq!(
                vm::execute(&compiled["bits_u8"], bad, 10_000_000)
                    .unwrap()
                    .outcome,
                Outcome::Rejected
            );
        }
        for bad in ["(-129 1)", "(128 1)", "(0xff80 1)", "(0x0001 1)"] {
            assert_eq!(
                vm::execute(&compiled["bits_i8"], bad, 10_000_000)
                    .unwrap()
                    .outcome,
                Outcome::Rejected
            );
        }
        for bad in ["(0x00 1)", "(2 1)", "(1 2)", "((1) 1)"] {
            assert_eq!(
                vm::execute(&compiled["bool_ops"], bad, 10_000_000)
                    .unwrap()
                    .outcome,
                Outcome::Rejected
            );
        }
        for flag in [false, true] {
            let input = format!("({})", bit(flag));
            for name in ["eager_and", "eager_or"] {
                let run = vm::execute(&compiled[name], &input, 10_000_000).unwrap();
                assert_eq!(run.outcome, Outcome::Rejected);
                assert_eq!(run.failure.unwrap().code(), FailureCode::Raise);
            }
            for (name, accept) in [("lazy_and", !flag), ("lazy_or", flag)] {
                let run = vm::execute(&compiled[name], &input, 10_000_000).unwrap();
                assert_eq!(
                    run.outcome,
                    if accept {
                        expected(bit(flag))
                    } else {
                        Outcome::Rejected
                    }
                );
            }
        }
        for name in ["unused", "unused_not", "unused_helper"] {
            assert_eq!(
                vm::execute(&compiled[name], "(0)", 10_000_000)
                    .unwrap()
                    .outcome,
                expected("7")
            );
            assert_eq!(
                vm::execute(&compiled[name], "(255)", 10_000_000)
                    .unwrap()
                    .outcome,
                Outcome::Rejected
            );
        }
        assert_eq!(
            vm::execute(&compiled["lazy"], "(255 ())", 10_000_000)
                .unwrap()
                .outcome,
            expected("7")
        );
        assert_eq!(
            vm::execute(&compiled["lazy"], "(255 1)", 10_000_000)
                .unwrap()
                .outcome,
            Outcome::Rejected
        );
        assert_eq!(
            vm::execute(&compiled["runtime_snapshot"], "(165)", 10_000_000)
                .unwrap()
                .outcome,
            expected("(5 90)")
        );
        assert_eq!(
            vm::execute(&compiled["recursive"], "(165 3)", 10_000_000)
                .unwrap()
                .outcome,
            expected("91")
        );
        assert_eq!(
            vm::execute(&compiled["misleading_names"], "(1 2)", 10_000_000)
                .unwrap()
                .outcome,
            expected("(3 1)")
        );
        for (name, outcome, code, kind) in [
            (
                "reject_then_spin",
                Outcome::Rejected,
                FailureCode::Raise,
                FailureKind::Program,
            ),
            (
                "spin_then_reject",
                Outcome::Inconclusive,
                FailureCode::CostExceeded,
                FailureKind::Resource,
            ),
        ] {
            let run =
                chialisp_oxide_backend_experiment::execute(&compiled[name], "()", 100_000).unwrap();
            assert_eq!(run.outcome, outcome);
            let failure = run.failure.unwrap();
            assert_eq!(failure.code(), code);
            assert_eq!(failure.kind(), kind);
        }
        for name in ["eager_false_spin", "eager_true_spin"] {
            let run =
                chialisp_oxide_backend_experiment::execute(&compiled[name], "()", 100_000).unwrap();
            assert_eq!(run.outcome, Outcome::Inconclusive);
            let failure = run.failure.unwrap();
            assert_eq!(failure.code(), FailureCode::CostExceeded);
            assert_eq!(failure.kind(), FailureKind::Resource);
        }
        for (name, wanted) in [("lazy_false_spin", false), ("lazy_true_spin", true)] {
            assert_eq!(
                vm::execute(&compiled[name], "()", 100_000).unwrap().outcome,
                expected(bit(wanted))
            );
        }
    }
}

#[test]
fn user_operator_traits_are_not_mistaken_for_primitive_mir() {
    for source in [
        "#[repr(transparent)] pub struct Word(pub u8); impl std::ops::BitAnd for Word {type Output=Self; fn bitand(self,rhs:Self)->Self {Word(self.0^rhs.0)}} #[chialisp_oxide::puzzle] pub fn puzzle(a:Word,b:Word)->Word {a&b}",
        "#[repr(transparent)] pub struct Word(pub u8); impl std::ops::Not for Word {type Output=Self; fn not(self)->Self {self}} #[chialisp_oxide::puzzle] pub fn puzzle(a:Word)->Word {!a}",
    ] {
        let fixture = Fixture::new();
        fixture.source(source);
        success(fixture.cargo(false));
        let output = fixture.cargo(true);
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("generic call") || error.contains("unsupported"),
            "{error}"
        );
        assert!(!error.contains("internal compiler error"), "{error}");
        assert!(fixture.exports().is_empty());
    }
}
