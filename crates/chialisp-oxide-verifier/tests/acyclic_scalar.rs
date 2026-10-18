//! Explicit typed-domain call models and finite VM controls, not preservation.
use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Function, Parameter, Program, Type};
use chialisp_oxide_verifier::{
    Comparison as C, Obligation, Predicate as P, ReplayOutcome, SolverConfig, Specification,
    Term as T, Verification, WitnessValue, emit, emit_acyclic_scalar, replay,
    verify_acyclic_scalar,
};
use chialisp_oxide_vm::FailureCode;
use std::sync::Mutex;
static SOLVER: Mutex<()> = Mutex::new(());
fn ty() -> Type {
    Type::Int {
        bits: 8,
        signed: false,
    }
}
fn arg(index: usize) -> Expr {
    Expr::Arg { index }
}
fn int(n: u16) -> Expr {
    Expr::Int {
        value: n.to_string(),
        ty: ty(),
    }
}
fn binary(kind: BinaryOp, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        kind,
        left: Box::new(left),
        right: Box::new(right),
        ty: ty(),
    }
}
fn call(id: usize, args: Vec<Expr>) -> Expr {
    Expr::Call {
        function_id: id,
        arguments: args,
    }
}
fn reject() -> Expr {
    Expr::Reject {
        message: "guard".into(),
    }
}
fn fun(id: usize, parameters: Vec<Type>, result: Type, body: Expr) -> Function {
    Function {
        id,
        name: format!("f{id}"),
        parameters: parameters
            .into_iter()
            .enumerate()
            .map(|(i, ty)| Parameter {
                name: format!("p{i}"),
                ty,
            })
            .collect(),
        result,
        body,
    }
}
fn program(body: Expr, functions: Vec<Function>) -> Program {
    Program {
        name: "acyclic".into(),
        parameters: vec![Parameter {
            name: "x".into(),
            ty: ty(),
        }],
        result: ty(),
        body,
        functions,
        environment: Environment::Strict,
    }
}
fn comparison(c: C, left: T, right: T) -> P {
    P::Compare {
        relation: c,
        left,
        right,
    }
}
fn input() -> T {
    T::Input {
        argument: 0,
        fields: vec![],
    }
}
fn number(n: u16) -> T {
    T::Int {
        value: n.to_string(),
    }
}
fn acceptance(p: P) -> Specification {
    Specification {
        assumptions: P::default(),
        obligation: Obligation::AcceptanceIff { predicate: p },
    }
}
fn success(p: P) -> Specification {
    Specification {
        assumptions: P::default(),
        obligation: Obligation::OnSuccess { property: p },
    }
}
fn verify(p: &Program, s: &Specification) -> Verification {
    let _guard = SOLVER.lock().unwrap();
    verify_acyclic_scalar(p, s, &SolverConfig::default()).unwrap()
}
fn proven(p: &Program, s: &Specification) {
    assert!(matches!(verify(p, s), Verification::Proven { .. }));
}
fn counterexample(p: &Program, s: &Specification) -> chialisp_oxide_verifier::Witness {
    let Verification::Refuted {
        counterexample,
        nonvacuity: Some(_),
    } = verify(p, s)
    else {
        panic!("nonvacuous mutant was not refuted")
    };
    counterexample
}
fn rejected(p: &Program, witness: &chialisp_oxide_verifier::Witness) {
    for row in replay(p, witness, 10_000_000).unwrap() {
        match row.outcome {
            ReplayOutcome::Rejected { failure, .. } => {
                assert_eq!(failure.code(), FailureCode::Raise)
            }
            other => panic!("not a conclusive Raise: {other:?}"),
        }
    }
}
fn accepted(p: &Program, witness: &chialisp_oxide_verifier::Witness, hex: &str) {
    for row in replay(p, witness, 10_000_000).unwrap() {
        let ReplayOutcome::Accepted { output_hex, .. } = row.outcome else {
            panic!("acceptance failed: {row:?}")
        };
        assert_eq!(output_hex, hex);
    }
}
#[test]
fn callee_first_summaries_prove_a_guard_and_output_without_runtime_default_admission() {
    let helper = fun(
        1,
        vec![ty()],
        ty(),
        Expr::If {
            condition: Box::new(binary(BinaryOp::Lt, arg(0), int(100))),
            then_value: Box::new(binary(BinaryOp::Add, arg(0), int(1))),
            else_value: Box::new(reject()),
        },
    );
    let wrapper = fun(0, vec![ty()], ty(), call(1, vec![arg(0)]));
    let p = program(call(0, vec![arg(0)]), vec![wrapper, helper]);
    let policy = acceptance(comparison(C::Lt, input(), number(100)));
    proven(&p, &policy);
    proven(
        &p,
        &success(comparison(
            C::Eq,
            T::Output { fields: vec![] },
            T::Add {
                left: Box::new(input()),
                right: Box::new(number(1)),
            },
        )),
    );
    assert!(
        emit(&p, &policy)
            .unwrap_err()
            .to_string()
            .contains("does not model runtime")
    );
    let query = emit_acyclic_scalar(&p, &policy).unwrap().counterexample;
    assert!(
        query.find("define-fun f_1_success").unwrap()
            < query.find("define-fun f_0_success").unwrap()
    );
    let mut mutant = p.clone();
    mutant.functions[1].body = binary(BinaryOp::Add, arg(0), int(1));
    let witness = counterexample(&mutant, &policy);
    rejected(&p, &witness);
    let WitnessValue::Int { value } = &witness.arguments[0] else {
        panic!("integer witness")
    };
    let n: u8 = value.parse().unwrap();
    assert!((100..255).contains(&n));
    let output = if n < 127 {
        format!("{:02x}", n + 1)
    } else {
        format!("8200{:02x}", n + 1)
    };
    accepted(&mutant, &witness, &output);
}
#[test]
fn unused_initializers_and_unused_call_arguments_remain_strict() {
    let policy = acceptance(comparison(C::Le, input(), number(254)));
    let increment = binary(BinaryOp::Add, arg(0), int(1));
    let ignored = fun(0, vec![ty()], ty(), int(7));
    let p = program(call(0, vec![increment.clone()]), vec![ignored.clone()]);
    proven(&p, &policy);
    let mutant = program(call(0, vec![arg(0)]), vec![ignored]);
    let witness = counterexample(&mutant, &policy);
    assert_eq!(
        witness.arguments,
        vec![WitnessValue::Int {
            value: "255".into()
        }]
    );
    rejected(&p, &witness);
    accepted(&mutant, &witness, "07");
    let p = program(
        Expr::Let {
            local_id: 0,
            ty: ty(),
            value: Box::new(increment),
            body: Box::new(int(7)),
        },
        vec![],
    );
    proven(&p, &policy);
    let mutant = program(int(7), vec![]);
    let witness = counterexample(&mutant, &policy);
    rejected(&p, &witness);
    accepted(&mutant, &witness, "07");
}
#[test]
fn lexical_frames_and_sibling_local_ids_do_not_capture_the_caller() {
    let helper = fun(
        0,
        vec![ty()],
        ty(),
        Expr::Let {
            local_id: 0,
            ty: ty(),
            value: Box::new(arg(0)),
            body: Box::new(Expr::Local { local_id: 0 }),
        },
    );
    let p = program(
        Expr::Let {
            local_id: 0,
            ty: ty(),
            value: Box::new(int(99)),
            body: Box::new(call(0, vec![arg(0)])),
        },
        vec![helper],
    );
    proven(
        &p,
        &success(comparison(C::Eq, T::Output { fields: vec![] }, input())),
    );
    let item = |n| Expr::Let {
        local_id: 0,
        ty: ty(),
        value: Box::new(int(n)),
        body: Box::new(Expr::Local { local_id: 0 }),
    };
    let mut tuple = program(
        Expr::Tuple {
            items: vec![item(1), item(2)],
        },
        vec![],
    );
    tuple.result = Type::Tuple {
        items: vec![ty(), ty()],
    };
    proven(
        &tuple,
        &success(P::All {
            items: vec![
                comparison(C::Eq, T::Output { fields: vec![0] }, number(1)),
                comparison(C::Eq, T::Output { fields: vec![1] }, number(2)),
            ],
        }),
    );
}
#[test]
fn tuples_evaluate_every_field_before_a_projection_or_unused_formal() {
    let tuple_ty = Type::Tuple {
        items: vec![ty(), ty()],
    };
    let helper = fun(
        0,
        vec![tuple_ty],
        ty(),
        Expr::Field {
            value: Box::new(arg(0)),
            index: 0,
        },
    );
    let tuple = Expr::Tuple {
        items: vec![int(7), binary(BinaryOp::Add, arg(0), int(1))],
    };
    let p = program(call(0, vec![tuple]), vec![helper]);
    proven(&p, &acceptance(comparison(C::Le, input(), number(254))));
    proven(
        &p,
        &success(comparison(C::Eq, T::Output { fields: vec![] }, number(7))),
    );
}
#[test]
fn rejection_does_not_become_a_vacuous_proof_and_lazy_branches_stay_lazy() {
    let p = program(
        call(0, vec![arg(0)]),
        vec![fun(0, vec![ty()], ty(), reject())],
    );
    assert_eq!(
        verify(&p, &success(P::Constant { value: false })),
        Verification::Vacuous
    );
    assert!(matches!(
        verify(&p, &acceptance(P::default())),
        Verification::Refuted {
            nonvacuity: None,
            ..
        }
    ));
    let mut lazy = p;
    lazy.functions[0].body = Expr::If {
        condition: Box::new(binary(BinaryOp::Lt, arg(0), int(100))),
        then_value: Box::new(int(7)),
        else_value: Box::new(reject()),
    };
    proven(&lazy, &acceptance(comparison(C::Lt, input(), number(100))));
}
#[test]
fn unused_recursive_scc_and_dead_unsupported_nodes_fail_before_solver() {
    let spec = success(P::default());
    let p = program(
        int(7),
        vec![fun(0, vec![ty()], ty(), call(0, vec![arg(0)]))],
    );
    assert!(
        emit_acyclic_scalar(&p, &spec)
            .unwrap_err()
            .to_string()
            .contains("recursive")
    );
    let p = program(
        int(7),
        vec![
            fun(0, vec![], ty(), call(1, vec![])),
            fun(1, vec![], ty(), call(0, vec![])),
        ],
    );
    assert!(
        emit_acyclic_scalar(&p, &spec)
            .unwrap_err()
            .to_string()
            .contains("recursive")
    );
    let mut p = program(
        int(7),
        vec![fun(0, vec![], Type::Atom, Expr::Atom { hex: "".into() })],
    );
    assert!(
        emit_acyclic_scalar(&p, &spec)
            .unwrap_err()
            .to_string()
            .contains("does not model")
    );
    p.functions[0] = fun(
        0,
        vec![],
        Type::Bytes { len: 32 },
        Expr::Sha256 { parts: vec![] },
    );
    assert!(emit_acyclic_scalar(&p, &spec).is_err());
    let mut p = program(int(7), vec![]);
    p.body = Expr::Local { local_id: 0 };
    assert!(
        emit_acyclic_scalar(&p, &spec)
            .unwrap_err()
            .to_string()
            .contains("invalid IR")
    );
}
#[test]
fn graph_and_optional_model_limits_do_not_change_compiler_admission() {
    let p = program(
        int(7),
        (0..17).map(|i| fun(i, vec![], ty(), int(1))).collect(),
    );
    p.validate().unwrap();
    assert!(
        emit_acyclic_scalar(&p, &success(P::default()))
            .unwrap_err()
            .to_string()
            .contains("budget")
    );
    let mut p = program(int(7), vec![]);
    for _ in 0..40 {
        p.body = Expr::Sequence {
            first: Box::new(Expr::Unit),
            then_value: Box::new(p.body),
        };
    }
    p.validate().unwrap();
    assert!(
        emit_acyclic_scalar(&p, &success(P::default()))
            .unwrap_err()
            .to_string()
            .contains("budget")
    );
}

#[test]
fn explosive_dependency_dags_and_specifications_fail_before_solver_allocation() {
    let functions = (0..16)
        .map(|id| {
            let body = if id == 15 {
                arg(0)
            } else {
                binary(
                    BinaryOp::Add,
                    call(id + 1, vec![arg(0)]),
                    call(id + 1, vec![arg(0)]),
                )
            };
            fun(id, vec![ty()], ty(), body)
        })
        .collect();
    let p = program(call(0, vec![arg(0)]), functions);
    p.validate().unwrap();
    assert!(
        emit_acyclic_scalar(&p, &success(P::default()))
            .unwrap_err()
            .to_string()
            .contains("dependency-work")
    );
    let p = program(int(7), vec![]);
    let mut predicate = P::default();
    for _ in 0..20 {
        predicate = P::Not {
            value: Box::new(predicate),
        };
    }
    assert!(
        emit_acyclic_scalar(&p, &success(predicate))
            .unwrap_err()
            .to_string()
            .contains("specification budget")
    );
}

#[test]
fn tuple_results_unit_parameters_and_bitwise_helpers_use_typed_formals() {
    let result = Type::Tuple {
        items: vec![ty(), Type::Bool, Type::Unit],
    };
    let helper = fun(
        0,
        vec![ty(), Type::Unit],
        result.clone(),
        Expr::Tuple {
            items: vec![
                binary(BinaryOp::Bitand, arg(0), int(15)),
                Expr::Bool { value: true },
                Expr::Unit,
            ],
        },
    );
    let mut p = program(
        Expr::Field {
            value: Box::new(call(0, vec![arg(0), Expr::Unit])),
            index: 0,
        },
        vec![helper],
    );
    proven(
        &p,
        &success(comparison(C::Le, T::Output { fields: vec![] }, number(15))),
    );
    p.body = call(0, vec![arg(0), Expr::Unit]);
    p.result = result;
    proven(
        &p,
        &success(comparison(
            C::Eq,
            T::Output { fields: vec![1] },
            T::Bool { value: true },
        )),
    );
}

#[test]
fn every_intermediate_tuple_is_bounded_before_projection_or_discard() {
    let policy = success(P::default());
    let tuple = Expr::Tuple {
        items: vec![arg(0); 17],
    };
    for body in [
        Expr::Field {
            value: Box::new(tuple.clone()),
            index: 0,
        },
        Expr::Sequence {
            first: Box::new(tuple),
            then_value: Box::new(int(7)),
        },
    ] {
        let p = program(body, vec![]);
        p.validate().unwrap();
        assert!(
            emit_acyclic_scalar(&p, &policy)
                .unwrap_err()
                .to_string()
                .contains("intermediate value/type budget")
        );
    }
    // Each child has <=16 fields, but the parent exceeds the aggregate bound.
    let nested = Expr::Tuple {
        items: vec![
            Expr::Tuple {
                items: vec![arg(0); 9]
            };
            2
        ],
    };
    let p = program(
        Expr::Sequence {
            first: Box::new(nested),
            then_value: Box::new(int(7)),
        },
        vec![],
    );
    p.validate().unwrap();
    assert!(
        emit_acyclic_scalar(&p, &policy)
            .unwrap_err()
            .to_string()
            .contains("intermediate value/type budget")
    );
    // Positive boundary: a projected 16-field intermediate remains admitted.
    let p = program(
        Expr::Field {
            value: Box::new(Expr::Tuple {
                items: vec![arg(0); 16],
            }),
            index: 15,
        },
        vec![],
    );
    assert!(emit_acyclic_scalar(&p, &policy).is_ok());
}
