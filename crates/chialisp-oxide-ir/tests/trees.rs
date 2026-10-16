use chialisp_oxide_ir::{Environment, Expr, Parameter, Program, Type, compile};
use clvmr::{
    Allocator, ChiaDialect, MEMPOOL_MODE, run_program,
    serde::{node_from_bytes, node_to_bytes},
};
use serde_json::json;

fn arg() -> Box<Expr> {
    Box::new(Expr::Arg { index: 0 })
}
fn atom(hex: &str) -> Expr {
    Expr::Atom { hex: hex.into() }
}
fn tree_atom(hex: &str) -> Expr {
    Expr::TreeAtom {
        value: Box::new(atom(hex)),
    }
}
fn reject() -> Box<Expr> {
    Box::new(Expr::Reject {
        message: "required failure".into(),
    })
}
fn program(body: Expr, result: Type) -> Program {
    Program {
        environment: Environment::Raw,
        name: "tree".into(),
        parameters: vec![Parameter {
            name: "environment".into(),
            ty: Type::Tree,
        }],
        result,
        body,
    }
}
fn run(p: &Program, input: &str, opt: bool) -> Option<String> {
    let mut a = Allocator::new();
    let code = node_from_bytes(&mut a, &compile(p, opt).unwrap()).unwrap();
    let env = rue_clvm::assemble(&mut a, input).unwrap();
    run_program(
        &mut a,
        &ChiaDialect::new(MEMPOOL_MODE),
        code,
        env,
        100_000_000,
    )
    .ok()
    .map(|v| hex::encode(node_to_bytes(&a, v.1).unwrap()))
}
fn serialized(input: &str) -> String {
    let mut a = Allocator::new();
    let n = rue_clvm::assemble(&mut a, input).unwrap();
    hex::encode(node_to_bytes(&a, n).unwrap())
}

#[test]
fn raw_policy_consumes_the_whole_tree_and_has_a_fail_closed_version() {
    let p = program(*arg(), Type::Tree);
    let wire = serde_json::to_value(&p).unwrap();
    assert_eq!(wire["ir_version"], 3);
    assert_eq!(wire["environment"], "raw");
    assert!(wire.get("name").is_none()); // A permissive old reader still needs name.
    let roundtrip: Program = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(roundtrip).unwrap(), wire);
    for (key, value) in [
        ("ir_version", json!(2)),
        ("environment", json!("strict")),
        ("environment", json!("ignored")),
    ] {
        let mut bad = wire.clone();
        bad[key] = value;
        assert!(serde_json::from_value::<Program>(bad).is_err());
    }
    for params in [
        json!([]),
        json!([{"name":"x","ty":{"kind":"atom"}}]),
        json!([{"name":"x","ty":{"kind":"tree"}},{"name":"y","ty":{"kind":"tree"}}]),
    ] {
        let mut bad = wire.clone();
        bad["program"]["parameters"] = params;
        assert!(serde_json::from_value::<Program>(bad).is_err());
    }
    for opt in [false, true] {
        for input in [
            "()",
            "1",
            "0x0001",
            "(1 2)",
            "(0x00 . 0x0000)",
            "((1 . 2) . (3 . 4))",
        ] {
            assert_eq!(run(&p, input, opt), Some(serialized(input)));
        }
    }
}

#[test]
fn atom_and_tree_types_preserve_bytes_while_strict_arguments_keep_their_shape() {
    for ty in [Type::Tree, Type::Atom] {
        let mut p = program(*arg(), ty.clone());
        p.environment = Environment::Strict;
        p.parameters[0].ty = ty.clone();
        for opt in [false, true] {
            for input in ["()", "0x00", "0x0001", "0xffff", "0x80"] {
                assert_eq!(run(&p, &format!("({input})"), opt), Some(serialized(input)));
            }
            for input in ["()", "1", "(1 2)", "(1 . 2)"] {
                assert_eq!(run(&p, input, opt), None);
            }
            assert_eq!(
                run(&p, "((1 . 2))", opt),
                if ty == Type::Tree {
                    Some(serialized("(1 . 2)"))
                } else {
                    None
                }
            );
        }
    }
}

#[test]
fn projections_are_checked_and_unselected_projections_remain_lazy() {
    for opt in [false, true] {
        let first = program(Expr::TreeFirst { value: arg() }, Type::Tree);
        let rest = program(Expr::TreeRest { value: arg() }, Type::Tree);
        let bytes = program(Expr::TreeAtomValue { value: arg() }, Type::Atom);
        for input in ["()", "1", "0x0001"] {
            assert_eq!(run(&first, input, opt), None);
            assert_eq!(run(&rest, input, opt), None);
            assert_eq!(run(&bytes, input, opt), Some(serialized(input)));
        }
        assert_eq!(
            run(&first, "(0x0001 . 0x00)", opt),
            Some(serialized("0x0001"))
        );
        assert_eq!(run(&rest, "(0x0001 . 0x00)", opt), Some(serialized("0x00")));
        assert_eq!(run(&bytes, "(1 . 2)", opt), None);
        let lazy = program(
            Expr::If {
                condition: Box::new(Expr::TreeIsPair { value: arg() }),
                then_value: Box::new(Expr::TreeFirst { value: arg() }),
                else_value: Box::new(tree_atom("0001")),
            },
            Type::Tree,
        );
        assert_eq!(run(&lazy, "()", opt), Some(serialized("0x0001")));
        assert_eq!(
            run(&lazy, "((2 . 3) . 4)", opt),
            Some(serialized("(2 . 3)"))
        );
    }
}

#[test]
fn tree_operations_preserve_required_failures_even_when_values_are_discarded() {
    for body in [
        Expr::TreePair {
            first: reject(),
            rest: Box::new(tree_atom("")),
        },
        Expr::TreePair {
            first: Box::new(tree_atom("")),
            rest: reject(),
        },
        Expr::TreeIsPair { value: reject() },
        Expr::TreeAtomValue { value: reject() },
        Expr::TreeAtom { value: reject() },
        Expr::TreeFirst { value: reject() },
        Expr::TreeRest { value: reject() },
        Expr::TreeFirst {
            value: Box::new(Expr::TreePair {
                first: Box::new(tree_atom("")),
                rest: reject(),
            }),
        },
    ] {
        let p = program(
            Expr::Sequence {
                first: Box::new(body),
                then_value: Box::new(tree_atom("01")),
            },
            Type::Tree,
        );
        for opt in [false, true] {
            assert_eq!(run(&p, "(1 . 2)", opt), None, "{p:?} opt={opt}");
        }
    }
}

#[test]
fn reconstructing_a_pair_must_not_turn_checked_projections_into_identity() {
    let p = program(
        Expr::TreePair {
            first: Box::new(Expr::TreeFirst { value: arg() }),
            rest: Box::new(Expr::TreeRest { value: arg() }),
        },
        Type::Tree,
    );
    for opt in [false, true] {
        for input in ["()", "1", "0x0001"] {
            assert_eq!(run(&p, input, opt), None, "{input} opt={opt}");
        }
        for input in ["(1 . 2)", "((0x00 . 0x0001) . ())"] {
            assert_eq!(run(&p, input, opt), Some(serialized(input)));
        }
    }
}

#[test]
fn tree_constructors_are_typed_and_literal_budgets_are_enforced() {
    for body in [
        Expr::TreePair {
            first: Box::new(atom("01")),
            rest: arg(),
        },
        Expr::TreeFirst {
            value: Box::new(atom("01")),
        },
        Expr::TreeAtom { value: arg() },
        Expr::TreeAtom {
            value: Box::new(atom("xyz")),
        },
        Expr::TreeAtom {
            value: Box::new(atom(&"ff".repeat(4097))),
        },
    ] {
        assert!(program(body, Type::Tree).validate().is_err());
    }
    let p = program(
        Expr::TreePair {
            first: Box::new(tree_atom("01")),
            rest: arg(),
        },
        Type::Tree,
    );
    for opt in [false, true] {
        assert_eq!(
            run(&p, "(0x0000 . 3)", opt),
            Some(serialized("(1 . (0x0000 . 3))"))
        );
    }
}
