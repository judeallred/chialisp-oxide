//! Model-level bitwise checks plus actual finite VM witness replay. None of
//! these assertions proves Rust extraction, ABI or backend preservation.
use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Parameter, Program, Type};
use chialisp_oxide_verifier::{
    Comparison as C, Obligation, Predicate as P, ReplayOutcome, SolverConfig, Specification,
    StructuralBooleanVerification, Term as T, TreeBooleanSpecification, TreePredicate as TP,
    TreeTerm as TT, TreeValue, TreeWitness, Verification, Witness, WitnessValue, emit,
    emit_structural_boolean, replay, replay_structural_boolean, verify, verify_structural_boolean,
};
use chialisp_oxide_vm::{FailureCode, FailureKind};
use num_bigint::BigInt;
use std::sync::Mutex;

static SOLVER: Mutex<()> = Mutex::new(());
const OPS: [BinaryOp; 3] = [BinaryOp::Bitand, BinaryOp::Bitor, BinaryOp::Bitxor];
fn arg(index: usize) -> Expr {
    Expr::Arg { index }
}
fn input(argument: usize) -> T {
    T::Input {
        argument,
        fields: vec![],
    }
}
fn output() -> T {
    T::Output { fields: vec![] }
}
fn integer(value: impl ToString) -> T {
    T::Int {
        value: value.to_string(),
    }
}
fn literal(ty: &Type, value: impl ToString) -> Expr {
    Expr::Int {
        value: value.to_string(),
        ty: ty.clone(),
    }
}
fn bits(op: BinaryOp, ty: &Type, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        kind: op,
        ty: ty.clone(),
        left: Box::new(left),
        right: Box::new(right),
    }
}
fn sequence(first: Expr, then_value: Expr) -> Expr {
    Expr::Sequence {
        first: Box::new(first),
        then_value: Box::new(then_value),
    }
}
fn program(types: Vec<Type>, result: Type, body: Expr) -> Program {
    Program {
        name: "bitwise-model".into(),
        environment: Environment::Strict,
        parameters: types
            .into_iter()
            .enumerate()
            .map(|(i, ty)| Parameter {
                name: format!("arg{i}"),
                ty,
            })
            .collect(),
        result,
        body,
        functions: vec![],
    }
}
fn compare(relation: C, left: T, right: T) -> P {
    P::Compare {
        relation,
        left,
        right,
    }
}
fn eq(left: T, right: T) -> P {
    compare(C::Eq, left, right)
}
fn on_success(property: P) -> Specification {
    Specification {
        assumptions: P::default(),
        obligation: Obligation::OnSuccess { property },
    }
}
fn accept(predicate: P) -> Specification {
    Specification {
        assumptions: P::default(),
        obligation: Obligation::AcceptanceIff { predicate },
    }
}
fn prove(p: &Program, spec: &Specification) -> Witness {
    let result = verify(p, spec, &SolverConfig::default()).unwrap_or_else(|error| {
        panic!(
            "{error}\nProgram: {p:?}\nSpecification: {spec:?}\nQueries: {:?}",
            emit(p, spec).unwrap()
        )
    });
    let Verification::Proven { nonvacuity } = result else {
        panic!("{result:?}")
    };
    nonvacuity
}
fn wire_number(value: impl ToString) -> String {
    let value: BigInt = value.to_string().parse().unwrap();
    let bytes = if value == BigInt::from(0) {
        vec![]
    } else {
        value.to_signed_bytes_be()
    };
    if bytes.is_empty() {
        return "80".into();
    }
    let prefix = if bytes.len() == 1 && bytes[0] < 128 {
        String::new()
    } else {
        format!("{:02x}", 128 + bytes.len())
    };
    format!(
        "{prefix}{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )
}
fn replays_equal(p: &Program, witness: &Witness, expected: &str) {
    for row in replay(p, witness, 10_000_000).unwrap() {
        let ReplayOutcome::Accepted { output_hex, cost } = row.outcome else {
            panic!("unexpected replay rejection")
        };
        assert!(cost > 0);
        assert_eq!(output_hex, expected);
    }
}

#[test]
fn all_width_absorption_and_cancellation_identities_have_nonvacuity_and_vm_replay() {
    let _guard = SOLVER.lock().unwrap();
    for width in [8, 16, 32, 64, 128] {
        for signed in [false, true] {
            let ty = Type::Int {
                bits: width,
                signed,
            };
            let all = if signed {
                BigInt::from(-1)
            } else {
                (BigInt::from(1u8) << usize::from(width)) - 1u8
            };
            for (body, expected) in [
                (
                    bits(BinaryOp::Bitand, &ty, arg(0), literal(&ty, 0)),
                    BigInt::from(0),
                ),
                (
                    bits(BinaryOp::Bitor, &ty, literal(&ty, &all), arg(0)),
                    all.clone(),
                ),
                (bits(BinaryOp::Bitxor, &ty, arg(0), arg(0)), BigInt::from(0)),
            ] {
                let p = program(vec![ty.clone()], ty.clone(), body);
                let spec = on_success(eq(output(), integer(&expected)));
                let query = emit(&p, &spec).unwrap();
                assert!(query.counterexample.contains("(set-logic ALL)"));
                assert!(
                    query
                        .counterexample
                        .contains(&format!("(declare-const arg_0_bits (_ BitVec {width}))"))
                );
                assert!(query.counterexample.contains("bv2int"));
                let witness = prove(&p, &spec);
                replays_equal(&p, &witness, &wire_number(&expected));
            }
            let identity = program(
                vec![ty.clone()],
                ty.clone(),
                bits(BinaryOp::Bitand, &ty, arg(0), arg(0)),
            );
            let witness = prove(&identity, &on_success(eq(output(), input(0))));
            let [WitnessValue::Int { value }] = witness.arguments.as_slice() else {
                panic!("expected typed integer identity witness")
            };
            replays_equal(&identity, &witness, &wire_number(value));
        }
    }
}

#[test]
fn sign_bit_reinterpretation_matches_native_i128_and_u128_under_exact_inputs() {
    let _guard = SOLVER.lock().unwrap();
    macro_rules! native {
        ($native:ty, $signed:literal, $a:expr, $b:expr) => {{
            let ty = Type::Int {
                bits: 128,
                signed: $signed,
            };
            let a: $native = $a;
            let b: $native = $b;
            for op in OPS {
                let expected: $native = match op {
                    BinaryOp::Bitand => a & b,
                    BinaryOp::Bitor => a | b,
                    BinaryOp::Bitxor => a ^ b,
                    _ => unreachable!(),
                };
                let p = program(
                    vec![ty.clone(), ty.clone()],
                    ty.clone(),
                    bits(op, &ty, arg(0), arg(1)),
                );
                let mut spec = on_success(eq(output(), integer(expected)));
                spec.assumptions = P::All {
                    items: vec![eq(input(0), integer(a)), eq(input(1), integer(b))],
                };
                let witness = prove(&p, &spec);
                replays_equal(&p, &witness, &wire_number(expected));
            }
        }};
    }
    native!(i128, true, i128::MIN, -1);
    native!(i128, true, i128::MIN, i128::MAX);
    native!(u128, false, 1u128 << 127, u128::MAX);
}

#[test]
fn integer_operator_mutation_produces_a_real_counterexample_not_a_vacuous_proof() {
    let _guard = SOLVER.lock().unwrap();
    for signed in [false, true] {
        let ty = Type::Int { bits: 128, signed };
        let original = program(
            vec![ty.clone()],
            ty.clone(),
            bits(BinaryOp::Bitxor, &ty, arg(0), arg(0)),
        );
        let mutant = program(
            vec![ty.clone()],
            ty.clone(),
            bits(BinaryOp::Bitor, &ty, arg(0), arg(0)),
        );
        let spec = on_success(eq(output(), integer(0)));
        prove(&original, &spec);
        let result = verify(&mutant, &spec, &SolverConfig::default()).unwrap();
        let Verification::Refuted {
            counterexample,
            nonvacuity: Some(_),
        } = result
        else {
            panic!("{result:?}")
        };
        let [WitnessValue::Int { value }] = counterexample.arguments.as_slice() else {
            panic!("expected one integer witness")
        };
        assert_ne!(value.parse::<BigInt>().unwrap(), BigInt::from(0));
        replays_equal(&original, &counterexample, "80");
        replays_equal(&mutant, &counterexample, &wire_number(value));
    }
}

fn boolean_policy(op: BinaryOp) -> P {
    P::Any {
        items: [false, true]
            .into_iter()
            .flat_map(|a| {
                [false, true].into_iter().map(move |b| {
                    let expected = match op {
                        BinaryOp::Bitand => a & b,
                        BinaryOp::Bitor => a | b,
                        BinaryOp::Bitxor => a ^ b,
                        _ => unreachable!(),
                    };
                    P::All {
                        items: vec![
                            eq(input(0), T::Bool { value: a }),
                            eq(input(1), T::Bool { value: b }),
                            eq(output(), T::Bool { value: expected }),
                        ],
                    }
                })
            })
            .collect(),
    }
}

#[test]
fn scalar_boolean_truth_tables_and_strict_rejection_are_modeled_and_replayed() {
    let _guard = SOLVER.lock().unwrap();
    for op in OPS {
        let p = program(
            vec![Type::Bool, Type::Bool],
            Type::Bool,
            bits(op, &Type::Bool, arg(0), arg(1)),
        );
        let spec = on_success(boolean_policy(op));
        assert!(
            emit(&p, &spec)
                .unwrap()
                .counterexample
                .contains("(set-logic QF_NIA)")
        );
        prove(&p, &spec);
        for a in [false, true] {
            for b in [false, true] {
                let wanted = match op {
                    BinaryOp::Bitand => a & b,
                    BinaryOp::Bitor => a | b,
                    BinaryOp::Bitxor => a ^ b,
                    _ => unreachable!(),
                };
                replays_equal(
                    &p,
                    &Witness {
                        arguments: vec![
                            WitnessValue::Bool { value: a },
                            WitnessValue::Bool { value: b },
                        ],
                    },
                    if wanted { "01" } else { "80" },
                );
            }
        }
        let checked = Expr::If {
            condition: Box::new(arg(0)),
            then_value: Box::new(Expr::Reject {
                message: "intentional true-input rejection".into(),
            }),
            else_value: Box::new(Expr::Bool { value: false }),
        };
        // false & checked and true | checked still evaluate checked.
        let mask = Expr::Bool {
            value: op == BinaryOp::Bitor,
        };
        for (left, right) in [(mask.clone(), checked.clone()), (checked, mask)] {
            let p = program(
                vec![Type::Bool],
                Type::Bool,
                bits(op, &Type::Bool, left, right),
            );
            prove(&p, &accept(eq(input(0), T::Bool { value: false })));
            for row in replay(
                &p,
                &Witness {
                    arguments: vec![WitnessValue::Bool { value: true }],
                },
                1_000_000,
            )
            .unwrap()
            {
                let ReplayOutcome::Rejected { failure, .. } = row.outcome else {
                    panic!("strict operand was skipped")
                };
                assert_eq!(failure.kind(), FailureKind::Program);
                assert_eq!(failure.code(), FailureCode::Raise);
            }
        }
    }
    let p = program(
        vec![Type::Bool, Type::Bool],
        Type::Bool,
        bits(BinaryOp::Bitor, &Type::Bool, arg(0), arg(1)),
    );
    let result = verify(
        &p,
        &on_success(boolean_policy(BinaryOp::Bitand)),
        &SolverConfig::default(),
    )
    .unwrap();
    let Verification::Refuted {
        counterexample,
        nonvacuity: Some(_),
    } = result
    else {
        panic!("{result:?}")
    };
    let [
        WitnessValue::Bool { value: a },
        WitnessValue::Bool { value: b },
    ] = counterexample.arguments.as_slice()
    else {
        panic!("Boolean mutant witness")
    };
    assert_ne!(a & b, a | b);
    replays_equal(&p, &counterexample, if a | b { "01" } else { "80" });
}

fn tree_atom(hex: &str) -> TT {
    TT::Atom { hex: hex.into() }
}
fn head(index: usize) -> TT {
    let tail = (0..index).fold(TT::Input, |value, _| TT::Rest {
        value: Box::new(value),
    });
    TT::First {
        value: Box::new(tail),
    }
}
fn byte_equal(left: TT, right: TT) -> TP {
    TP::ByteEqual { left, right }
}
fn structural_spec(op: BinaryOp) -> TreeBooleanSpecification {
    let mut tail = TT::Input;
    let mut items = vec![];
    for index in 0..2 {
        items.push(TP::IsPair {
            value: tail.clone(),
        });
        items.push(TP::Any {
            items: vec![
                byte_equal(head(index), tree_atom("")),
                byte_equal(head(index), tree_atom("01")),
            ],
        });
        tail = TT::Rest {
            value: Box::new(tail),
        };
    }
    items.push(byte_equal(tail, tree_atom("")));
    let a = byte_equal(head(0), tree_atom("01"));
    let b = byte_equal(head(1), tree_atom("01"));
    let output = match op {
        BinaryOp::Bitand => TP::All { items: vec![a, b] },
        BinaryOp::Bitor => TP::Any { items: vec![a, b] },
        BinaryOp::Bitxor => TP::Not {
            value: Box::new(byte_equal(head(0), head(1))),
        },
        _ => unreachable!(),
    };
    TreeBooleanSpecification {
        acceptance: TP::All { items },
        output,
    }
}

#[test]
fn structural_boolean_bitwise_models_the_complete_canonical_abi_and_rejects_integer_semantics() {
    let _guard = SOLVER.lock().unwrap();
    for op in OPS {
        let p = program(
            vec![Type::Bool, Type::Bool],
            Type::Bool,
            bits(op, &Type::Bool, arg(0), arg(1)),
        );
        let spec = structural_spec(op);
        let result = verify_structural_boolean(&p, &spec, &SolverConfig::default()).unwrap();
        let StructuralBooleanVerification::Proven {
            true_witness: Some(yes),
            false_witness: Some(no),
            ..
        } = result
        else {
            panic!("{result:?}")
        };
        for (witness, wanted) in [(yes, "01"), (no, "80")] {
            for row in replay_structural_boolean(&p, &witness, 1_000_000).unwrap() {
                let ReplayOutcome::Accepted { output_hex, .. } = row.outcome else {
                    panic!("Boolean proof witness rejected")
                };
                assert_eq!(output_hex, wanted);
            }
        }
        let mut reject = p.clone();
        reject.body = bits(
            op,
            &Type::Bool,
            Expr::Bool {
                value: op == BinaryOp::Bitor,
            },
            sequence(
                Expr::Reject {
                    message: "required Boolean".into(),
                },
                Expr::Bool { value: true },
            ),
        );
        let result = verify_structural_boolean(&reject, &spec, &SolverConfig::default()).unwrap();
        let StructuralBooleanVerification::Refuted { counterexample, .. } = result else {
            panic!("always-reject bitwise was hidden")
        };
        for row in replay_structural_boolean(&reject, &counterexample, 1_000_000).unwrap() {
            let ReplayOutcome::Rejected { failure, .. } = row.outcome else {
                panic!("strict bitwise rejection was skipped")
            };
            assert_eq!(failure.code(), FailureCode::Raise);
        }
        let ty = Type::Int {
            bits: 8,
            signed: false,
        };
        let mut unsupported = p.clone();
        unsupported.body = Expr::If {
            condition: Box::new(Expr::Bool { value: true }),
            then_value: Box::new(p.body.clone()),
            else_value: Box::new(sequence(
                bits(op, &ty, literal(&ty, 1), literal(&ty, 2)),
                Expr::Bool { value: false },
            )),
        };
        unsupported.validate().unwrap();
        assert!(emit_structural_boolean(&unsupported, &spec).is_err());
    }
    // A nonempty zero atom is not a Boolean false, and an improper tail is not
    // the exact two-argument ABI. These are program failures, never resources.
    let atom = |hex: &str| TreeValue::Atom { hex: hex.into() };
    let pair = |first, rest| TreeValue::Pair {
        first: Box::new(first),
        rest: Box::new(rest),
    };
    let p = program(
        vec![Type::Bool, Type::Bool],
        Type::Bool,
        bits(BinaryOp::Bitand, &Type::Bool, arg(0), arg(1)),
    );
    for env in [
        pair(atom("00"), pair(atom(""), atom(""))),
        pair(atom("01"), pair(atom(""), atom("00"))),
    ] {
        for row in
            replay_structural_boolean(&p, &TreeWitness { environment: env }, 1_000_000).unwrap()
        {
            let ReplayOutcome::Rejected { failure, .. } = row.outcome else {
                panic!("malformed Boolean ABI accepted")
            };
            assert_eq!(failure.kind(), FailureKind::Program);
        }
    }
}
