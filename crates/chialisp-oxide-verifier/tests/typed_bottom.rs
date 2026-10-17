//! Values under rejecting operands are unobservable. Typed placeholders must
//! retain strict failure and must never excuse unsupported sibling semantics.
use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Parameter, Program, Type};
use chialisp_oxide_verifier::{
    Comparison, Obligation, Predicate, ReplayOutcome, SolverConfig, Specification,
    StructuralBooleanVerification, Term, TreeBooleanSpecification, TreePredicate, TreeTerm,
    Verification, Witness, WitnessValue, emit, emit_structural_boolean, replay,
    replay_structural_boolean, verify, verify_structural_boolean,
};
use chialisp_oxide_vm::FailureCode;
use std::sync::Mutex;

static SOLVER: Mutex<()> = Mutex::new(());
fn reject() -> Expr {
    Expr::Reject {
        message: "direct bottom".into(),
    }
}
fn boolean(value: bool) -> Expr {
    Expr::Bool { value }
}
fn not(value: Expr) -> Expr {
    Expr::Not {
        value: Box::new(value),
    }
}
fn integer(ty: &Type, value: &str) -> Expr {
    Expr::Int {
        value: value.into(),
        ty: ty.clone(),
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
fn sequence(first: Expr, then_value: Expr) -> Expr {
    Expr::Sequence {
        first: Box::new(first),
        then_value: Box::new(then_value),
    }
}
fn program(parameters: Vec<Type>, result: Type, body: Expr) -> Program {
    Program {
        name: "typed-bottom-model".into(),
        environment: Environment::Strict,
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
        functions: vec![],
    }
}
fn accept(predicate: Predicate) -> Specification {
    Specification {
        assumptions: Predicate::default(),
        obligation: Obligation::AcceptanceIff { predicate },
    }
}
fn rejected(p: &Program, witness: &Witness) {
    for row in replay(p, witness, 1_000_000).unwrap() {
        let ReplayOutcome::Rejected { failure, .. } = row.outcome else {
            panic!("bottom became a successful placeholder")
        };
        assert_eq!(failure.code(), FailureCode::Raise);
    }
}

#[test]
fn scalar_direct_bottom_operands_and_not_are_refuted_against_total_acceptance() {
    let _guard = SOLVER.lock().unwrap();
    let i8 = Type::Int {
        bits: 8,
        signed: true,
    };
    let u128 = Type::Int {
        bits: 128,
        signed: false,
    };
    let mut programs = vec![program(vec![], Type::Bool, not(reject()))];
    for op in [
        BinaryOp::Bitand,
        BinaryOp::Bitor,
        BinaryOp::Bitxor,
        BinaryOp::Eq,
        BinaryOp::Ne,
    ] {
        for (left, right) in [
            (reject(), boolean(op == BinaryOp::Bitor)),
            (boolean(false), reject()),
            (reject(), reject()),
        ] {
            programs.push(program(
                vec![],
                Type::Bool,
                binary(op, &Type::Bool, left, right),
            ));
        }
    }
    for (ty, op) in [
        (i8.clone(), BinaryOp::Add),
        (i8.clone(), BinaryOp::Eq),
        (i8.clone(), BinaryOp::Bitor),
        (u128, BinaryOp::Bitxor),
    ] {
        let result = if op == BinaryOp::Eq {
            Type::Bool
        } else {
            ty.clone()
        };
        programs.push(program(
            vec![],
            result,
            binary(op, &ty, reject(), integer(&ty, "0")),
        ));
        let result = if op == BinaryOp::Eq {
            Type::Bool
        } else {
            ty.clone()
        };
        programs.push(program(vec![], result, binary(op, &ty, reject(), reject())));
    }
    for p in programs {
        p.validate().unwrap();
        let result = verify(&p, &accept(Predicate::default()), &SolverConfig::default()).unwrap();
        let Verification::Refuted {
            counterexample,
            nonvacuity: None,
        } = result
        else {
            panic!("always-reject program was hidden: {result:?}")
        };
        rejected(&p, &counterexample);
        let invariant = Specification {
            assumptions: Predicate::default(),
            obligation: Obligation::OnSuccess {
                property: Predicate::Constant { value: false },
            },
        };
        assert_eq!(
            verify(&p, &invariant, &SolverConfig::default()).unwrap(),
            Verification::Vacuous
        );
    }
}

#[test]
fn lazy_successful_branch_survives_a_strict_bottom_branch_in_scalar_and_structural_models() {
    let _guard = SOLVER.lock().unwrap();
    for op in [BinaryOp::Bitand, BinaryOp::Bitor, BinaryOp::Bitxor] {
        let strict = binary(op, &Type::Bool, boolean(op == BinaryOp::Bitor), reject());
        let p = program(
            vec![Type::Bool],
            Type::Bool,
            Expr::If {
                condition: Box::new(Expr::Arg { index: 0 }),
                then_value: Box::new(strict),
                else_value: Box::new(not(boolean(false))),
            },
        );
        let condition = Predicate::Compare {
            relation: Comparison::Eq,
            left: Term::Input {
                argument: 0,
                fields: vec![],
            },
            right: Term::Bool { value: false },
        };
        let Verification::Proven { nonvacuity } =
            verify(&p, &accept(condition), &SolverConfig::default()).unwrap()
        else {
            panic!("incorrect scalar acceptance")
        };
        assert_eq!(
            nonvacuity.arguments,
            vec![WitnessValue::Bool { value: false }]
        );
        for row in replay(&p, &nonvacuity, 1_000_000).unwrap() {
            let ReplayOutcome::Accepted { output_hex, .. } = row.outcome else {
                panic!("lazy branch rejected")
            };
            assert_eq!(output_hex, "01");
        }
        rejected(
            &p,
            &Witness {
                arguments: vec![WitnessValue::Bool { value: true }],
            },
        );

        let spec = TreeBooleanSpecification {
            acceptance: TreePredicate::All {
                items: vec![
                    TreePredicate::IsPair {
                        value: TreeTerm::Input,
                    },
                    TreePredicate::ByteEqual {
                        left: TreeTerm::First {
                            value: Box::new(TreeTerm::Input),
                        },
                        right: TreeTerm::Atom { hex: "".into() },
                    },
                    TreePredicate::ByteEqual {
                        left: TreeTerm::Rest {
                            value: Box::new(TreeTerm::Input),
                        },
                        right: TreeTerm::Atom { hex: "".into() },
                    },
                ],
            },
            output: TreePredicate::Constant { value: true },
        };
        let result = verify_structural_boolean(&p, &spec, &SolverConfig::default()).unwrap();
        let StructuralBooleanVerification::Proven {
            acceptance_nonvacuity,
            ..
        } = result
        else {
            panic!("{result:?}")
        };
        for row in replay_structural_boolean(&p, &acceptance_nonvacuity, 1_000_000).unwrap() {
            let ReplayOutcome::Accepted { output_hex, .. } = row.outcome else {
                panic!("structural witness rejected")
            };
            assert_eq!(output_hex, "01");
        }
        for mutation in [
            not(reject()),
            binary(BinaryOp::Eq, &Type::Atom, reject(), reject()),
            binary(BinaryOp::Ne, &Type::Bytes { len: 32 }, reject(), reject()),
        ] {
            let mut mutant = p.clone();
            mutant.body = mutation;
            let result =
                verify_structural_boolean(&mutant, &spec, &SolverConfig::default()).unwrap();
            let StructuralBooleanVerification::Refuted { counterexample, .. } = result else {
                panic!("{result:?}")
            };
            for row in replay_structural_boolean(&mutant, &counterexample, 1_000_000).unwrap() {
                let ReplayOutcome::Rejected { failure, .. } = row.outcome else {
                    panic!("structural bottom became accepted")
                };
                assert_eq!(failure.code(), FailureCode::Raise);
            }
        }
    }
}

#[test]
fn bottom_cannot_short_circuit_model_capability_checks_in_the_other_operand() {
    let i8 = Type::Int {
        bits: 8,
        signed: true,
    };
    let scalar_spec = accept(Predicate::default());
    let structural_spec = TreeBooleanSpecification {
        acceptance: TreePredicate::Constant { value: true },
        output: TreePredicate::Constant { value: true },
    };
    let unsupported_scalar = sequence(Expr::Atom { hex: "00".into() }, boolean(true));
    let unsupported_structural = sequence(integer(&i8, "0"), boolean(true));
    for (left, right) in [
        (reject(), unsupported_scalar.clone()),
        (unsupported_scalar, reject()),
    ] {
        let p = program(
            vec![],
            Type::Bool,
            binary(BinaryOp::Bitand, &Type::Bool, left, right),
        );
        p.validate().unwrap();
        assert!(emit(&p, &scalar_spec).is_err());
    }
    for (left, right) in [
        (reject(), unsupported_structural.clone()),
        (unsupported_structural, reject()),
    ] {
        let p = program(
            vec![],
            Type::Bool,
            binary(BinaryOp::Eq, &Type::Bool, left, right),
        );
        p.validate().unwrap();
        assert!(emit_structural_boolean(&p, &structural_spec).is_err());
    }
    let p = program(
        vec![],
        Type::Bool,
        binary(BinaryOp::Eq, &i8, reject(), reject()),
    );
    p.validate().unwrap();
    assert!(emit_structural_boolean(&p, &structural_spec).is_err());
    let p = program(
        vec![],
        Type::Bool,
        binary(BinaryOp::Eq, &Type::Bytes { len: 32 }, reject(), reject()),
    );
    p.validate().unwrap();
    assert!(emit(&p, &scalar_spec).is_err());
}
