//! Typed wire-pair regressions; no source admission or preservation theorem.
#[path = "runtime_evaluation/support.rs"]
#[allow(dead_code)]
mod support;
use chialisp_oxide_ir::{Environment, Expr, Program, Type, compile, optimize};
use chialisp_oxide_vm::FailureCode;
use support::*;

fn ty(first: Type, rest: Type) -> Type {
    Type::Pair {
        first: Box::new(first),
        rest: Box::new(rest),
    }
}
fn cons(first: Expr, rest: Expr) -> Expr {
    Expr::Pair {
        first: Box::new(first),
        rest: Box::new(rest),
    }
}
fn head(value: Expr) -> Expr {
    Expr::PairFirst {
        value: Box::new(value),
    }
}
fn tail(value: Expr) -> Expr {
    Expr::PairRest {
        value: Box::new(value),
    }
}
fn fixture(result: Type, body: Expr) -> Program {
    program(Environment::Strict, vec![], result, body, vec![])
}

#[test]
fn explicit_pair_and_existing_tuple_have_distinct_exact_wire_formats() {
    let tuple_ty = Type::Tuple {
        items: vec![u8_type(), u8_type()],
    };
    let first = Expr::Tuple {
        items: vec![int(1), int(2)],
    };
    let rest = tree_atom("0000");
    let pair_program = fixture(
        ty(tuple_ty.clone(), Type::Tree),
        cons(first.clone(), rest.clone()),
    );
    let tuple_program = fixture(
        Type::Tuple {
            items: vec![tuple_ty, Type::Tree],
        },
        Expr::Tuple {
            items: vec![first, rest],
        },
    );
    let wanted_pair = pair(&list(&[uint(1), uint(2)]), &atom(&[0, 0]));
    let wanted_tuple = list(&[list(&[uint(1), uint(2)]), atom(&[0, 0])]);
    assert_ne!(wanted_pair, wanted_tuple);
    for (_, code) in modes(&pair_program) {
        accepted(run(&code, &list(&[]), FINITE_LIMIT), &wanted_pair);
    }
    for (_, code) in modes(&tuple_program) {
        accepted(run(&code, &list(&[]), FINITE_LIMIT), &wanted_tuple);
    }
    assert_eq!(
        serde_json::to_value(&pair_program).unwrap()["ir_version"],
        5
    );
    assert!(
        serde_json::to_value(&tuple_program)
            .unwrap()
            .get("ir_version")
            .is_none()
    );
}

#[test]
fn pair_abi_preserves_arbitrary_rest_but_validates_head_and_outer_arity() {
    let input_ty = ty(u8_type(), Type::Tree);
    let p = program(
        Environment::Strict,
        vec![input_ty],
        Type::Tree,
        tail(arg(0)),
        vec![],
    );
    for (_, code) in modes(&p) {
        for rest in [atom(&[]), atom(&[0, 0, 1]), pair(&uint(99), &atom(&[0xff]))] {
            accepted(
                run(&code, &list(&[pair(&uint(3), &rest)]), FINITE_LIMIT),
                &rest,
            );
        }
        for bad in [
            list(&[]),
            list(&[atom(&[])]),
            list(&[pair(&atom(&[0, 3]), &uint(4))]),
            list(&[pair(&uint(256), &uint(4))]),
            list(&[pair(&uint(3), &uint(4)), atom(&[])]),
        ] {
            abi_rejected(run(&code, &bad, FINITE_LIMIT));
        }
    }
}

#[test]
fn strict_pair_operands_and_unused_siblings_survive_public_transport() {
    for (body, expected) in [
        (
            cons(reject(&u8_type()), call(0, vec![])),
            FailureCode::Raise,
        ),
        (
            cons(call(0, vec![]), reject(&u8_type())),
            FailureCode::CostExceeded,
        ),
        (cons(int(7), call(0, vec![])), FailureCode::CostExceeded),
    ] {
        let p = program(
            Environment::Strict,
            vec![],
            u8_type(),
            head(body),
            vec![spin(&u8_type())],
        );
        for candidate in [p.clone(), optimize(&p).unwrap()] {
            let wire = serde_json::to_vec(&candidate).unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&wire).unwrap()["ir_version"],
                5
            );
            let reread: Program = serde_json::from_slice(&wire).unwrap();
            assert!(reread.uses_runtime() && reread.uses_pairs());
            for (_, code) in modes(&reread) {
                failed(run(&code, &list(&[]), LIMIT), expected);
            }
        }
    }
}

#[test]
fn discarded_hash_overflow_and_pair_rest_projections_still_execute() {
    let bad_hash = Expr::Sha256 {
        parts: vec![Expr::TreeAtomValue {
            value: Box::new(Expr::TreePair {
                first: Box::new(tree_atom("")),
                rest: Box::new(tree_atom("")),
            }),
        }],
    };
    let cases = [bad_hash, add(int(255), int(1))];
    for rest in cases {
        let p = fixture(u8_type(), head(cons(int(7), rest)));
        for (_, code) in modes(&p) {
            abi_rejected(run(&code, &list(&[]), FINITE_LIMIT));
        }
    }
    let p = fixture(u8_type(), tail(cons(reject(&u8_type()), int(9))));
    for (_, code) in modes(&p) {
        failed(run(&code, &list(&[]), FINITE_LIMIT), FailureCode::Raise);
    }
}

#[test]
fn pair_capability_in_signatures_unused_bodies_and_bindings_requires_v5() {
    let pair_ty = ty(u8_type(), Type::Tree);
    let variants = [
        fixture(
            pair_ty.clone(),
            Expr::Reject {
                message: "bottom".into(),
            },
        ),
        program(
            Environment::Strict,
            vec![],
            u8_type(),
            int(7),
            vec![function(0, vec![pair_ty.clone()], u8_type(), int(8))],
        ),
        fixture(
            u8_type(),
            binding(0, pair_ty.clone(), cons(int(1), tree_atom("")), int(7)),
        ),
    ];
    for (index, p) in variants.into_iter().enumerate() {
        p.validate().unwrap();
        assert!(p.uses_pairs());
        let value = serde_json::to_value(&p).unwrap();
        assert_eq!(value["ir_version"], 5);
        for version in [1, 2, 3, 4] {
            let mut bad = value.clone();
            bad["ir_version"] = version.into();
            assert!(serde_json::from_value::<Program>(bad).is_err());
        }
        let mut legacy = value["program"].clone();
        legacy.as_object_mut().unwrap().remove("functions");
        if index == 1 {
            // Removing the unused table also removes the only Pair capability.
            assert!(
                !serde_json::from_value::<Program>(legacy)
                    .unwrap()
                    .uses_pairs()
            );
        } else {
            assert!(serde_json::from_value::<Program>(legacy).is_err());
        }
    }
    let p = program(
        Environment::Strict,
        vec![],
        u8_type(),
        int(7),
        vec![function(0, vec![], u8_type(), int(8))],
    );
    let mut value = serde_json::to_value(&p).unwrap();
    assert_eq!(value["ir_version"], 4);
    value["ir_version"] = 5.into();
    assert!(serde_json::from_value::<Program>(value).is_err());
}

#[test]
fn whole_validation_precedes_pair_route_and_no_optional_pass_hides_bad_types() {
    let mut p = fixture(
        u8_type(),
        branch(Expr::Bool { value: false }, head(tree_atom("")), int(7)),
    );
    for optimize in [false, true] {
        assert!(
            compile(&p, optimize)
                .unwrap_err()
                .to_string()
                .contains("typed pair projection requires a pair")
        );
    }
    p.body = int(7);
    p.functions.push(function(
        0,
        vec![],
        ty(Type::Bytes { len: 4097 }, Type::Tree),
        Expr::Reject {
            message: "unused".into(),
        },
    ));
    for optimize in [false, true] {
        assert!(
            compile(&p, optimize)
                .unwrap_err()
                .to_string()
                .contains("byte array too large")
        );
    }
    assert!(serde_json::to_vec(&p).is_err());
    let mut t = Type::Unit;
    for _ in 0..32 {
        t = ty(u8_type(), t);
    }
    let deep = fixture(
        t,
        Expr::Reject {
            message: "deep".into(),
        },
    );
    assert!(
        deep.validate()
            .unwrap_err()
            .to_string()
            .contains("type nesting limit")
    );
}

#[test]
fn pair_capability_cannot_bypass_whole_ir_or_pre_copy_budgets() {
    let wide_type = Type::Tuple {
        items: vec![
            ty(
                Type::Tuple {
                    items: vec![Type::Atom; 256]
                },
                Type::Unit
            );
            256
        ],
    };
    let invalid = fixture(
        wide_type,
        Expr::Reject {
            message: "unused giant type".into(),
        },
    );
    assert!(invalid.uses_pairs());
    for optimize in [false, true] {
        assert!(
            compile(&invalid, optimize)
                .unwrap_err()
                .to_string()
                .contains("type node budget")
        );
    }
    let group_type = Type::Tuple {
        items: vec![Type::Atom; 256],
    };
    let group = Expr::Tuple {
        items: vec![
            Expr::Atom {
                hex: "aa".repeat(256)
            };
            256
        ],
    };
    let p = fixture(
        ty(
            Type::Tuple {
                items: vec![group_type; 64],
            },
            Type::Unit,
        ),
        cons(
            Expr::Tuple {
                items: vec![group; 64],
            },
            Expr::Unit,
        ),
    );
    p.validate().unwrap();
    assert!(
        optimize(&p)
            .unwrap_err()
            .to_string()
            .contains("copy budget exceeded")
    );
    assert!(
        compile(&p, true)
            .unwrap_err()
            .to_string()
            .contains("copy budget exceeded")
    );
}

#[test]
fn pair_capable_deep_unary_public_serialization_keeps_the_existing_stack_and_depth_bounds() {
    let mut body = arg(0);
    for _ in 0..193 {
        body = Expr::Not {
            value: Box::new(body),
        };
    }
    let p = program(
        Environment::Strict,
        vec![Type::Bool],
        Type::Bool,
        body,
        vec![function(
            0,
            vec![ty(Type::Bool, Type::Unit)],
            Type::Unit,
            Expr::Unit,
        )],
    );
    p.validate().unwrap();
    let reduced = optimize(&p).unwrap();
    assert_eq!(
        serde_json::to_value(&reduced).unwrap(),
        serde_json::to_value(&p).unwrap()
    );
    let serialized = serde_json::to_vec(&reduced).unwrap();
    assert!(serialized.starts_with(b"{\"ir_version\":5,"));
    assert!(
        chialisp_oxide_ir::compile(&p, true)
            .unwrap_err()
            .to_string()
            .contains("runtime expression depth budget exceeded")
    );
}

#[test]
fn pair_validation_keeps_existing_depth_limits_and_checks_a_bottom_sibling() {
    for (layers, valid) in [(127, true), (128, false)] {
        let mut body = Expr::Bool { value: true };
        for _ in 0..layers {
            body = head(cons(body, Expr::Bool { value: false }));
        }
        let p = fixture(Type::Bool, body);
        assert!(p.uses_pairs());
        if valid {
            p.validate().unwrap();
        } else {
            assert!(
                p.validate()
                    .unwrap_err()
                    .to_string()
                    .contains("expression nesting limit")
            );
        }
    }
    let p = fixture(
        ty(Type::Bool, Type::Bool),
        cons(
            Expr::Reject {
                message: "first does not return".into(),
            },
            head(Expr::Bool { value: true }),
        ),
    );
    assert!(
        p.validate()
            .unwrap_err()
            .to_string()
            .contains("typed pair projection requires a pair")
    );
}
