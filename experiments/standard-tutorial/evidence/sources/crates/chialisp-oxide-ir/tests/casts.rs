//! Primitive Rust integer `as`: finite native/VM observations, not preservation.
#[path = "runtime_evaluation/support.rs"]
#[allow(dead_code)]
mod support;
use chialisp_oxide_ir::{BinaryOp, Environment, Expr, IntegerCast, Type, compile, optimize};
use chialisp_oxide_vm::FailureCode;
use num_bigint::BigInt;
use support::*;
fn integer(bits: u16, signed: bool) -> Type {
    Type::Int { bits, signed }
}
fn literal(n: impl ToString, ty: &Type) -> Expr {
    Expr::Int {
        value: n.to_string(),
        ty: ty.clone(),
    }
}
fn cast(value: Expr, source: &Type, target: &Type) -> Expr {
    Expr::IntCast(IntegerCast {
        value: Box::new(value),
        source: source.clone(),
        target: target.clone(),
    })
}
fn wire(n: impl ToString) -> Vec<u8> {
    let n: BigInt = n.to_string().parse().unwrap();
    atom(&if n == BigInt::from(0) {
        vec![]
    } else {
        n.to_signed_bytes_be()
    })
}
fn reference(target: &Type) -> Vec<u8> {
    let Type::Int { bits, signed } = target else {
        panic!()
    };
    let modulus = BigInt::from(1u8) << usize::from(*bits);
    let source = if *signed {
        format!(
            "(mod (X) (defun interpret (N) (if (> N {}) (- N {modulus}) N)) (interpret (r (divmod X {modulus}))))",
            (&modulus >> 1usize) - 1
        )
    } else {
        format!("(mod (X) (r (divmod X {modulus})))")
    };
    chialisp_oxide_reference::compile(&source).unwrap()
}
#[test]
#[allow(
    clippy::unnecessary_cast,
    reason = "All primitive identity and nonidentity casts are the native test oracle"
)]
fn all_100_primitive_pairs_match_native_casts_and_independent_floor_modulo_reference() {
    macro_rules! pair {
        ($from:ty,$fb:literal,$fs:literal,$to:ty,$tb:literal,$ts:literal) => {{
            let source = integer($fb, $fs);
            let target = integer($tb, $ts);
            let p = program(
                Environment::Strict,
                vec![source.clone()],
                target.clone(),
                cast(arg(0), &source, &target),
                vec![],
            );
            let mut values: Vec<$from> = if $fb == 8 {
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
            for width in [($tb - 1) as usize, $tb as usize] {
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
            let canonical = reference(&target);
            for (_, code) in modes(&p) {
                for value in &values {
                    let env = list(&[wire(value)]);
                    let expected = wire(*value as $to);
                    accepted(run(&code, &env, FINITE_LIMIT), &expected);
                    accepted(run(&canonical, &env, FINITE_LIMIT), &expected);
                }
            }
        }};
    }
    macro_rules! targets {
        ($from:ty,$bits:literal,$signed:literal) => {
            pair!($from, $bits, $signed, u8, 8, false);
            pair!($from, $bits, $signed, i8, 8, true);
            pair!($from, $bits, $signed, u16, 16, false);
            pair!($from, $bits, $signed, i16, 16, true);
            pair!($from, $bits, $signed, u32, 32, false);
            pair!($from, $bits, $signed, i32, 32, true);
            pair!($from, $bits, $signed, u64, 64, false);
            pair!($from, $bits, $signed, i64, 64, true);
            pair!($from, $bits, $signed, u128, 128, false);
            pair!($from, $bits, $signed, i128, 128, true);
        };
    }
    targets!(u8, 8, false);
    targets!(i8, 8, true);
    targets!(u16, 16, false);
    targets!(i16, 16, true);
    targets!(u32, 32, false);
    targets!(i32, 32, true);
    targets!(u64, 64, false);
    targets!(i64, 64, true);
    targets!(u128, 128, false);
    targets!(i128, 128, true);
}
#[test]
fn strict_initializers_lazy_arms_and_abi_boundaries_remain_distinct() {
    let source = integer(8, true);
    let target = integer(128, false);
    let failure = Expr::Binary {
        kind: BinaryOp::Div,
        ty: source.clone(),
        left: Box::new(literal(1, &source)),
        right: Box::new(literal(0, &source)),
    };
    let strict = program(
        Environment::Ignored,
        vec![],
        target.clone(),
        sequence(cast(failure.clone(), &source, &target), literal(7, &target)),
        vec![],
    );
    for (_, code) in modes(&strict) {
        failed(
            run(&code, &atom(&[]), FINITE_LIMIT),
            FailureCode::DivisionByZero,
        );
    }
    let lazy = program(
        Environment::Ignored,
        vec![],
        target.clone(),
        branch(
            Expr::Bool { value: false },
            cast(failure, &source, &target),
            literal(7, &target),
        ),
        vec![],
    );
    assert!(optimize(&lazy).unwrap().uses_casts());
    for (_, code) in modes(&lazy) {
        accepted(run(&code, &atom(&[]), FINITE_LIMIT), &wire(7));
    }
    let p = program(
        Environment::Strict,
        vec![source.clone()],
        target.clone(),
        cast(arg(0), &source, &target),
        vec![],
    );
    for (_, code) in modes(&p) {
        accepted(
            run(&code, &list(&[wire(-1)]), FINITE_LIMIT),
            &wire(u128::MAX),
        );
        for malformed in [
            atom(&[]),
            list(&[atom(&[0, 1])]),
            list(&[atom(&[0])]),
            list(&[atom(&[0xff, 0xff])]),
            list(&[pair(&wire(1), &wire(2))]),
            pair(&wire(1), &wire(2)),
            list(&[atom(&[1; 32])]),
            list(&[wire(128)]),
            list(&[wire(-129)]),
            list(&[wire(1), wire(2)]),
        ] {
            abi_rejected(run(&code, &malformed, FINITE_LIMIT));
        }
    }
}
#[test]
fn version_7_is_required_and_composes_with_pairs_and_optional_outputs() {
    let source = integer(8, true);
    let target = integer(16, false);
    let output = Expr::Pair {
        first: Box::new(cast(arg(0), &source, &target)),
        rest: Box::new(Expr::OptionalSome {
            value: Box::new(cast(arg(0), &source, &target)),
        }),
    };
    let result = Type::Pair {
        first: Box::new(target.clone()),
        rest: Box::new(Type::Optional {
            item: Box::new(target.clone()),
        }),
    };
    let p = program(
        Environment::Strict,
        vec![source.clone()],
        result,
        output,
        vec![],
    );
    let encoded = serde_json::to_value(&p).unwrap();
    assert_eq!(encoded["ir_version"], 7);
    let round: chialisp_oxide_ir::Program = serde_json::from_value(encoded.clone()).unwrap();
    for optimized in [false, true] {
        let code = compile(&round, optimized).unwrap();
        accepted(
            run(&code, &list(&[wire(-1)]), FINITE_LIMIT),
            &pair(&wire(u16::MAX), &list(&[wire(u16::MAX)])),
        );
    }
    for version in [4, 5, 6, 8] {
        let mut old = encoded.clone();
        old["ir_version"] = version.into();
        assert!(serde_json::from_value::<chialisp_oxide_ir::Program>(old).is_err());
    }
    let simple = program(
        Environment::Ignored,
        vec![],
        target.clone(),
        cast(literal(-1, &source), &source, &target),
        vec![],
    );
    let original = serde_json::to_value(&simple).unwrap();
    for field in ["saturating", "wrapping_authenticated", "ignored"] {
        let mut extra = original.clone();
        extra["program"]["body"][field] = true.into();
        assert!(serde_json::from_value::<chialisp_oxide_ir::Program>(extra).is_err());
    }
    for side in ["source", "target"] {
        let mut extra = original.clone();
        extra["program"]["body"][side]["saturating"] = true.into();
        assert!(serde_json::from_value::<chialisp_oxide_ir::Program>(extra).is_err());
    }
    let mut no_cast = original.clone();
    no_cast["program"]["body"] = serde_json::to_value(literal(7, &target)).unwrap();
    assert!(serde_json::from_value::<chialisp_oxide_ir::Program>(no_cast).is_err());
    for version in [2, 3] {
        let mut old = original.clone();
        old["ir_version"] = version.into();
        old["program"].as_object_mut().unwrap().remove("functions");
        if version == 3 {
            old["environment"] = "raw".into();
            old["program"]["parameters"] =
                serde_json::json!([{"name":"tree","ty":{"kind":"tree"}}]);
        }
        assert!(serde_json::from_value::<chialisp_oxide_ir::Program>(old).is_err());
    }
    let mut legacy = original["program"].clone();
    legacy.as_object_mut().unwrap().remove("functions");
    assert!(serde_json::from_value::<chialisp_oxide_ir::Program>(legacy).is_err());
}
#[test]
fn explicit_raw_environment_cast_literal_does_not_narrow_input_trees() {
    let source = integer(8, true);
    let target = integer(16, false);
    let p = program(
        Environment::Raw,
        vec![Type::Tree],
        target.clone(),
        cast(literal(-1, &source), &source, &target),
        vec![],
    );
    let encoded = serde_json::to_value(&p).unwrap();
    assert_eq!(encoded["ir_version"], 7);
    let p: chialisp_oxide_ir::Program = serde_json::from_value(encoded).unwrap();
    for (_, code) in modes(&p) {
        for input in [
            atom(&[]),
            wire(7),
            pair(&wire(1), &wire(2)),
            list(&[wire(3), wire(4)]),
        ] {
            accepted(run(&code, &input, FINITE_LIMIT), &wire(u16::MAX));
        }
    }
}

#[test]
fn illegal_metadata_and_source_mismatches_do_not_hide_under_bottom_or_dead_arms() {
    let i8 = integer(8, true);
    let u8 = integer(8, false);
    let bad_types = [
        Type::Bool,
        Type::Unit,
        Type::Tree,
        Type::Atom,
        Type::Bytes { len: 1 },
        Type::Tuple {
            items: vec![i8.clone()],
        },
        integer(7, true),
        integer(256, false),
    ];
    for bad in bad_types {
        for (source, target) in [(bad.clone(), u8.clone()), (u8.clone(), bad)] {
            let expr = cast(
                Expr::Reject {
                    message: "bottom".into(),
                },
                &source,
                &target,
            );
            let p = program(Environment::Ignored, vec![], target.clone(), expr, vec![]);
            assert!(p.validate().is_err());
            assert!(optimize(&p).is_err());
        }
    }
    let mismatch = cast(literal(1, &u8), &i8, &u8);
    let p = program(
        Environment::Ignored,
        vec![],
        u8.clone(),
        branch(Expr::Bool { value: true }, literal(7, &u8), mismatch),
        vec![],
    );
    assert!(
        p.validate()
            .unwrap_err()
            .to_string()
            .contains("source type mismatch")
    );
}
#[test]
fn cast_metadata_and_recursive_child_depth_share_existing_budgets() {
    let ty = integer(8, false);
    for depth in [255, 256] {
        let body = (0..depth).fold(literal(1, &ty), |body, _| cast(body, &ty, &ty));
        let p = program(Environment::Ignored, vec![], ty.clone(), body, vec![]);
        if depth == 255 {
            p.validate().unwrap();
        } else {
            assert!(
                p.validate()
                    .unwrap_err()
                    .to_string()
                    .contains("expression nesting")
            );
        }
    }
    // Each cast charges two independent metadata types plus its child, even when
    // rejection makes its value unobservable. Do not allow 20k type nodes twice.
    let entries = (0..256)
        .map(|_| {
            cast(
                Expr::Reject {
                    message: "bottom".into(),
                },
                &ty,
                &ty,
            )
        })
        .collect::<Vec<_>>();
    let body = Expr::Tuple { items: entries };
    let mut p = program(Environment::Ignored, vec![], Type::Unit, Expr::Unit, vec![]);
    p.functions = (0..40)
        .map(|id| {
            function(
                id,
                vec![],
                Type::Tuple {
                    items: vec![ty.clone(); 256],
                },
                body.clone(),
            )
        })
        .collect();
    assert!(p.validate().is_err());
}
