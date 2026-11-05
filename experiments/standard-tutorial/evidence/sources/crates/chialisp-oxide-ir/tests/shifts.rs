//! Fixed-width Rust shifts discard bits; CLVM's unbounded shifts alone do not.
#[path = "runtime_evaluation/support.rs"]
#[allow(dead_code)]
mod support;
use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Type};
use chialisp_oxide_vm::FailureCode;
use num_bigint::BigInt;
use support::*;
fn number(n: impl ToString) -> Vec<u8> {
    let n: BigInt = n.to_string().parse().unwrap();
    atom(&if n == BigInt::from(0) {
        vec![]
    } else {
        n.to_signed_bytes_be()
    })
}
fn binary(kind: BinaryOp, ty: &Type, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        kind,
        ty: ty.clone(),
        left: Box::new(left),
        right: Box::new(right),
    }
}
#[test]
fn every_integer_width_matches_native_discard_and_signed_right_shift() {
    macro_rules! check {
        ($native:ty,$bits:literal,$signed:literal) => {{
            let ty = Type::Int {
                bits: $bits,
                signed: $signed,
            };
            let rhs = Type::Int {
                bits: 32,
                signed: false,
            };
            for runtime in [false, true] {
                for kind in [BinaryOp::Shl, BinaryOp::Shr] {
                    let table = if runtime {
                        vec![function(0, vec![], Type::Unit, Expr::Unit)]
                    } else {
                        vec![]
                    };
                    let p = program(
                        Environment::Strict,
                        vec![ty.clone(), rhs.clone()],
                        ty.clone(),
                        binary(kind, &ty, arg(0), arg(1)),
                        table,
                    );
                    for (_, code) in modes(&p) {
                        for value in [0, 1, 2, 3, <$native>::MIN, <$native>::MAX, !0] {
                            for count in [0, 1, $bits - 1, $bits, $bits + 1, u32::MAX] {
                                let expected = if kind == BinaryOp::Shl {
                                    value.checked_shl(count)
                                } else {
                                    value.checked_shr(count)
                                };
                                let actual = run(
                                    &code,
                                    &list(&[number(value), number(count)]),
                                    FINITE_LIMIT,
                                );
                                if let Some(expected) = expected {
                                    accepted(actual, &number(expected));
                                } else {
                                    failed(actual, FailureCode::Raise);
                                }
                            }
                        }
                    }
                }
            }
        }};
    }
    check!(u8, 8, false);
    check!(i8, 8, true);
    check!(u16, 16, false);
    check!(i16, 16, true);
    check!(u32, 32, false);
    check!(i32, 32, true);
    check!(u64, 64, false);
    check!(i64, 64, true);
    check!(u128, 128, false);
    check!(i128, 128, true);
}
#[test]
fn negative_counts_and_unused_shifts_trap_but_lazy_branches_do_not() {
    let ty = u8_type();
    let rhs = Type::Int {
        bits: 16,
        signed: true,
    };
    for kind in [BinaryOp::Shl, BinaryOp::Shr] {
        for runtime in [false, true] {
            let table = if runtime {
                vec![function(0, vec![], Type::Unit, Expr::Unit)]
            } else {
                vec![]
            };
            let operation = binary(kind, &ty, arg(0), arg(1));
            let p = program(
                Environment::Strict,
                vec![ty.clone(), rhs.clone()],
                ty.clone(),
                sequence(operation.clone(), int(7)),
                table.clone(),
            );
            for (_, code) in modes(&p) {
                for count in [-32768, -1, 8, 32767] {
                    failed(
                        run(&code, &list(&[number(1), number(count)]), FINITE_LIMIT),
                        FailureCode::Raise,
                    );
                }
            }
            let lazy = program(
                Environment::Strict,
                vec![ty.clone(), rhs.clone()],
                ty.clone(),
                branch(Expr::Bool { value: false }, operation, int(7)),
                table,
            );
            for (_, code) in modes(&lazy) {
                accepted(
                    run(&code, &list(&[number(1), number(-1)]), FINITE_LIMIT),
                    &number(7),
                );
            }
        }
    }
}
#[test]
fn shifts_preserve_source_operand_failure_order() {
    let ty = u8_type();
    let trap = binary(BinaryOp::Div, &ty, int(1), int(0));
    let raised = sequence(
        Expr::Reject {
            message: "raise".into(),
        },
        int(1),
    );
    for kind in [BinaryOp::Shl, BinaryOp::Shr] {
        for runtime in [false, true] {
            let table = if runtime {
                vec![function(0, vec![], Type::Unit, Expr::Unit)]
            } else {
                vec![]
            };
            for (left, right, expected) in [
                (raised.clone(), trap.clone(), FailureCode::Raise),
                (trap.clone(), raised.clone(), FailureCode::DivisionByZero),
            ] {
                let p = program(
                    Environment::Ignored,
                    vec![],
                    ty.clone(),
                    binary(kind, &ty, left, right),
                    table.clone(),
                );
                for (_, code) in modes(&p) {
                    failed(run(&code, &atom(&[]), FINITE_LIMIT), expected);
                }
            }
        }
    }
}
