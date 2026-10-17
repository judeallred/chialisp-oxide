//! Optional optimization must not overflow the ordinary test-thread stack.
use chialisp_oxide_ir::{Environment, Expr, Function, Program, Type, compile, optimize};

fn unary_nodes(nodes: usize) -> Expr {
    assert!(nodes > 0);
    (1..nodes).fold(Expr::Bool { value: false }, |value, _| Expr::Not {
        value: Box::new(value),
    })
}

fn runtime(body: Expr) -> Program {
    Program {
        name: "optional-optimizer-depth".into(),
        environment: Environment::Strict,
        parameters: vec![],
        result: Type::Bool,
        body,
        // The unused table deliberately keeps the original runtime route.
        functions: vec![Function {
            id: 0,
            name: "unused".into(),
            parameters: vec![],
            result: Type::Bool,
            body: Expr::Bool { value: false },
        }],
    }
}

fn wire(program: &Program) -> Vec<u8> {
    serde_json::to_vec(program).unwrap()
}

#[test]
fn recursive_refinement_has_an_inclusive_sixty_four_node_depth_boundary() {
    let allowed = runtime(unary_nodes(64));
    allowed.validate().unwrap();
    let reduced = optimize(&allowed).unwrap();
    assert!(matches!(reduced.body, Expr::Bool { value: true }));
    assert_ne!(wire(&reduced), wire(&allowed));

    let deeper = runtime(unary_nodes(65));
    deeper.validate().unwrap();
    assert_eq!(wire(&optimize(&deeper).unwrap()), wire(&deeper));
}

#[test]
fn deep_runtime_input_reaches_the_existing_backend_error_without_host_overflow() {
    let source = runtime(unary_nodes(193));
    source.validate().unwrap();
    let original = wire(&source);
    assert_eq!(wire(&optimize(&source).unwrap()), original);
    assert_eq!(wire(&source), original, "optimizer mutated its input");
    let error = compile(&source, true).unwrap_err().to_string();
    assert!(
        error.contains("runtime expression depth budget"),
        "expected the existing controlled backend limit, got {error:?}"
    );
}

#[test]
fn an_unused_deep_function_also_skips_optional_recursive_refinement() {
    let mut source = runtime(unary_nodes(4));
    source.functions[0].body = unary_nodes(65);
    source.validate().unwrap();
    assert_eq!(wire(&optimize(&source).unwrap()), wire(&source));
}

#[test]
fn deep_fallback_never_skips_validation_of_a_dead_invalid_argument() {
    let mut source = runtime(Expr::If {
        condition: Box::new(Expr::Bool { value: true }),
        then_value: Box::new(Expr::Bool { value: false }),
        else_value: Box::new(Expr::Arg { index: 64 }),
    });
    source.functions[0].body = unary_nodes(65);
    let error = optimize(&source).unwrap_err().to_string();
    assert!(error.contains("invalid argument"), "{error}");
}
