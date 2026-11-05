//! Independent raw-environment specifications; these are IR-model checks plus
//! finite VM replay, not Rust extraction, ABI or compiler-preservation proofs.
use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Function, Parameter, Program, Type};
use chialisp_oxide_verifier::{
    ReplayFailure, ReplayOutcome, SolverConfig, StructuralBooleanVerification as Verification,
    TreeBooleanSpecification as Spec, TreePredicate as P, TreeTerm as T, TreeValue, TreeWitness,
    emit_structural_boolean, replay_structural_boolean, verify_structural_boolean,
};
use chialisp_oxide_vm::{FailureCode, FailureKind};
use std::sync::Mutex;

static SOLVER: Mutex<()> = Mutex::new(());
fn verify(program: &Program, spec: &Spec) -> anyhow::Result<Verification> {
    let _guard = SOLVER.lock().unwrap();
    verify_structural_boolean(program, spec, &SolverConfig::default())
}
fn atom(hex: &str) -> TreeValue {
    TreeValue::Atom { hex: hex.into() }
}
fn pair(first: TreeValue, rest: TreeValue) -> TreeValue {
    TreeValue::Pair {
        first: Box::new(first),
        rest: Box::new(rest),
    }
}
fn list(values: Vec<TreeValue>) -> TreeValue {
    values
        .into_iter()
        .rev()
        .fold(atom(""), |rest, first| pair(first, rest))
}
fn literal(hex: &str) -> T {
    T::Atom { hex: hex.into() }
}
fn first(value: T) -> T {
    T::First {
        value: Box::new(value),
    }
}
fn rest(value: T) -> T {
    T::Rest {
        value: Box::new(value),
    }
}
fn head(index: usize) -> T {
    first((0..index).fold(T::Input, |value, _| rest(value)))
}
fn byte_eq(left: T, right: T) -> P {
    P::ByteEqual { left, right }
}
fn not(value: P) -> P {
    P::Not {
        value: Box::new(value),
    }
}
fn constant(value: bool) -> P {
    P::Constant { value }
}
fn proper_arguments(count: usize) -> Vec<P> {
    let mut tail = T::Input;
    let mut conditions = vec![];
    for _ in 0..count {
        conditions.push(P::IsPair {
            value: tail.clone(),
        });
        tail = rest(tail);
    }
    conditions.push(byte_eq(tail, literal("")));
    conditions
}
fn atom_spec() -> Spec {
    let mut conditions = proper_arguments(2);
    conditions.extend([P::IsAtom { value: head(0) }, P::IsAtom { value: head(1) }]);
    Spec {
        acceptance: P::All { items: conditions },
        output: not(byte_eq(head(0), head(1))),
    }
}
fn arg(index: usize) -> Expr {
    Expr::Arg { index }
}
fn program(types: Vec<Type>, body: Expr) -> Program {
    Program {
        name: "structural-Boolean".into(),
        environment: Environment::Strict,
        parameters: types
            .into_iter()
            .enumerate()
            .map(|(i, ty)| Parameter {
                name: format!("arg{i}"),
                ty,
            })
            .collect(),
        result: Type::Bool,
        body,
        functions: vec![],
    }
}
fn unequal(ty: Type) -> Expr {
    Expr::Binary {
        kind: BinaryOp::Ne,
        left: Box::new(arg(0)),
        right: Box::new(arg(1)),
        ty,
    }
}
fn source() -> Program {
    program(vec![Type::Atom, Type::Atom], unequal(Type::Atom))
}
fn replay(program: &Program, environment: TreeValue) -> Option<bool> {
    let witness = TreeWitness { environment };
    let mut outcomes = vec![];
    for replay in replay_structural_boolean(program, &witness, 10_000_000).unwrap() {
        outcomes.push(match replay.outcome {
            ReplayOutcome::Accepted { output_hex, cost } => {
                assert!(cost > 0);
                Some(match output_hex.as_str() {
                    "80" => false,
                    "01" => true,
                    _ => panic!("not a canonical Boolean"),
                })
            }
            ReplayOutcome::Rejected { failure, .. } => {
                assert_eq!(failure.kind(), FailureKind::Program);
                None
            }
        });
    }
    assert_eq!(
        outcomes[0], outcomes[1],
        "optimization must preserve the full witness outcome"
    );
    outcomes[0]
}
fn expected(environment: &TreeValue) -> Option<bool> {
    let TreeValue::Pair { first, rest } = environment else {
        return None;
    };
    let TreeValue::Pair {
        first: second,
        rest: ending,
    } = rest.as_ref()
    else {
        return None;
    };
    let (TreeValue::Atom { hex: a }, TreeValue::Atom { hex: b }, TreeValue::Atom { hex: nil }) =
        (first.as_ref(), second.as_ref(), ending.as_ref())
    else {
        return None;
    };
    if !nil.is_empty() {
        return None;
    }
    // Only the hexadecimal spelling is case-insensitive; decoded atom bytes
    // and their complete lengths remain distinct.
    Some(!a.eq_ignore_ascii_case(b))
}

#[test]
fn two_atom_contract_has_both_boolean_witnesses_and_all_tree_counterexample_domain() {
    let p = source();
    let result = verify(&p, &atom_spec()).unwrap();
    let Verification::Proven {
        abi_nonvacuity,
        acceptance_nonvacuity,
        true_witness,
        false_witness,
    } = result
    else {
        panic!("exact primitive byte inequality must satisfy the independent specification")
    };
    for witness in [abi_nonvacuity, acceptance_nonvacuity] {
        assert_eq!(
            replay(&p, witness.environment.clone()),
            expected(&witness.environment)
        );
        assert!(expected(&witness.environment).is_some());
    }
    assert_eq!(replay(&p, true_witness.unwrap().environment), Some(true));
    assert_eq!(replay(&p, false_witness.unwrap().environment), Some(false));
    for a in ["", "00", "0000", "01", "0001", "ff", "ffff", "fffe"] {
        for b in ["", "00", "0000", "01", "0001", "ff", "ffff", "fffe"] {
            let env = list(vec![atom(a), atom(b)]);
            assert_eq!(replay(&p, env.clone()), expected(&env));
        }
    }
    let large = "00".repeat(8192);
    for b in [large.clone(), format!("{}01", "00".repeat(8191))] {
        let env = list(vec![atom(&large), atom(&b)]);
        assert_eq!(replay(&p, env.clone()), expected(&env));
    }
    for env in [
        atom(""),
        atom("00"),
        list(vec![atom("")]),
        list(vec![atom(""), atom(""), atom("")]),
        pair(atom(""), pair(atom(""), atom("0000"))),
        list(vec![pair(atom(""), atom("")), atom("")]),
    ] {
        assert_eq!(expected(&env), None);
        assert_eq!(replay(&p, env), None);
    }
}

#[test]
fn output_reject_all_and_coherent_abi_narrowing_mutants_are_refuted_and_replayed() {
    let original = source();
    let mut inverted = original.clone();
    let Expr::Binary { kind, .. } = &mut inverted.body else {
        unreachable!()
    };
    *kind = BinaryOp::Eq;
    let mut reject_all = original.clone();
    reject_all.body = Expr::Reject {
        message: "deliberate incomplete policy".into(),
    };
    let narrowed = program(
        vec![Type::Bytes { len: 1 }, Type::Bytes { len: 1 }],
        unequal(Type::Bytes { len: 1 }),
    );
    for mutant in [inverted, reject_all, narrowed] {
        let Verification::Refuted { counterexample, .. } = verify(&mutant, &atom_spec()).unwrap()
        else {
            panic!("the fixed independent contract must detect the mutant")
        };
        let wanted = expected(&counterexample.environment)
            .expect("all these mutants change valid specified inputs");
        assert_eq!(
            replay(&original, counterexample.environment.clone()),
            Some(wanted)
        );
        assert_ne!(replay(&mutant, counterexample.environment), Some(wanted));
    }
}

#[test]
fn fixed_width_abi_uses_a_solver_checked_stable_zero_candidate() {
    let p = program(
        vec![Type::Bytes { len: 2 }, Type::Bytes { len: 2 }],
        unequal(Type::Bytes { len: 2 }),
    );
    let specification = atom_spec();
    let queries = emit_structural_boolean(&p, &specification).unwrap();
    // The source ABI query admits arbitrary two-byte contents. Witness seeds
    // are solver-checked only after this original unbounded query is SAT.
    assert!(!queries.abi_nonvacuity.contains("(seq.unit #x00)"));
    assert!(!queries.abi_nonvacuity.contains("(assert (= env "));
    let Verification::Refuted { abi_nonvacuity, .. } = verify(&p, &specification).unwrap() else {
        panic!("the narrower ABI must still be refuted against the all-atom contract")
    };
    assert_eq!(
        abi_nonvacuity.environment.serialized_hex().unwrap(),
        "ff820000ff82000080"
    );
    assert_eq!(replay(&p, abi_nonvacuity.environment), Some(false));
}

#[test]
fn a_synthetic_wide_raw_adapter_cannot_hide_improper_or_extra_argument_tails() {
    let checked_atom = |value| Expr::TreeAtomValue {
        value: Box::new(value),
    };
    let first_expr = |value| Expr::TreeFirst {
        value: Box::new(value),
    };
    let rest_expr = |value| Expr::TreeRest {
        value: Box::new(value),
    };
    // This valid raw IR is an explicitly synthetic wider adapter, not a claim
    // that deleting redundant atom guards from the actual source changes it.
    let mut wide = program(
        vec![Type::Tree],
        Expr::Binary {
            kind: BinaryOp::Ne,
            ty: Type::Atom,
            left: Box::new(checked_atom(first_expr(arg(0)))),
            right: Box::new(checked_atom(first_expr(rest_expr(arg(0))))),
        },
    );
    wide.environment = Environment::Raw;
    let Verification::Refuted { counterexample, .. } = verify(&wide, &atom_spec()).unwrap() else {
        panic!("raw adapter ignores the exact outer-tail contract")
    };
    assert_eq!(expected(&counterexample.environment), None);
    assert_eq!(replay(&source(), counterexample.environment.clone()), None);
    assert!(replay(&wide, counterexample.environment).is_some());
    for ending in [atom("00"), atom("0000"), list(vec![atom("01")])] {
        let env = pair(atom("00"), pair(atom("01"), ending));
        assert_eq!(replay(&source(), env.clone()), None);
        assert_eq!(replay(&wide, env), Some(true));
    }
}

#[test]
fn strict_mixed_byte_and_boolean_parameters_require_exact_lengths_and_canonical_bools() {
    let p = program(
        vec![Type::Bool, Type::Bytes { len: 2 }, Type::Atom],
        Expr::If {
            condition: Box::new(arg(0)),
            then_value: Box::new(Expr::Binary {
                kind: BinaryOp::Eq,
                ty: Type::Bytes { len: 2 },
                left: Box::new(arg(1)),
                right: Box::new(Expr::Bytes { hex: "0001".into() }),
            }),
            else_value: Box::new(Expr::AtomIsEmpty {
                value: Box::new(arg(2)),
            }),
        },
    );
    let mut acceptance = proper_arguments(3);
    acceptance.extend([
        P::Any {
            items: vec![
                byte_eq(head(0), literal("")),
                byte_eq(head(0), literal("01")),
            ],
        },
        P::ByteLength {
            value: head(1),
            len: 2,
        },
        P::IsAtom { value: head(2) },
    ]);
    let spec = Spec {
        acceptance: P::All { items: acceptance },
        output: P::Any {
            items: vec![
                P::All {
                    items: vec![
                        byte_eq(head(0), literal("01")),
                        byte_eq(head(1), literal("0001")),
                    ],
                },
                P::All {
                    items: vec![
                        byte_eq(head(0), literal("")),
                        P::ByteLength {
                            value: head(2),
                            len: 0,
                        },
                    ],
                },
            ],
        },
    };
    let Verification::Proven {
        true_witness: Some(t),
        false_witness: Some(f),
        ..
    } = verify(&p, &spec).unwrap()
    else {
        panic!("general mixed decoder must prove both output domains")
    };
    assert_eq!(replay(&p, t.environment), Some(true));
    assert_eq!(replay(&p, f.environment), Some(false));
    for boolean in ["00", "0001", "02", "ff"] {
        assert_eq!(
            replay(&p, list(vec![atom(boolean), atom("0001"), atom("")])),
            None
        );
    }
    for width in ["", "01", "000001"] {
        assert_eq!(
            replay(&p, list(vec![atom("01"), atom(width), atom("")])),
            None
        );
    }
    assert_eq!(
        replay(&p, list(vec![atom(""), atom("ffff"), atom("00")])),
        Some(false)
    );
}

#[test]
fn empty_acceptance_and_undefined_specification_values_never_become_proofs() {
    let p = program(vec![], Expr::Bool { value: false });
    let empty_spec = Spec {
        acceptance: constant(false),
        output: constant(false),
    };
    assert!(
        verify(&p, &empty_spec)
            .unwrap_err()
            .to_string()
            .contains("specified acceptance is empty")
    );
    let spec = Spec {
        acceptance: byte_eq(T::Input, literal("")),
        output: constant(false),
    };
    let Verification::Proven {
        true_witness: None,
        false_witness: Some(_),
        ..
    } = verify(&p, &spec).unwrap()
    else {
        panic!("a general Boolean program may legitimately be constant false")
    };
    let undefined = not(P::IsAtom {
        value: first(T::Input),
    });
    assert!(
        verify(
            &p,
            &Spec {
                acceptance: not(P::IsAtom {
                    value: first(literal(""))
                }),
                output: constant(false)
            }
        )
        .is_err()
    );
    let Verification::Refuted { counterexample, .. } = verify(
        &p,
        &Spec {
            acceptance: spec.acceptance,
            output: undefined,
        },
    )
    .unwrap() else {
        panic!("undefined expected output on accepted nil must be a mismatch")
    };
    assert_eq!(counterexample.environment.serialized_hex().unwrap(), "80");
}

#[test]
fn byte_ordering_remains_unsupported_by_the_structural_proof_model() {
    for kind in [BinaryOp::Lt, BinaryOp::Le, BinaryOp::Gt, BinaryOp::Ge] {
        for ty in [Type::Atom, Type::Bytes { len: 2 }] {
            let comparison = Expr::Binary {
                kind,
                ty: ty.clone(),
                left: Box::new(arg(0)),
                right: Box::new(arg(1)),
            };
            for body in [
                comparison.clone(),
                Expr::If {
                    condition: Box::new(Expr::Bool { value: true }),
                    then_value: Box::new(Expr::Bool { value: false }),
                    else_value: Box::new(comparison),
                },
            ] {
                let p = program(vec![ty.clone(), ty.clone()], body);
                p.validate().expect("byte ordering is valid executable IR");
                assert!(
                    emit_structural_boolean(&p, &atom_spec())
                        .unwrap_err()
                        .to_string()
                        .contains("unsupported expression in structural tree model"),
                    "execution support must not silently become a byte-order proof"
                );
            }
        }
    }
}

#[test]
fn unsupported_source_and_specification_work_fail_closed_and_resources_abort_replay() {
    let p = source();
    for ty in [
        Type::Tree,
        Type::Unit,
        Type::Tuple { items: vec![] },
        Type::Int {
            bits: 8,
            signed: false,
        },
    ] {
        let invalid = program(vec![ty], Expr::Bool { value: false });
        assert!(emit_structural_boolean(&invalid, &atom_spec()).is_err());
    }
    let mut ignored = program(vec![], Expr::Bool { value: false });
    ignored.environment = Environment::Ignored;
    assert!(emit_structural_boolean(&ignored, &atom_spec()).is_err());
    let mut runtime = p.clone();
    runtime.functions.push(Function {
        id: 0,
        name: "unused".into(),
        parameters: vec![],
        result: Type::Unit,
        body: Expr::Unit,
    });
    assert!(
        emit_structural_boolean(&runtime, &atom_spec())
            .unwrap_err()
            .to_string()
            .contains("runtime")
    );
    let mut unsupported = p.clone();
    unsupported.body = Expr::If {
        condition: Box::new(Expr::Bool { value: true }),
        then_value: Box::new(p.body.clone()),
        else_value: Box::new(Expr::Sequence {
            first: Box::new(Expr::Int {
                value: "0".into(),
                ty: Type::Int {
                    bits: 8,
                    signed: false,
                },
            }),
            then_value: Box::new(Expr::Bool { value: false }),
        }),
    };
    assert!(emit_structural_boolean(&unsupported, &atom_spec()).is_err());
    for bad in [
        P::All {
            items: vec![constant(true); 8193],
        },
        P::ByteEqual {
            left: literal(&"00".repeat(4097)),
            right: literal(""),
        },
        P::ByteEqual {
            left: literal("01) (assert false)"),
            right: literal(""),
        },
    ] {
        assert!(
            emit_structural_boolean(
                &p,
                &Spec {
                    acceptance: bad,
                    output: constant(false)
                }
            )
            .is_err()
        );
    }
    let mut deep = constant(true);
    for _ in 0..66 {
        deep = not(deep);
    }
    assert!(
        emit_structural_boolean(
            &p,
            &Spec {
                acceptance: deep,
                output: constant(false)
            }
        )
        .is_err()
    );
    let unavailable = SolverConfig {
        executable: "/no/such/structural-boolean-solver".into(),
        ..SolverConfig::default()
    };
    assert!(verify_structural_boolean(&p, &atom_spec(), &unavailable).is_err());
    let witness = TreeWitness {
        environment: list(vec![atom(""), atom("")]),
    };
    let exhausted = replay_structural_boolean(&p, &witness, 1).unwrap_err();
    let exhausted = exhausted.downcast_ref::<ReplayFailure>().unwrap();
    assert_eq!(exhausted.failure.kind(), FailureKind::Resource);
    assert_eq!(exhausted.failure.code(), FailureCode::CostExceeded);
    assert!(
        serde_json::from_value::<P>(
            serde_json::json!({"predicate":"constant","value":true,"unknown":0})
        )
        .is_err()
    );
}
