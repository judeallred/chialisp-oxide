use chialisp_oxide_ir::{BinaryOp, Expr, Parameter, Program, Type, compile};
use clvmr::{
    Allocator, ChiaDialect, MEMPOOL_MODE, run_program,
    serde::{node_from_bytes, node_to_bytes},
};
fn int_type() -> Type {
    Type::Int {
        bits: 8,
        signed: false,
    }
}
fn number(n: i64) -> Expr {
    Expr::Int {
        value: n.to_string(),
        ty: int_type(),
    }
}
fn program(body: Expr, ty: Type, params: Vec<Type>) -> Program {
    Program {
        environment: chialisp_oxide_ir::Environment::Strict,
        name: "test".into(),
        parameters: params
            .into_iter()
            .enumerate()
            .map(|(i, ty)| Parameter {
                name: format!("arg{i}"),
                ty,
            })
            .collect(),
        result: ty,
        body,
    }
}
fn run(program: &Program, solution: &str, opt: bool) -> Option<String> {
    let bytes = compile(program, opt).unwrap();
    let mut a = Allocator::new();
    let ptr = node_from_bytes(&mut a, &bytes).unwrap();
    let env = rue_clvm::assemble(&mut a, solution).unwrap();
    run_program(
        &mut a,
        &ChiaDialect::new(MEMPOOL_MODE),
        ptr,
        env,
        10_000_000,
    )
    .ok()
    .map(|v| hex::encode(node_to_bytes(&a, v.1).unwrap()))
}
fn expected(value: &str) -> String {
    let mut a = Allocator::new();
    let ptr = rue_clvm::assemble(&mut a, value).unwrap();
    hex::encode(node_to_bytes(&a, ptr).unwrap())
}
#[test]
fn ignored_environment_is_explicit_and_preserves_effects() {
    use chialisp_oxide_ir::Environment;
    let mut closed = program(number(7), int_type(), vec![]);
    for opt in [false, true] {
        assert_eq!(run(&closed, "1", opt), None);
    }
    closed.environment = Environment::Ignored;
    for opt in [false, true] {
        for input in ["()", "1", "0x0001", "(1 2)", "(1 . 2)", "((1) . (2 . 3))"] {
            assert_eq!(run(&closed, input, opt), Some(expected("7")));
        }
        let mut fallible = closed.clone();
        fallible.body = Expr::Sequence {
            first: Box::new(Expr::Reject {
                message: "must fail".into(),
            }),
            then_value: Box::new(number(7)),
        };
        assert_eq!(run(&fallible, "(1 . 2)", opt), None);
        fallible.body = Expr::Binary {
            kind: BinaryOp::Add,
            left: Box::new(number(255)),
            right: Box::new(number(1)),
            ty: int_type(),
        };
        assert_eq!(run(&fallible, "1", opt), None);
        fallible.body = Expr::If {
            condition: Box::new(Expr::Bool { value: false }),
            then_value: Box::new(Expr::Reject {
                message: "unselected".into(),
            }),
            else_value: Box::new(number(7)),
        };
        assert_eq!(run(&fallible, "1", opt), Some(expected("7")));
    }
    closed.parameters.push(Parameter {
        name: "unused".into(),
        ty: Type::Unit,
    });
    assert!(
        closed
            .validate()
            .unwrap_err()
            .to_string()
            .contains("zero parameters")
    );
    assert!(compile(&closed, true).is_err());
    closed.parameters.clear();
    closed.body = Expr::Arg { index: 0 };
    assert!(closed.validate().is_err());
}
#[test]
fn identity_multiplication_compiles() {
    let p = program(
        Expr::Binary {
            kind: BinaryOp::Mul,
            left: Box::new(number(1)),
            right: Box::new(number(1)),
            ty: int_type(),
        },
        int_type(),
        vec![],
    );
    for opt in [false, true] {
        assert_eq!(run(&p, "()", opt), Some(expected("1")));
    }
}
#[test]
fn scalar_abi_checks_canonical_encodings_and_shape() {
    let p = program(Expr::Arg { index: 0 }, int_type(), vec![int_type()]);
    for opt in [false, true] {
        for i in 0..=255 {
            assert_eq!(
                run(&p, &format!("({i})"), opt),
                Some(expected(&i.to_string()))
            );
        }
        for input in [
            "()",
            "(1 2)",
            "1",
            "((1))",
            "(-1)",
            "(256)",
            "(0x00)",
            "(0x0001)",
            "(0x007f)",
            "(0x000080)",
            "(1 . 2)",
        ] {
            assert_eq!(run(&p, input, opt), None, "{input} opt={opt}");
        }
    }
}
#[test]
fn signed_byte_boundary_is_exact() {
    let ty = Type::Int {
        bits: 8,
        signed: true,
    };
    let p = program(Expr::Arg { index: 0 }, ty.clone(), vec![ty]);
    for opt in [false, true] {
        for i in -128..=127 {
            assert_eq!(
                run(&p, &format!("({i})"), opt),
                Some(expected(&i.to_string()))
            );
        }
        for v in ["(-129)", "(128)", "(0xffff)", "(0xff80)", "(0x0080)"] {
            assert_eq!(run(&p, v, opt), None, "{v}");
        }
    }
}
#[test]
fn checked_overflow_and_underflow_reject() {
    for (op, left, right) in [
        (BinaryOp::Add, 255, 1),
        (BinaryOp::Sub, 0, 1),
        (BinaryOp::Mul, 128, 2),
    ] {
        let p = program(
            Expr::Binary {
                kind: op,
                left: Box::new(number(left)),
                right: Box::new(number(right)),
                ty: int_type(),
            },
            int_type(),
            vec![],
        );
        for opt in [false, true] {
            assert_eq!(run(&p, "()", opt), None);
        }
    }
}
#[test]
fn unused_failure_and_lazy_branches_are_preserved() {
    let p = program(
        Expr::Sequence {
            first: Box::new(Expr::Reject {
                message: "unused failure".into(),
            }),
            then_value: Box::new(number(42)),
        },
        int_type(),
        vec![],
    );
    for opt in [false, true] {
        assert_eq!(run(&p, "()", opt), None);
    }
    let p = program(
        Expr::If {
            condition: Box::new(Expr::Arg { index: 0 }),
            then_value: Box::new(number(42)),
            else_value: Box::new(Expr::Reject {
                message: "false branch".into(),
            }),
        },
        int_type(),
        vec![Type::Bool],
    );
    for opt in [false, true] {
        assert_eq!(run(&p, "(1)", opt), Some(expected("42")));
        assert_eq!(run(&p, "(())", opt), None);
        assert_eq!(run(&p, "(2)", opt), None);
    }
}
#[test]
fn fixed_byte_projection_canonicalizes_all_bytes() {
    let p = program(
        Expr::Field {
            value: Box::new(Expr::Arg { index: 0 }),
            index: 0,
        },
        int_type(),
        vec![Type::Bytes { len: 1 }],
    );
    for opt in [false, true] {
        for byte in 0..=255 {
            assert_eq!(
                run(&p, &format!("(0x{byte:02x})"), opt),
                Some(expected(&byte.to_string())),
                "{byte} {opt}"
            );
        }
    }
}

#[test]
fn checked_arithmetic_expands_linearly_and_large_programs_have_a_budget() {
    let mut body = Expr::Arg { index: 0 };
    for _ in 0..8 {
        body = Expr::Binary {
            kind: BinaryOp::Add,
            left: Box::new(body),
            right: Box::new(number(1)),
            ty: int_type(),
        };
    }
    let p = program(body.clone(), int_type(), vec![int_type()]);
    for opt in [false, true] {
        assert_eq!(run(&p, "(1)", opt), Some(expected("9")));
        assert!(compile(&p, opt).unwrap().len() < 10_000);
    }
    // An attacker-supplied large IR must fail before recursive CLVM serialization.
    let p = program(
        Expr::Tuple {
            items: vec![body; 256],
        },
        Type::Tuple {
            items: vec![int_type(); 256],
        },
        vec![int_type()],
    );
    let error = compile(&p, false).unwrap_err().to_string();
    assert!(
        error.contains("budget") || error.contains("nesting limit"),
        "{error}"
    );
}
