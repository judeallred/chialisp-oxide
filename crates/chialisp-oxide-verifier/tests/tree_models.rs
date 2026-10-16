use chialisp_oxide_ir::{Environment, Expr, Parameter, Program, Type};
use chialisp_oxide_verifier::{
    Obligation, Predicate, ReplayOutcome, SolverConfig, Specification, TreeShape,
    TreeSpecification, TreeTerm, TreeValue, TreeVerification, TreeWitness, emit, emit_tree,
    replay_tree, verify_tree, verify_tree_equivalence,
};

fn atom(hex: &str) -> Expr {
    Expr::TreeAtom {
        value: Box::new(Expr::Atom { hex: hex.into() }),
    }
}
fn arg() -> Expr {
    Expr::Arg { index: 0 }
}
fn reject() -> Expr {
    Expr::Reject {
        message: "atom environment".into(),
    }
}
fn quote(value: Expr, prefix: &str) -> Expr {
    Expr::TreePair {
        first: Box::new(atom(prefix)),
        rest: Box::new(value),
    }
}
fn program(body: Expr) -> Program {
    Program {
        name: "tree-test".into(),
        environment: Environment::Raw,
        parameters: vec![Parameter {
            name: "environment".into(),
            ty: Type::Tree,
        }],
        result: Type::Tree,
        body,
    }
}
fn original() -> Program {
    program(Expr::If {
        condition: Box::new(Expr::TreeIsPair {
            value: Box::new(arg()),
        }),
        then_value: Box::new(quote(
            Expr::TreeFirst {
                value: Box::new(arg()),
            },
            "01",
        )),
        else_value: Box::new(reject()),
    })
}
fn specification() -> TreeSpecification {
    TreeSpecification {
        acceptance: TreeShape::Pair,
        output: TreeTerm::Pair {
            first: Box::new(TreeTerm::Atom { hex: "01".into() }),
            rest: Box::new(TreeTerm::First {
                value: Box::new(TreeTerm::Input),
            }),
        },
    }
}
fn value(hex: &str) -> TreeValue {
    TreeValue::Atom { hex: hex.into() }
}
fn pair(first: TreeValue, rest: TreeValue) -> TreeValue {
    TreeValue::Pair {
        first: Box::new(first),
        rest: Box::new(rest),
    }
}
fn expected(environment: &TreeValue) -> Option<TreeValue> {
    match environment {
        TreeValue::Atom { .. } => None,
        TreeValue::Pair { first, .. } => Some(pair(value("01"), first.as_ref().clone())),
    }
}
fn compare_vm(program: &Program, environment: TreeValue, expected: Option<TreeValue>) {
    let witness = TreeWitness { environment };
    for replay in replay_tree(program, &witness, 1_000_000).unwrap() {
        match (&expected, replay.outcome) {
            (None, ReplayOutcome::Rejected { .. }) => (),
            (Some(expected), ReplayOutcome::Accepted { output_hex, cost }) => {
                assert_eq!(output_hex, expected.serialized_hex().unwrap());
                assert!(cost > 0);
            }
            (expected, actual) => panic!(
                "optimize={} expected={expected:?}, actual={actual:?}",
                replay.optimize
            ),
        }
    }
}

#[test]
fn symbolic_atom_pair_cases_are_nonvacuous_and_replay_in_both_vm_modes() {
    let p = original();
    let result = verify_tree(&p, &specification(), &SolverConfig::default()).unwrap();
    let TreeVerification::Proven { cases } = result else {
        panic!("{result:?}")
    };
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0].input_shape, TreeShape::Atom);
    assert_eq!(cases[1].input_shape, TreeShape::Pair);
    for case in cases {
        assert!(case.counterexample.is_none());
        let environment = case.domain_nonvacuity.environment;
        let output = expected(&environment);
        compare_vm(&p, environment, output);
    }
    let atoms = ["", "00", "01", "0001", "80", "ffff", "00000000000000000000"];
    for a in atoms {
        compare_vm(&p, value(a), None);
        for b in atoms {
            let environment = pair(value(a), value(b));
            compare_vm(&p, environment.clone(), expected(&environment));
            let environment = pair(pair(value(a), value(b)), pair(value(b), value(a)));
            compare_vm(&p, environment.clone(), expected(&environment));
        }
    }
}

#[test]
fn actual_guard_optimization_preserves_the_independent_structural_specification() {
    let before = original();
    let after = chialisp_oxide_ir::optimize(&before).unwrap();
    assert_ne!(
        serde_json::to_value(&before.body).unwrap(),
        serde_json::to_value(&after.body).unwrap(),
        "this proof must exercise an actual rewrite"
    );
    for p in [&before, &after] {
        assert!(matches!(
            verify_tree(p, &specification(), &SolverConfig::default()).unwrap(),
            TreeVerification::Proven { .. }
        ));
    }
    assert!(matches!(
        verify_tree_equivalence(&before, &after, &SolverConfig::default()).unwrap(),
        TreeVerification::Proven { .. }
    ));
}

#[test]
fn symbolic_equivalence_detects_unsafe_guard_elision_without_required_projection() {
    let kept = atom("aa");
    let before = program(Expr::If {
        condition: Box::new(Expr::TreeIsPair {
            value: Box::new(arg()),
        }),
        then_value: Box::new(kept.clone()),
        else_value: Box::new(reject()),
    });
    let actual = chialisp_oxide_ir::optimize(&before).unwrap();
    assert!(matches!(
        verify_tree_equivalence(&before, &actual, &SolverConfig::default()).unwrap(),
        TreeVerification::Proven { .. }
    ));
    let unsafe_after = program(kept);
    let TreeVerification::Refuted { cases } =
        verify_tree_equivalence(&before, &unsafe_after, &SolverConfig::default()).unwrap()
    else {
        panic!("removing this guard changes atom rejection to acceptance")
    };
    let witness = cases[0].counterexample.clone().unwrap();
    compare_vm(&before, witness.environment.clone(), None);
    compare_vm(&unsafe_after, witness.environment, Some(value("aa")));
}

#[test]
fn symbolic_equivalence_checks_lazy_and_atom_projection_counterexamples() {
    let kept = [
        Expr::If {
            condition: Box::new(Expr::Bool { value: false }),
            then_value: Box::new(Expr::TreeFirst {
                value: Box::new(arg()),
            }),
            else_value: Box::new(atom("aa")),
        },
        Expr::TreeAtom {
            value: Box::new(Expr::TreeAtomValue {
                value: Box::new(arg()),
            }),
        },
    ];
    for kept in kept {
        let before = program(Expr::If {
            condition: Box::new(Expr::TreeIsPair {
                value: Box::new(arg()),
            }),
            then_value: Box::new(kept.clone()),
            else_value: Box::new(reject()),
        });
        let after = chialisp_oxide_ir::optimize(&before).unwrap();
        assert!(matches!(
            verify_tree_equivalence(&before, &after, &SolverConfig::default()).unwrap(),
            TreeVerification::Proven { .. }
        ));
        let TreeVerification::Refuted { cases } =
            verify_tree_equivalence(&before, &program(kept), &SolverConfig::default()).unwrap()
        else {
            panic!("an unexecuted or atom-only projection cannot require a pair")
        };
        assert!(cases[0].counterexample.is_some());
    }
    let before = program(Expr::If {
        condition: Box::new(Expr::TreeIsPair {
            value: Box::new(arg()),
        }),
        then_value: Box::new(Expr::Sequence {
            first: Box::new(Expr::TreeRest {
                value: Box::new(arg()),
            }),
            then_value: Box::new(atom("aa")),
        }),
        else_value: Box::new(reject()),
    });
    let after = chialisp_oxide_ir::optimize(&before).unwrap();
    assert_ne!(
        serde_json::to_value(&before.body).unwrap(),
        serde_json::to_value(&after.body).unwrap()
    );
    assert!(matches!(
        verify_tree_equivalence(&before, &after, &SolverConfig::default()).unwrap(),
        TreeVerification::Proven { .. }
    ));
}

#[test]
fn atom_acceptance_and_quote_constructor_mutations_are_refuted_and_replayed() {
    let mut guard_mutant = original();
    let Expr::If { else_value, .. } = &mut guard_mutant.body else {
        unreachable!()
    };
    **else_value = atom("");
    let TreeVerification::Refuted { cases } =
        verify_tree(&guard_mutant, &specification(), &SolverConfig::default()).unwrap()
    else {
        panic!("atom acceptance mutation was not detected")
    };
    let witness = cases[0].counterexample.clone().unwrap();
    assert!(matches!(witness.environment, TreeValue::Atom { .. }));
    compare_vm(&original(), witness.environment.clone(), None);
    compare_vm(&guard_mutant, witness.environment, Some(value("")));

    let constructor_mutant = program(Expr::If {
        condition: Box::new(Expr::TreeIsPair {
            value: Box::new(arg()),
        }),
        then_value: Box::new(quote(
            Expr::TreeFirst {
                value: Box::new(arg()),
            },
            "02",
        )),
        else_value: Box::new(reject()),
    });
    let TreeVerification::Refuted { cases } = verify_tree(
        &constructor_mutant,
        &specification(),
        &SolverConfig::default(),
    )
    .unwrap() else {
        panic!("constructor mutation was not detected")
    };
    let witness = cases[1].counterexample.clone().unwrap();
    let TreeVerification::Refuted { cases } =
        verify_tree_equivalence(&original(), &constructor_mutant, &SolverConfig::default())
            .unwrap()
    else {
        panic!("unchanged acceptance with a changed output must not be equivalent")
    };
    assert!(cases[0].counterexample.is_none());
    assert!(cases[1].counterexample.is_some());
    let TreeValue::Pair { first, .. } = &witness.environment else {
        panic!("pair witness expected")
    };
    compare_vm(
        &original(),
        witness.environment.clone(),
        expected(&witness.environment),
    );
    compare_vm(
        &constructor_mutant,
        witness.environment.clone(),
        Some(pair(value("02"), first.as_ref().clone())),
    );
}

#[test]
fn checked_atom_projection_preserves_every_byte_and_rejects_pairs() {
    let p = program(Expr::If {
        condition: Box::new(Expr::TreeIsPair {
            value: Box::new(arg()),
        }),
        then_value: Box::new(reject()),
        else_value: Box::new(Expr::TreeAtom {
            value: Box::new(Expr::TreeAtomValue {
                value: Box::new(arg()),
            }),
        }),
    });
    let spec = TreeSpecification {
        acceptance: TreeShape::Atom,
        output: TreeTerm::Input,
    };
    assert!(matches!(
        verify_tree(&p, &spec, &SolverConfig::default()).unwrap(),
        TreeVerification::Proven { .. }
    ));
    for bytes in ["", "00", "0001", "01", "ff", "0080"] {
        compare_vm(&p, value(bytes), Some(value(bytes)));
    }
    compare_vm(&p, pair(value(""), value("01")), None);
}

#[test]
fn discarded_projection_and_pair_siblings_remain_strict_while_if_is_lazy() {
    let failing = Expr::TreeFirst {
        value: Box::new(atom("")),
    };
    for body in [
        Expr::Sequence {
            first: Box::new(failing.clone()),
            then_value: Box::new(atom("01")),
        },
        Expr::TreePair {
            first: Box::new(atom("01")),
            rest: Box::new(failing.clone()),
        },
    ] {
        let p = program(body);
        let TreeVerification::Refuted { cases } =
            verify_tree(&p, &specification(), &SolverConfig::default()).unwrap()
        else {
            panic!("false success must be refuted")
        };
        assert!(cases[1].counterexample.is_some());
        compare_vm(&p, pair(value("01"), value("0001")), None);
    }
    let p = program(Expr::If {
        condition: Box::new(Expr::Bool { value: true }),
        then_value: Box::new(original().body),
        else_value: Box::new(failing),
    });
    assert!(matches!(
        verify_tree(&p, &specification(), &SolverConfig::default()).unwrap(),
        TreeVerification::Proven { .. }
    ));
    let input = pair(value("0001"), value("ff"));
    compare_vm(&p, input.clone(), expected(&input));
}

#[test]
fn source_specification_projection_errors_do_not_become_vacuous_success() {
    let p = program(arg());
    let spec = TreeSpecification {
        acceptance: TreeShape::Atom,
        output: TreeTerm::First {
            value: Box::new(TreeTerm::Input),
        },
    };
    let TreeVerification::Refuted { cases } =
        verify_tree(&p, &spec, &SolverConfig::default()).unwrap()
    else {
        panic!("undefined accepted specification must be refuted")
    };
    assert!(cases.iter().all(|case| case.counterexample.is_some()));
}

#[test]
fn scalar_proofs_reject_tree_signatures_and_even_unreachable_tree_operations() {
    let scalar = Specification {
        assumptions: Predicate::default(),
        obligation: Obligation::OnSuccess {
            property: Predicate::default(),
        },
    };
    assert!(
        emit(&original(), &scalar)
            .unwrap_err()
            .to_string()
            .contains("scalar SMT")
    );
    let integer = Expr::Int {
        value: "0".into(),
        ty: Type::Int {
            bits: 8,
            signed: false,
        },
    };
    let p = Program {
        name: "unreachable-tree".into(),
        environment: Environment::Strict,
        parameters: vec![],
        result: Type::Int {
            bits: 8,
            signed: false,
        },
        body: Expr::If {
            condition: Box::new(Expr::Bool { value: true }),
            then_value: Box::new(integer.clone()),
            else_value: Box::new(Expr::Sequence {
                first: Box::new(atom("01")),
                then_value: Box::new(integer),
            }),
        },
    };
    p.validate().unwrap();
    assert!(
        emit(&p, &scalar)
            .unwrap_err()
            .to_string()
            .contains("tree expressions")
    );
}

#[test]
fn structural_requests_fail_closed_on_unsupported_ir_specs_witnesses_and_solver_absence() {
    let mut p = original();
    p.body = Expr::Sequence {
        first: Box::new(Expr::Int {
            value: "0".into(),
            ty: Type::Int {
                bits: 8,
                signed: false,
            },
        }),
        then_value: Box::new(p.body),
    };
    assert!(
        emit_tree(&p, &specification())
            .unwrap_err()
            .to_string()
            .contains("unsupported expression")
    );
    let mut spec = specification();
    spec.output = TreeTerm::Atom {
        hex: "01) (assert false)".into(),
    };
    assert!(emit_tree(&original(), &spec).is_err());
    let unavailable = SolverConfig {
        executable: "/no/such/tree-solver".into(),
        ..SolverConfig::default()
    };
    assert!(verify_tree(&original(), &specification(), &unavailable).is_err());
    let bad = TreeWitness {
        environment: value("0) (x)"),
    };
    assert!(bad.environment.solution().is_err());
    assert!(replay_tree(&original(), &bad, 1_000_000).is_err());
    assert!(
        replay_tree(
            &original(),
            &TreeWitness {
                environment: pair(value(""), value(""))
            },
            0
        )
        .is_err()
    );
    assert!(
        replay_tree(
            &original(),
            &TreeWitness {
                environment: pair(value(""), value(""))
            },
            1
        )
        .unwrap_err()
        .to_string()
        .contains("exceeded cost limit")
    );
}

#[test]
fn raw_replay_uses_the_entire_tree_without_a_typed_argument_list_wrapper() {
    let p = program(arg());
    for input in [value(""), value("00"), pair(value("0001"), value("ff"))] {
        compare_vm(&p, input.clone(), Some(input));
    }
}

#[test]
fn witness_limits_count_total_atom_bytes_before_rendering_or_allocation() {
    let payload = "00".repeat(600_000);
    let oversized = pair(value(&payload), value(&payload));
    assert!(
        oversized
            .solution()
            .unwrap_err()
            .to_string()
            .contains("total byte limit")
    );
    assert!(
        oversized
            .serialized_hex()
            .unwrap_err()
            .to_string()
            .contains("total byte limit")
    );
}
