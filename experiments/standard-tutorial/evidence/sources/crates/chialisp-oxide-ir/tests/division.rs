//! Checked Rust integer division, not arbitrary-width CLVM numeric parity.
#[path = "runtime_evaluation/support.rs"]
#[allow(dead_code)]
mod support;
use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Type};
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
fn native_all_widths_and_independent_chialisp_agree_in_both_emitters_and_modes() {
    macro_rules! check {
        ($native:ty, $bits:literal, $signed:literal) => {{
            let ty = Type::Int {
                bits: $bits,
                signed: $signed,
            };
            let values: [$native; 7] = [
                0,
                1,
                2,
                3,
                <$native>::MIN,
                <$native>::MAX,
                if $signed { !0 } else { 7 },
            ];
            for runtime in [false, true] {
                for kind in [BinaryOp::Div, BinaryOp::Rem] {
                    let table = if runtime {
                        vec![function(0, vec![], Type::Unit, Expr::Unit)]
                    } else {
                        vec![]
                    };
                    let p = program(
                        Environment::Strict,
                        vec![ty.clone(), ty.clone()],
                        ty.clone(),
                        binary(kind, &ty, arg(0), arg(1)),
                        table,
                    );
                    for (_, code) in modes(&p) {
                        for a in values {
                            for b in values {
                                let expected = if kind == BinaryOp::Div {
                                    a.checked_div(b)
                                } else {
                                    a.checked_rem(b)
                                };
                                let input = list(&[number(a), number(b)]);
                                let result = run(&code, &input, FINITE_LIMIT);
                                if let Some(expected) = expected {
                                    accepted(result, &number(expected));
                                } else {
                                    assert!(
                                        matches!(result, Outcome::Failed(_)),
                                        "{a} {kind:?} {b}"
                                    );
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
    // Independent Chialisp source spells truncation through positive magnitudes.
    // This oracle intentionally has an unbounded output; boundary traps above
    // belong to the Rust source contract, not the Chialisp module.
    let reference = chialisp_oxide_reference::compile("(mod (A B) (defun abs (X) (if (> 0 X) (- 0 X) X)) (defun quotient (A B) (if (= (> 0 A) (> 0 B)) (/ (abs A) (abs B)) (- 0 (/ (abs A) (abs B))))) (list (quotient A B) (- A (* B (quotient A B)))))").unwrap();
    let ty = Type::Int {
        bits: 64,
        signed: true,
    };
    let p = program(
        Environment::Strict,
        vec![ty.clone(), ty.clone()],
        Type::Tuple {
            items: vec![ty.clone(), ty.clone()],
        },
        Expr::Tuple {
            items: vec![
                binary(BinaryOp::Div, &ty, arg(0), arg(1)),
                binary(BinaryOp::Rem, &ty, arg(0), arg(1)),
            ],
        },
        vec![],
    );
    for (_, code) in modes(&p) {
        for a in [-7, -6, -1, 0, 1, 6, 7] {
            for b in [-3, -2, -1, 1, 2, 3] {
                let env = list(&[number(a), number(b)]);
                let expected = list(&[number(a / b), number(a % b)]);
                accepted(run(&code, &env, FINITE_LIMIT), &expected);
                accepted(run(&reference, &env, FINITE_LIMIT), &expected);
            }
        }
    }
}
#[test]
fn unused_failures_remain_strict_and_unchosen_branches_remain_lazy() {
    let ty = u8_type();
    for kind in [BinaryOp::Div, BinaryOp::Rem] {
        for runtime in [false, true] {
            let table = if runtime {
                vec![function(0, vec![], Type::Unit, Expr::Unit)]
            } else {
                vec![]
            };
            let bad = binary(kind, &ty, int(1), int(0));
            let strict = program(
                Environment::Ignored,
                vec![],
                ty.clone(),
                sequence(bad.clone(), int(7)),
                table.clone(),
            );
            for (_, code) in modes(&strict) {
                assert!(matches!(
                    run(&code, &atom(&[]), FINITE_LIMIT),
                    Outcome::Failed(_)
                ));
            }
            let lazy = program(
                Environment::Ignored,
                vec![],
                ty.clone(),
                branch(Expr::Bool { value: false }, bad, int(7)),
                table,
            );
            for (_, code) in modes(&lazy) {
                accepted(run(&code, &atom(&[]), FINITE_LIMIT), &number(7));
            }
        }
    }
}

#[test]
fn division_operands_evaluate_left_before_right_including_distinct_failures() {
    use chialisp_oxide_vm::FailureCode;
    let ty = u8_type();
    let trap = binary(BinaryOp::Div, &ty, int(1), int(0));
    let raised = sequence(
        Expr::Reject {
            message: "left/right raise".into(),
        },
        int(1),
    );
    for kind in [BinaryOp::Div, BinaryOp::Rem] {
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
