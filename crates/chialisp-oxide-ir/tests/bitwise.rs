//! Native fixed-width/Boolean oracles and adversarial executions. These finite
//! tests do not prove Rust extraction, compiler preservation or termination.
#[path = "runtime_evaluation/support.rs"]
#[allow(
    dead_code,
    reason = "Shared runtime fixtures include unrelated operations."
)]
mod support;

use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Function, Type, compile};
use chialisp_oxide_vm::FailureCode;
use num_bigint::BigInt;
use support::*;

const OPS: [BinaryOp; 3] = [BinaryOp::Bitand, BinaryOp::Bitor, BinaryOp::Bitxor];
fn bits(kind: BinaryOp, ty: &Type, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        kind,
        ty: ty.clone(),
        left: Box::new(left),
        right: Box::new(right),
    }
}
fn marker(runtime: bool) -> Vec<Function> {
    if runtime {
        vec![function(0, vec![], Type::Unit, Expr::Unit)]
    } else {
        vec![]
    }
}
fn number(value: impl ToString) -> Vec<u8> {
    let n: BigInt = value.to_string().parse().unwrap();
    atom(&if n == BigInt::from(0) {
        vec![]
    } else {
        n.to_signed_bytes_be()
    })
}
fn literal(ty: &Type, value: impl ToString) -> Expr {
    if *ty == Type::Bool {
        Expr::Bool {
            value: value.to_string() != "0",
        }
    } else {
        Expr::Int {
            value: value.to_string(),
            ty: ty.clone(),
        }
    }
}
fn rejection(ty: &Type) -> Expr {
    sequence(
        Expr::Reject {
            message: "required operand".into(),
        },
        literal(ty, 1),
    )
}

#[test]
fn all_native_integer_widths_match_both_lowering_paths_and_preserve_wire_versions() {
    macro_rules! native {
        ($native:ty, $width:literal, $signed:literal) => {{
            let ty = Type::Int {
                bits: $width,
                signed: $signed,
            };
            let mut values: Vec<$native> = vec![
                0,
                1,
                2,
                <$native>::MIN,
                <$native>::MIN + 1,
                <$native>::MAX,
                <$native>::MAX / 2,
                <$native>::MAX / 3,
                !(<$native>::MAX / 3),
                !0,
                !1,
            ];
            values.sort_unstable();
            values.dedup();
            for runtime in [false, true] {
                for op in OPS {
                    let p = program(
                        Environment::Strict,
                        vec![ty.clone(), ty.clone()],
                        ty.clone(),
                        bits(op, &ty, arg(0), arg(1)),
                        marker(runtime),
                    );
                    let wire = serde_json::to_value(&p).unwrap();
                    assert_eq!(wire.get("ir_version").is_some(), runtime);
                    let restored = serde_json::from_value(wire).unwrap();
                    for (_, code) in modes(&restored) {
                        for &a in &values {
                            for &b in &values {
                                // The oracle uses actual Rust primitive operators at
                                // the declared width, independently of CLVM/BigInt ops.
                                let expected: $native = match op {
                                    BinaryOp::Bitand => a & b,
                                    BinaryOp::Bitor => a | b,
                                    BinaryOp::Bitxor => a ^ b,
                                    _ => unreachable!(),
                                };
                                accepted(
                                    run(&code, &list(&[number(a), number(b)]), FINITE_LIMIT),
                                    &number(expected),
                                );
                            }
                        }
                    }
                }
            }
        }};
    }
    native!(u8, 8, false);
    native!(i8, 8, true);
    native!(u16, 16, false);
    native!(i16, 16, true);
    native!(u32, 32, false);
    native!(i32, 32, true);
    native!(u64, 64, false);
    native!(i64, 64, true);
    native!(u128, 128, false);
    native!(i128, 128, true);
}

#[test]
fn canonical_boolean_bitwise_is_strict_and_returns_only_nil_or_one() {
    for runtime in [false, true] {
        for op in OPS {
            let p = program(
                Environment::Strict,
                vec![Type::Bool, Type::Bool],
                Type::Bool,
                bits(op, &Type::Bool, arg(0), arg(1)),
                marker(runtime),
            );
            for (_, code) in modes(&p) {
                for a in [false, true] {
                    for b in [false, true] {
                        let wanted = match op {
                            BinaryOp::Bitand => a & b,
                            BinaryOp::Bitor => a | b,
                            BinaryOp::Bitxor => a ^ b,
                            _ => unreachable!(),
                        };
                        accepted(
                            run(
                                &code,
                                &list(&[uint(u16::from(a)), uint(u16::from(b))]),
                                LIMIT,
                            ),
                            &uint(u16::from(wanted)),
                        );
                    }
                }
                for bad in [atom(&[0]), atom(&[0, 1]), uint(2), atom(&[0xff])] {
                    for environment in [list(&[bad.clone(), uint(0)]), list(&[uint(1), bad])] {
                        failed(run(&code, &environment, LIMIT), FailureCode::Raise);
                    }
                }
                for malformed in [
                    atom(&[]),
                    list(&[uint(1)]),
                    list(&[uint(1), uint(0), uint(1)]),
                    pair(&uint(1), &pair(&uint(0), &atom(&[0]))),
                    list(&[pair(&uint(0), &uint(1)), uint(0)]),
                ] {
                    abi_rejected(run(&code, &malformed, LIMIT));
                }
            }
        }
        for (op, value) in [(BinaryOp::Bitand, false), (BinaryOp::Bitor, true)] {
            for (left, right) in [
                (Expr::Bool { value }, rejection(&Type::Bool)),
                (rejection(&Type::Bool), Expr::Bool { value }),
            ] {
                let strict = program(
                    Environment::Ignored,
                    vec![],
                    Type::Bool,
                    bits(op, &Type::Bool, left, right),
                    marker(runtime),
                );
                for (_, code) in modes(&strict) {
                    failed(run(&code, &atom(&[]), LIMIT), FailureCode::Raise);
                }
            }
            let lazy = program(
                Environment::Ignored,
                vec![],
                Type::Bool,
                branch(
                    Expr::Bool { value },
                    if value {
                        Expr::Bool { value }
                    } else {
                        rejection(&Type::Bool)
                    },
                    if value {
                        rejection(&Type::Bool)
                    } else {
                        Expr::Bool { value }
                    },
                ),
                marker(runtime),
            );
            for (_, code) in modes(&lazy) {
                accepted(run(&code, &atom(&[]), LIMIT), &uint(u16::from(value)));
            }
        }
    }
}

#[test]
fn integer_abi_rejects_padding_width_overflow_and_wrong_shapes_even_when_masked() {
    for width in [8, 16, 32, 64, 128] {
        for signed in [false, true] {
            let ty = Type::Int {
                bits: width,
                signed,
            };
            let magnitude = BigInt::from(1u8) << usize::from(width - u16::from(signed));
            let min = if signed { -&magnitude } else { BigInt::from(0) };
            let max = &magnitude - 1u8;
            for runtime in [false, true] {
                for op in OPS {
                    // Input checks must survive annihilator constants and an
                    // unused second parameter, not just direct Arg/Arg results.
                    let constant = if op == BinaryOp::Bitor {
                        max.clone()
                    } else {
                        BigInt::from(0)
                    };
                    let p = program(
                        Environment::Strict,
                        vec![ty.clone(), ty.clone()],
                        ty.clone(),
                        bits(op, &ty, arg(0), literal(&ty, constant)),
                        marker(runtime),
                    );
                    for (_, code) in modes(&p) {
                        for bad in [
                            atom(&[0]),
                            atom(&[0, 1]),
                            atom(&[0xff, 0xff]),
                            number(&min - 1u8),
                            number(&max + 1u8),
                        ] {
                            for env in [list(&[bad.clone(), number(0)]), list(&[number(0), bad])] {
                                failed(run(&code, &env, FINITE_LIMIT), FailureCode::Raise);
                            }
                        }
                        for malformed in [
                            atom(&[]),
                            list(&[number(0)]),
                            list(&[number(0), number(0), number(0)]),
                            pair(&number(0), &pair(&number(0), &atom(&[0]))),
                            list(&[pair(&number(0), &number(0)), number(0)]),
                        ] {
                            abi_rejected(run(&code, &malformed, FINITE_LIMIT));
                        }
                        failed(
                            run(&code, &list(&[number(0), number(0)]), 1),
                            FailureCode::CostExceeded,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn byte_tree_ordering_mixed_widths_and_other_operations_remain_rejected_in_dead_code() {
    for op in OPS {
        for (ty, value) in [
            (Type::Atom, Expr::Atom { hex: "00".into() }),
            (Type::Bytes { len: 1 }, Expr::Bytes { hex: "01".into() }),
            (Type::Tree, tree_atom("01")),
            (Type::Unit, Expr::Unit),
            (Type::Tuple { items: vec![] }, Expr::Tuple { items: vec![] }),
        ] {
            let invalid = bits(op, &ty, value.clone(), value);
            let p = program(
                Environment::Ignored,
                vec![],
                Type::Bool,
                branch(
                    Expr::Bool { value: true },
                    Expr::Bool { value: true },
                    invalid,
                ),
                vec![],
            );
            assert!(p.validate().is_err());
            for optimize in [false, true] {
                assert!(compile(&p, optimize).is_err());
            }
        }
        let p = program(
            Environment::Ignored,
            vec![],
            u8_type(),
            bits(
                op,
                &u8_type(),
                int(1),
                literal(
                    &Type::Int {
                        bits: 16,
                        signed: false,
                    },
                    1,
                ),
            ),
            vec![],
        );
        assert!(p.validate().is_err());
    }
    for op in [BinaryOp::Div, BinaryOp::Rem, BinaryOp::Shl, BinaryOp::Shr] {
        let p = program(
            Environment::Ignored,
            vec![],
            u8_type(),
            bits(op, &u8_type(), int(1), int(1)),
            vec![],
        );
        assert!(p.validate().is_err());
    }
}

#[test]
fn masks_unused_results_and_lazy_branches_preserve_failure_and_runtime_operand_order() {
    for ty in [
        u8_type(),
        Type::Int {
            bits: 8,
            signed: true,
        },
        Type::Bool,
    ] {
        let all = if ty == Type::Bool {
            1
        } else if ty == u8_type() {
            255
        } else {
            -1
        };
        for op in OPS {
            let identity = literal(&ty, if op == BinaryOp::Bitor { all } else { 0 });
            for runtime in [false, true] {
                for (left, right) in [
                    (rejection(&ty), identity.clone()),
                    (identity.clone(), rejection(&ty)),
                ] {
                    let effect = bits(op, &ty, left, right);
                    let p = program(
                        Environment::Ignored,
                        vec![],
                        Type::Bool,
                        sequence(effect, Expr::Bool { value: true }),
                        marker(runtime),
                    );
                    for (_, code) in modes(&p) {
                        failed(run(&code, &atom(&[]), LIMIT), FailureCode::Raise);
                    }
                }
                let overflow = sequence(add(int(255), int(1)), literal(&ty, 1));
                let p = program(
                    Environment::Ignored,
                    vec![],
                    ty.clone(),
                    bits(op, &ty, identity.clone(), overflow),
                    marker(runtime),
                );
                for (_, code) in modes(&p) {
                    failed(run(&code, &atom(&[]), LIMIT), FailureCode::Raise);
                }
            }
            let table = vec![spin(&ty), function(1, vec![], ty.clone(), rejection(&ty))];
            for (left, right, wanted) in [
                (call(0, vec![]), call(1, vec![]), FailureCode::CostExceeded),
                (call(1, vec![]), call(0, vec![]), FailureCode::Raise),
            ] {
                let expression = bits(op, &ty, left, right);
                for body in [
                    sequence(expression.clone(), Expr::Bool { value: true }),
                    binding(0, ty.clone(), expression, Expr::Bool { value: true }),
                ] {
                    let p = program(
                        Environment::Ignored,
                        vec![],
                        Type::Bool,
                        body,
                        table.clone(),
                    );
                    for (_, code) in modes(&p) {
                        failed(run(&code, &atom(&[]), LIMIT), wanted);
                    }
                }
            }
            for (left, right) in [
                (call(0, vec![]), identity.clone()),
                (identity.clone(), call(0, vec![])),
            ] {
                let p = program(
                    Environment::Ignored,
                    vec![],
                    ty.clone(),
                    bits(op, &ty, left, right),
                    table.clone(),
                );
                for (_, code) in modes(&p) {
                    failed(run(&code, &atom(&[]), LIMIT), FailureCode::CostExceeded);
                }
            }
            let lazy = program(
                Environment::Ignored,
                vec![],
                ty.clone(),
                branch(
                    Expr::Bool { value: false },
                    bits(op, &ty, call(0, vec![]), call(1, vec![])),
                    literal(&ty, 1),
                ),
                table,
            );
            for (_, code) in modes(&lazy) {
                accepted(run(&code, &atom(&[]), LIMIT), &number(1));
            }
        }
    }
}

#[test]
fn runtime_reused_traversal_is_evaluated_once_and_repeated_calls_cost_more() {
    for ty in [u8_type(), Type::Bool] {
        let walk = function(
            0,
            vec![Type::Tree],
            ty.clone(),
            branch(
                is_pair(arg(0)),
                call(0, vec![rest(arg(0))]),
                literal(&ty, 1),
            ),
        );
        let env = right_spine(64, &atom(&[0, 1]));
        for op in OPS {
            let once = program(
                Environment::Raw,
                vec![Type::Tree],
                ty.clone(),
                binding(
                    0,
                    ty.clone(),
                    call(0, vec![arg(0)]),
                    bits(op, &ty, local(0), local(0)),
                ),
                vec![walk.clone()],
            );
            let twice = program(
                Environment::Raw,
                vec![Type::Tree],
                ty.clone(),
                bits(op, &ty, call(0, vec![arg(0)]), call(0, vec![arg(0)])),
                vec![walk.clone()],
            );
            let expected = number(u8::from(op != BinaryOp::Bitxor));
            for optimize in [false, true] {
                let one = compile(&once, optimize).unwrap();
                let two = compile(&twice, optimize).unwrap();
                let one_cost = accepted(run(&one, &env, FINITE_LIMIT), &expected);
                let two_cost = accepted(run(&two, &env, FINITE_LIMIT), &expected);
                assert!(
                    two_cost > one_cost,
                    "an explicitly repeated traversal must cost more"
                );
                let budget = one_cost + (two_cost - one_cost) / 2;
                accepted(run(&one, &env, budget), &expected);
                failed(run(&two, &env, budget), FailureCode::CostExceeded);
            }
        }
    }
}
