use chialisp_oxide_ir::{BinaryOp, Expr, Parameter, Program, Type};
use chialisp_oxide_verifier::{
    Comparison as C, Obligation, Predicate as P, ReplayFailure, ReplayOutcome, SolverConfig,
    Specification, Term as T, Verification, Witness, WitnessValue as W, emit, replay,
    verify as verify_with_solver,
};
use chialisp_oxide_vm::{FailureCode, FailureKind, classify_failure};
use clvmr::{
    Allocator, ChiaDialect,
    allocator::NodePtr,
    chia_dialect::MEMPOOL_MODE,
    run_program,
    serde::{node_from_bytes, node_to_bytes},
};
use num_bigint::BigInt;
use std::sync::Mutex;

// Serialize solver calls within this test binary so concurrent test scheduling
// does not consume their wall-clock deadlines through contention. This changes
// test scheduling only; solver deadlines and fail-closed errors are unchanged.
static SOLVER: Mutex<()> = Mutex::new(());

fn verify(
    program: &Program,
    specification: &Specification,
    config: &SolverConfig,
) -> anyhow::Result<Verification> {
    let _guard = SOLVER.lock().expect("IR-model solver lock poisoned");
    verify_with_solver(program, specification, config)
}

fn i8_type() -> Type {
    Type::Int {
        bits: 8,
        signed: true,
    }
}
fn arg(index: usize) -> Expr {
    Expr::Arg { index }
}
fn lit(value: i16) -> Expr {
    Expr::Int {
        value: value.to_string(),
        ty: i8_type(),
    }
}
fn binary(kind: BinaryOp, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        kind,
        left: Box::new(left),
        right: Box::new(right),
        ty: i8_type(),
    }
}
fn input() -> T {
    T::Input {
        argument: 0,
        fields: vec![],
    }
}
fn integer(value: i16) -> T {
    T::Int {
        value: value.to_string(),
    }
}
fn compare(relation: C, left: T, right: T) -> P {
    P::Compare {
        relation,
        left,
        right,
    }
}
fn spec(property: P) -> Specification {
    Specification {
        assumptions: P::default(),
        obligation: Obligation::OnSuccess { property },
    }
}
fn acceptance(predicate: P) -> Specification {
    Specification {
        assumptions: P::default(),
        obligation: Obligation::AcceptanceIff { predicate },
    }
}
fn program(body: Expr) -> Program {
    Program {
        functions: vec![],
        environment: chialisp_oxide_ir::Environment::Strict,
        name: "test".into(),
        parameters: vec![Parameter {
            name: "value".into(),
            ty: i8_type(),
        }],
        result: i8_type(),
        body,
    }
}
fn assert_proven(program: &Program, specification: &Specification) {
    let result = verify(program, specification, &SolverConfig::default()).unwrap();
    assert!(matches!(result, Verification::Proven { .. }), "{result:?}");
}

fn allocate(a: &mut Allocator, value: &W) -> NodePtr {
    match value {
        W::Int { value } => {
            let n: BigInt = value.parse().unwrap();
            a.new_atom(&if n == BigInt::from(0) {
                vec![]
            } else {
                n.to_signed_bytes_be()
            })
            .unwrap()
        }
        W::Bool { value } => {
            if *value {
                a.new_atom(&[1]).unwrap()
            } else {
                NodePtr::NIL
            }
        }
        W::Tuple { items } => list(a, items),
        W::Unit => NodePtr::NIL,
    }
}
fn list(a: &mut Allocator, items: &[W]) -> NodePtr {
    items.iter().rev().fold(NodePtr::NIL, |tail, item| {
        let head = allocate(a, item);
        a.new_pair(head, tail).unwrap()
    })
}
fn execute(bytes: &[u8], witness: &Witness) -> Result<Vec<u8>, String> {
    let mut a = Allocator::new();
    let root = node_from_bytes(&mut a, bytes).unwrap();
    let solution = list(&mut a, &witness.arguments);
    match run_program(
        &mut a,
        &ChiaDialect::new(MEMPOOL_MODE),
        root,
        solution,
        10_000_000,
    ) {
        Ok(result) => Ok(node_to_bytes(&a, result.1).unwrap()),
        Err(error) => {
            let failure = classify_failure(&error);
            assert!(
                failure.is_program_rejection(),
                "inconclusive VM failure: {failure:?}: {error:?}"
            );
            assert_eq!(
                failure.code(),
                FailureCode::Raise,
                "unexpected scalar rejection: {error:?}"
            );
            Err(error.to_string())
        }
    }
}
fn scalar(value: i16) -> Witness {
    Witness {
        arguments: vec![W::Int {
            value: value.to_string(),
        }],
    }
}
fn encode_expected(value: i16) -> Vec<u8> {
    let mut a = Allocator::new();
    let node = allocate(
        &mut a,
        &W::Int {
            value: value.to_string(),
        },
    );
    node_to_bytes(&a, node).unwrap()
}

#[test]
fn guard_invariant_and_mutation_witness_replay_in_actual_clvm() {
    let condition = binary(BinaryOp::Le, arg(0), lit(100));
    let original = program(Expr::If {
        condition: Box::new(condition),
        then_value: Box::new(arg(0)),
        else_value: Box::new(Expr::Reject {
            message: "cap".into(),
        }),
    });
    let policy = compare(C::Le, input(), integer(100));
    assert_proven(&original, &acceptance(policy.clone()));
    let invariant = spec(compare(C::Le, T::Output { fields: vec![] }, integer(100)));
    assert_proven(&original, &invariant);
    let mutant = program(arg(0));
    let Verification::Refuted {
        counterexample,
        nonvacuity: Some(_),
    } = verify(&mutant, &invariant, &SolverConfig::default()).unwrap()
    else {
        panic!("removed guard was not detected")
    };
    let W::Int { value } = &counterexample.arguments[0] else {
        panic!("wrong witness type")
    };
    let amount: i8 = value.parse().unwrap();
    assert!(amount > 100); // Independent native Rust policy.
    for optimize in [false, true] {
        assert!(
            execute(
                &chialisp_oxide_ir::compile(&original, optimize).unwrap(),
                &counterexample
            )
            .is_err()
        );
        assert_eq!(
            execute(
                &chialisp_oxide_ir::compile(&mutant, optimize).unwrap(),
                &counterexample
            )
            .unwrap(),
            encode_expected(i16::from(amount))
        );
    }
}

#[test]
fn checked_arithmetic_model_matches_native_and_clvm_exhaustive_i8() {
    for (op, term) in [
        (
            BinaryOp::Add,
            T::Add {
                left: Box::new(input()),
                right: Box::new(integer(1)),
            },
        ),
        (
            BinaryOp::Sub,
            T::Sub {
                left: Box::new(input()),
                right: Box::new(integer(1)),
            },
        ),
        (
            BinaryOp::Mul,
            T::Mul {
                left: Box::new(input()),
                right: Box::new(integer(2)),
            },
        ),
    ] {
        let operand = if op == BinaryOp::Mul { 2 } else { 1 };
        let p = program(binary(op, arg(0), lit(operand)));
        assert_proven(
            &p,
            &spec(compare(C::Eq, T::Output { fields: vec![] }, term.clone())),
        );
        assert_proven(
            &p,
            &acceptance(P::All {
                items: vec![
                    compare(C::Ge, term.clone(), integer(-128)),
                    compare(C::Le, term, integer(127)),
                ],
            }),
        );
        for optimize in [false, true] {
            let bytes = chialisp_oxide_ir::compile(&p, optimize).unwrap();
            for value in i8::MIN..=i8::MAX {
                let native = match op {
                    BinaryOp::Add => value.checked_add(1),
                    BinaryOp::Sub => value.checked_sub(1),
                    BinaryOp::Mul => value.checked_mul(2),
                    _ => unreachable!(),
                };
                let output = execute(&bytes, &scalar(i16::from(value)));
                match native {
                    Some(value) => assert_eq!(output.unwrap(), encode_expected(i16::from(value))),
                    None => assert!(output.is_err()),
                }
            }
        }
    }
}

#[test]
fn if_is_lazy_and_discarded_arithmetic_remains_strict() {
    let overflow = binary(BinaryOp::Add, arg(0), lit(127));
    let lazy = program(Expr::If {
        condition: Box::new(binary(BinaryOp::Gt, arg(0), lit(0))),
        then_value: Box::new(overflow.clone()),
        else_value: Box::new(lit(0)),
    });
    let strict = program(Expr::Sequence {
        first: Box::new(overflow.clone()),
        then_value: Box::new(lit(0)),
    });
    let projection = program(Expr::Field {
        value: Box::new(Expr::Tuple {
            items: vec![arg(0), overflow],
        }),
        index: 0,
    });
    let policy = acceptance(compare(C::Le, input(), integer(0)));
    for p in [&lazy, &strict, &projection] {
        assert_proven(p, &policy);
        for optimize in [false, true] {
            let bytes = chialisp_oxide_ir::compile(p, optimize).unwrap();
            for value in [-128, -1, 0, 1, 127] {
                assert_eq!(
                    execute(&bytes, &scalar(value)).is_ok(),
                    value <= 0,
                    "value={value} optimize={optimize}"
                );
            }
        }
    }
}

#[test]
fn boolean_and_nested_tuple_inputs_get_typed_replayable_witnesses() {
    let mut p = program(Expr::Field {
        value: Box::new(arg(0)),
        index: 1,
    });
    p.parameters[0].ty = Type::Tuple {
        items: vec![Type::Bool, i8_type(), Type::Unit],
    };
    let invariant = spec(compare(
        C::Eq,
        T::Output { fields: vec![] },
        T::Input {
            argument: 0,
            fields: vec![1],
        },
    ));
    assert_proven(&p, &invariant);
    let false_invariant = spec(compare(
        C::Eq,
        T::Input {
            argument: 0,
            fields: vec![0],
        },
        T::Bool { value: true },
    ));
    let Verification::Refuted { counterexample, .. } =
        verify(&p, &false_invariant, &SolverConfig::default()).unwrap()
    else {
        panic!("expected typed boolean witness")
    };
    let W::Tuple { items } = &counterexample.arguments[0] else {
        panic!("tuple expected")
    };
    assert_eq!(items[0], W::Bool { value: false });
    for optimize in [false, true] {
        assert!(
            execute(
                &chialisp_oxide_ir::compile(&p, optimize).unwrap(),
                &counterexample
            )
            .is_ok()
        );
    }
}

#[test]
fn rejection_never_proves_a_vacuous_claim() {
    let p = program(Expr::Reject {
        message: "always reject".into(),
    });
    assert_eq!(
        verify(
            &p,
            &spec(P::Constant { value: false }),
            &SolverConfig::default()
        )
        .unwrap(),
        Verification::Vacuous
    );
    let violated_acceptance = acceptance(P::Constant { value: true });
    assert!(matches!(
        verify(&p, &violated_acceptance, &SolverConfig::default()).unwrap(),
        Verification::Refuted {
            nonvacuity: None,
            ..
        }
    ));
    let mut impossible_assumption = spec(P::Constant { value: false });
    impossible_assumption.assumptions = P::Constant { value: false };
    assert_eq!(
        verify(
            &program(arg(0)),
            &impossible_assumption,
            &SolverConfig::default()
        )
        .unwrap(),
        Verification::Vacuous
    );
}

#[test]
fn unsupported_or_malformed_inputs_fail_closed() {
    let mut p = program(arg(0));
    p.parameters[0].ty = Type::Bytes { len: 32 };
    p.result = Type::Bytes { len: 32 };
    assert!(
        emit(&p, &spec(P::default()))
            .unwrap_err()
            .to_string()
            .contains("byte semantics")
    );
    let p = program(arg(0));
    let injected = spec(compare(
        C::Eq,
        input(),
        T::Int {
            value: "0) (assert false) ;".into(),
        },
    ));
    assert!(emit(&p, &injected).is_err());
    let malformed_field = spec(compare(C::Eq, T::Output { fields: vec![0] }, integer(0)));
    assert!(emit(&p, &malformed_field).is_err());
    let mut output_assumption = spec(P::default());
    output_assumption.assumptions = compare(C::Eq, T::Output { fields: vec![] }, integer(0));
    assert!(emit(&p, &output_assumption).is_err());
    let unavailable = SolverConfig {
        executable: "/no/such/solver".into(),
        ..SolverConfig::default()
    };
    assert!(verify(&p, &spec(P::default()), &unavailable).is_err());
}

#[test]
fn unit_and_zero_argument_programs_are_nonvacuous() {
    let p = Program {
        functions: vec![],
        environment: chialisp_oxide_ir::Environment::Strict,
        name: "unit".into(),
        parameters: vec![],
        result: Type::Unit,
        body: Expr::Unit,
    };
    assert_eq!(
        verify(&p, &spec(P::default()), &SolverConfig::default()).unwrap(),
        Verification::Proven {
            nonvacuity: Witness { arguments: vec![] }
        }
    );
}

#[test]
fn public_replay_checks_witness_types_bounds_and_cost() {
    let p = program(arg(0));
    let results = replay(&p, &scalar(42), 1_000_000).unwrap();
    assert_eq!(results.len(), 2);
    for result in results {
        let ReplayOutcome::Accepted { output_hex, cost } = result.outcome else {
            panic!("identity rejected")
        };
        assert_eq!(output_hex, "2a");
        assert!(cost > 0);
    }
    assert!(replay(&p, &scalar(-129), 1_000_000).is_err());
    assert!(
        replay(
            &p,
            &Witness {
                arguments: vec![W::Bool { value: true }]
            },
            1_000_000
        )
        .is_err()
    );
    assert!(replay(&p, &Witness { arguments: vec![] }, 1_000_000).is_err());
    assert!(
        replay(&p, &scalar(42), 0)
            .unwrap_err()
            .to_string()
            .contains("cost limit")
    );
    let exhausted = replay(&p, &scalar(42), 1).unwrap_err();
    let exhausted = exhausted.downcast_ref::<ReplayFailure>().unwrap();
    assert_eq!(exhausted.failure.kind(), FailureKind::Resource);
    assert_eq!(exhausted.failure.code(), FailureCode::CostExceeded);
}
