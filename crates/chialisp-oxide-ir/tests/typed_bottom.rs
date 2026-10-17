//! Reject has no returned type. Explicit binary/Boolean contexts can ascribe
//! it without skipping sibling validation or strict runtime evaluation.
#[path = "runtime_evaluation/support.rs"]
#[allow(
    dead_code,
    reason = "Shared runtime fixtures also cover other operations."
)]
mod support;
use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Type, compile};
use chialisp_oxide_vm::FailureCode;
use support::*;

fn bottom() -> Expr {
    Expr::Reject {
        message: "direct rejecting operand".into(),
    }
}
fn binary(kind: BinaryOp, ty: &Type, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        kind,
        ty: ty.clone(),
        left: Box::new(left),
        right: Box::new(right),
    }
}
fn not(value: Expr) -> Expr {
    Expr::Not {
        value: Box::new(value),
    }
}

#[test]
fn explicit_binary_types_and_not_ascribe_direct_bottom_and_still_raise_in_both_modes() {
    for (ty, value, operations) in [
        (
            Type::Bool,
            Expr::Bool { value: false },
            vec![
                BinaryOp::Eq,
                BinaryOp::Ne,
                BinaryOp::Bitand,
                BinaryOp::Bitor,
                BinaryOp::Bitxor,
            ],
        ),
        (
            u8_type(),
            int(1),
            vec![
                BinaryOp::Eq,
                BinaryOp::Ne,
                BinaryOp::Lt,
                BinaryOp::Le,
                BinaryOp::Gt,
                BinaryOp::Ge,
                BinaryOp::Add,
                BinaryOp::Sub,
                BinaryOp::Mul,
                BinaryOp::Bitand,
                BinaryOp::Bitor,
                BinaryOp::Bitxor,
            ],
        ),
        (
            Type::Int {
                bits: 128,
                signed: true,
            },
            Expr::Int {
                value: "-1".into(),
                ty: Type::Int {
                    bits: 128,
                    signed: true,
                },
            },
            vec![
                BinaryOp::Add,
                BinaryOp::Sub,
                BinaryOp::Mul,
                BinaryOp::Bitand,
                BinaryOp::Bitor,
                BinaryOp::Bitxor,
            ],
        ),
        (
            Type::Atom,
            Expr::Atom { hex: "00".into() },
            vec![
                BinaryOp::Eq,
                BinaryOp::Ne,
                BinaryOp::Lt,
                BinaryOp::Le,
                BinaryOp::Gt,
                BinaryOp::Ge,
            ],
        ),
        (
            Type::Bytes { len: 1 },
            Expr::Bytes { hex: "00".into() },
            vec![BinaryOp::Eq, BinaryOp::Ne],
        ),
    ] {
        for op in operations {
            let result = if matches!(
                op,
                BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
            ) {
                Type::Bool
            } else {
                ty.clone()
            };
            for (left, right) in [
                (bottom(), value.clone()),
                (value.clone(), bottom()),
                (bottom(), bottom()),
            ] {
                for runtime in [false, true] {
                    let functions = if runtime {
                        vec![function(0, vec![], Type::Unit, Expr::Unit)]
                    } else {
                        vec![]
                    };
                    let p = program(
                        Environment::Ignored,
                        vec![],
                        result.clone(),
                        binary(op, &ty, left.clone(), right.clone()),
                        functions,
                    );
                    p.validate().unwrap();
                    assert_eq!(p.type_of(&p.body).unwrap(), Some(result.clone()));
                    for (_, code) in modes(&p) {
                        failed(run(&code, &atom(&[]), LIMIT), FailureCode::Raise);
                    }
                }
            }
        }
    }
    let p = program(
        Environment::Ignored,
        vec![],
        Type::Bool,
        not(bottom()),
        vec![],
    );
    assert_eq!(p.type_of(&p.body).unwrap(), Some(Type::Bool));
    for (_, code) in modes(&p) {
        failed(run(&code, &atom(&[]), LIMIT), FailureCode::Raise);
    }
}

#[test]
fn direct_bottom_remains_strict_and_lazy_good_branches_do_not_evaluate_it() {
    for op in [BinaryOp::Bitand, BinaryOp::Bitor, BinaryOp::Bitxor] {
        let mask = Expr::Bool {
            value: op == BinaryOp::Bitor,
        };
        let rejecting = binary(op, &Type::Bool, mask, bottom());
        for runtime in [false, true] {
            let functions = if runtime {
                vec![function(0, vec![], Type::Unit, Expr::Unit)]
            } else {
                vec![]
            };
            let p = program(
                Environment::Strict,
                vec![Type::Bool],
                Type::Bool,
                branch(arg(0), rejecting.clone(), Expr::Bool { value: true }),
                functions,
            );
            for (_, code) in modes(&p) {
                accepted(run(&code, &list(&[uint(0)]), LIMIT), &uint(1));
                failed(run(&code, &list(&[uint(1)]), LIMIT), FailureCode::Raise);
            }
        }
        for (left, right, wanted) in [
            (bottom(), call(0, vec![]), FailureCode::Raise),
            (call(0, vec![]), bottom(), FailureCode::CostExceeded),
        ] {
            let p = program(
                Environment::Ignored,
                vec![],
                Type::Bool,
                binding(
                    0,
                    Type::Bool,
                    binary(op, &Type::Bool, left, right),
                    Expr::Bool { value: true },
                ),
                vec![spin(&Type::Bool)],
            );
            for (_, code) in modes(&p) {
                failed(run(&code, &atom(&[]), LIMIT), wanted);
            }
        }
    }
}

#[test]
fn bottom_does_not_hide_invalid_siblings_types_operations_or_unascribed_aggregates() {
    for invalid in [
        binary(BinaryOp::Bitand, &Type::Bool, bottom(), int(0)),
        binary(BinaryOp::Eq, &Type::Bool, Expr::Unit, bottom()),
        binary(BinaryOp::Bitand, &Type::Bool, bottom(), arg(9)),
        binary(BinaryOp::Bitand, &Type::Atom, bottom(), bottom()),
        binary(BinaryOp::Eq, &Type::Tree, bottom(), bottom()),
        binary(
            BinaryOp::Bitxor,
            &Type::Int {
                bits: 7,
                signed: false,
            },
            bottom(),
            bottom(),
        ),
        binary(BinaryOp::Div, &u8_type(), bottom(), bottom()),
        binary(BinaryOp::Shr, &u8_type(), bottom(), bottom()),
        not(int(0)),
        Expr::Tuple {
            items: vec![bottom()],
        },
        field(bottom(), 0),
    ] {
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
        assert!(
            p.validate().is_err(),
            "bottom must not excuse invalid unreachable IR"
        );
        for optimize in [false, true] {
            assert!(compile(&p, optimize).is_err());
        }
    }
}
