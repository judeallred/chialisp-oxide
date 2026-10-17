//! Typed runtime structure checks; these establish no termination property.
use chialisp_oxide_ir::{Environment, Expr, Function, Parameter, Program, Type, compile};

fn parameter(ty: Type) -> Parameter {
    Parameter {
        name: "value".into(),
        ty,
    }
}
fn function(id: usize, body: Expr) -> Function {
    Function {
        id,
        name: format!("helper-{id}"),
        parameters: vec![],
        result: Type::Unit,
        body,
    }
}
fn program(body: Expr) -> Program {
    Program {
        name: "runtime-validation".into(),
        environment: Environment::Strict,
        parameters: vec![],
        result: Type::Unit,
        body,
        functions: vec![],
    }
}
fn call(id: usize, arguments: Vec<Expr>) -> Expr {
    Expr::Call {
        function_id: id,
        arguments,
    }
}
fn local(id: usize) -> Expr {
    Expr::Local { local_id: id }
}
fn bind(id: usize, ty: Type, value: Expr, body: Expr) -> Expr {
    Expr::Let {
        local_id: id,
        ty,
        value: Box::new(value),
        body: Box::new(body),
    }
}
fn reject() -> Expr {
    Expr::Reject {
        message: "deliberate reject".into(),
    }
}
fn sequence(first: Expr, then_value: Expr) -> Expr {
    Expr::Sequence {
        first: Box::new(first),
        then_value: Box::new(then_value),
    }
}
fn invalid(p: &Program, diagnostic: &str) {
    let error = p.validate().unwrap_err().to_string();
    assert!(
        error.contains(diagnostic),
        "expected {diagnostic:?}, got {error:?}"
    );
}

#[test]
fn finite_recursive_tables_are_typed_without_unrolling_or_proving_termination() {
    let mut p = program(call(0, vec![]));
    p.functions = vec![function(0, call(1, vec![])), function(1, call(0, vec![]))];
    p.validate().unwrap();
    assert!(p.uses_runtime());
    assert_eq!(p.type_of(&p.body).unwrap(), Some(Type::Unit));
    // Dense IDs are table identities; duplicate diagnostic names are harmless.
    p.functions[1].name = p.functions[0].name.clone();
    p.validate().unwrap();
}

#[test]
fn calls_require_dense_ids_and_exact_argument_types_and_arity() {
    let mut p = program(call(0, vec![]));
    invalid(&p, "invalid function");
    p.functions = vec![function(1, Expr::Unit)];
    invalid(&p, "dense table indices");
    p.functions = vec![function(0, Expr::Unit), function(0, Expr::Unit)];
    invalid(&p, "dense table indices");
    p.functions = vec![function(0, Expr::Unit)];
    p.body = call(0, vec![Expr::Unit]);
    invalid(&p, "argument count");
    p.functions[0].parameters = vec![parameter(Type::Bool)];
    invalid(&p, "argument type");
    p.body = call(0, vec![Expr::Bool { value: true }]);
    p.validate().unwrap();
    p.body = call(0, vec![reject()]);
    p.validate().unwrap();
}

#[test]
fn every_function_is_checked_in_its_own_argument_and_local_scope() {
    let mut p = program(bind(0, Type::Unit, Expr::Unit, local(0)));
    p.parameters = vec![parameter(Type::Tree)];
    p.functions = vec![function(0, Expr::Arg { index: 0 })];
    invalid(&p, "invalid argument");
    p.functions[0].body = local(0);
    invalid(&p, "unbound local");
    p.functions[0].body = Expr::Bool { value: true };
    invalid(&p, "body/result type mismatch");
    p.functions[0].parameters = vec![parameter(Type::Bool)];
    p.functions[0].result = Type::Bool;
    p.functions[0].body = Expr::Arg { index: 0 };
    p.validate().unwrap();
}

#[test]
fn lets_bind_once_in_lexical_scope_and_sibling_scopes_can_reuse_ids() {
    let mut p = program(bind(7, Type::Unit, Expr::Unit, local(7)));
    p.validate().unwrap();
    p.body = bind(7, Type::Unit, local(7), Expr::Unit);
    invalid(&p, "unbound local");
    p.body = bind(
        7,
        Type::Unit,
        Expr::Unit,
        bind(7, Type::Unit, Expr::Unit, local(7)),
    );
    invalid(&p, "duplicate active local");
    p.body = sequence(bind(7, Type::Unit, Expr::Unit, local(7)), local(7));
    invalid(&p, "unbound local");
    p.body = sequence(
        bind(7, Type::Unit, Expr::Unit, local(7)),
        bind(7, Type::Unit, Expr::Unit, local(7)),
    );
    p.validate().unwrap();
    p.body = bind(
        7,
        Type::Unit,
        Expr::Unit,
        bind(8, Type::Unit, local(7), local(8)),
    );
    p.validate().unwrap();
    p.body = bind(7, Type::Bool, Expr::Unit, local(7));
    invalid(&p, "initializer type mismatch");
}

#[test]
fn unused_and_unreachable_bindings_and_arguments_are_not_validation_exemptions() {
    let mut p = program(bind(0, Type::Unit, reject(), local(0)));
    p.validate().unwrap();
    p.body = bind(0, Type::Unit, reject(), local(1));
    invalid(&p, "unbound local");
    p.body = bind(0, Type::Unit, call(9, vec![]), Expr::Unit);
    invalid(&p, "invalid function");
    p.body = Expr::If {
        condition: Box::new(Expr::Bool { value: true }),
        then_value: Box::new(Expr::Unit),
        else_value: Box::new(call(9, vec![])),
    };
    invalid(&p, "invalid function");
    p.functions = vec![function(0, Expr::Unit)];
    p.functions[0].parameters = vec![parameter(Type::Unit), parameter(Type::Unit)];
    p.body = call(0, vec![reject(), local(9)]);
    invalid(&p, "unbound local");
}

#[test]
fn scoped_type_queries_use_the_supplied_function_scope() {
    let p = program(Expr::Unit);
    assert_eq!(
        p.type_of_scoped(&Expr::Arg { index: 0 }, &[parameter(Type::Bool)], &[])
            .unwrap(),
        Some(Type::Bool)
    );
    assert_eq!(
        p.type_of_scoped(&local(5), &[], &[(5, Type::Tree)])
            .unwrap(),
        Some(Type::Tree)
    );
    assert!(
        p.type_of_scoped(&local(5), &[], &[(5, Type::Tree), (5, Type::Tree)])
            .is_err()
    );
    assert!(
        p.type_of_scoped(&Expr::Arg { index: 0 }, &[], &[(0, Type::Tree)])
            .is_err()
    );
}

#[test]
fn function_and_parameter_limits_cover_unused_signatures_too() {
    let mut p = program(Expr::Unit);
    p.functions = (0..64).map(|id| function(id, Expr::Unit)).collect();
    p.validate().unwrap();
    p.functions.push(function(64, Expr::Unit));
    invalid(&p, "too many functions");
    p.functions = vec![function(0, Expr::Unit)];
    p.functions[0].parameters = vec![parameter(Type::Unit); 65];
    invalid(&p, "too many parameters");
    p.functions[0].parameters = vec![parameter(Type::Int {
        bits: 7,
        signed: false,
    })];
    invalid(&p, "integer width");
}

#[test]
fn node_type_depth_and_work_budgets_cover_the_combined_program() {
    let wide = Expr::Tuple {
        items: vec![Expr::Unit; 256],
    };
    let body = sequence(wide.clone(), sequence(wide, Expr::Unit));
    let mut p = program(Expr::Unit);
    p.functions = (0..38).map(|id| function(id, body.clone())).collect();
    p.validate().unwrap();
    p.functions = (0..40).map(|id| function(id, body.clone())).collect();
    invalid(&p, "expression node budget");

    let ty = Type::Tuple {
        items: vec![Type::Unit; 256],
    };
    p.functions = (0..2)
        .map(|id| {
            let mut f = function(id, Expr::Unit);
            f.parameters = vec![parameter(ty.clone()); 40];
            f
        })
        .collect();
    invalid(&p, "type node budget");

    p.functions.clear();
    p.body = (0..256).fold(Expr::Unit, |rest, _| sequence(Expr::Unit, rest));
    invalid(&p, "expression nesting");

    p.parameters = vec![parameter(ty)];
    let copies = Expr::Tuple {
        items: vec![Expr::Arg { index: 0 }; 256],
    };
    p.body = (0..8).fold(Expr::Unit, |rest, _| sequence(copies.clone(), rest));
    invalid(&p, "work budget");
}

fn runtime_compile_budget(p: &Program, kind: &str) {
    // These controls deliberately pass the structural IR limits. Rejection
    // must happen during bounded lowering, before codegen's expansion check.
    p.validate().unwrap();
    for optimize in [false, true] {
        let error = compile(p, optimize).unwrap_err().to_string();
        assert!(
            error.contains("runtime") && error.contains("budget") && error.contains(kind),
            "optimize={optimize}: expected early runtime {kind} budget error, got {error:?}"
        );
    }
}

#[test]
fn runtime_operand_frames_are_bounded_before_nested_wide_tuples_are_lowered() {
    // 50 * (255 Unit siblings + one Tuple) + the leaf + Sequence/Unit +
    // unused helper = 12,804 expression nodes, below the shared 20,000 cap.
    // Without an early frame bound, recursive operand lowering retains about
    // 12,750 temporary slots and repeatedly clones their growing vectors.
    let mut body = Expr::Unit;
    for _ in 0..50 {
        let mut items = vec![Expr::Unit; 255];
        items.push(body);
        body = Expr::Tuple { items };
    }
    let mut p = program(sequence(body, Expr::Unit));
    p.functions = vec![function(0, Expr::Unit)];
    runtime_compile_budget(&p, "frame");
}

#[test]
fn effectful_operands_retain_the_early_frame_bound_when_leaf_forwarding_is_enabled() {
    let value = Expr::If {
        condition: Box::new(Expr::Bool { value: true }),
        then_value: Box::new(Expr::Unit),
        else_value: Box::new(Expr::Unit),
    };
    let mut p = program(sequence(
        Expr::Tuple {
            items: vec![value; 129],
        },
        Expr::Unit,
    ));
    p.functions = vec![function(0, Expr::Unit)];
    runtime_compile_budget(&p, "frame");
}

#[test]
fn optimized_direct_unary_lowering_has_an_independent_native_depth_bound() {
    let mut p = program(Expr::Unit);
    p.result = Type::Bool;
    p.body = (0..193).fold(Expr::Bool { value: true }, |value, _| Expr::Not {
        value: Box::new(value),
    });
    p.functions = vec![function(0, Expr::Unit)];
    p.validate().unwrap();
    let error = compile(&p, true).unwrap_err().to_string();
    assert!(error.contains("runtime expression depth budget"), "{error}");
}

#[test]
fn runtime_lexical_frames_are_bounded_before_cloning_deeper_scopes() {
    let body = (0..129).rev().fold(Expr::Unit, |body, id| {
        bind(id, Type::Unit, Expr::Unit, body)
    });
    runtime_compile_budget(&program(body), "frame");
}

#[test]
fn runtime_work_budget_counts_aggregate_types_copied_with_lexical_scopes() {
    // Only 71 live slots and 18,249 declared type nodes are needed. Copying
    // every earlier aggregate local at each extension nevertheless exceeds
    // 500,000 type-node copies; a slot-count bound alone misses this workload.
    let ty = Type::Tuple {
        items: vec![Type::Unit; 256],
    };
    let mut p = program((0..70).rev().fold(Expr::Unit, |body, id| {
        bind(id, ty.clone(), Expr::Arg { index: 0 }, body)
    }));
    p.parameters = vec![parameter(ty)];
    p.functions = vec![function(0, Expr::Unit)];
    runtime_compile_budget(&p, "work");
}
