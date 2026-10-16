//! The existing successful-value/rejection models do not model runtime calls,
//! lexical bindings or divergence. Even unreachable runtime IR must fail closed.
use chialisp_oxide_ir::{Environment, Expr, Function, Parameter, Program, Type};
use chialisp_oxide_verifier::{
    Obligation, Predicate, ReplayOutcome, SolverConfig, Specification, TreeShape,
    TreeSpecification, TreeTerm, TreeValue, TreeWitness, Witness, WitnessValue, emit, emit_tree,
    emit_tree_equivalence, replay, replay_tree, verify, verify_tree, verify_tree_equivalence,
};
use std::{path::PathBuf, time::Duration};

fn scalar_type() -> Type {
    Type::Int {
        bits: 8,
        signed: true,
    }
}
fn program(tree: bool) -> Program {
    let ty = if tree { Type::Tree } else { scalar_type() };
    Program {
        name: "runtime-boundary".into(),
        parameters: vec![Parameter {
            name: "value".into(),
            ty: ty.clone(),
        }],
        result: ty,
        body: Expr::Arg { index: 0 },
        environment: if tree {
            Environment::Raw
        } else {
            Environment::Strict
        },
        functions: vec![],
    }
}
fn function(program: &Program, recursive: bool) -> Function {
    Function {
        id: 0,
        name: "identity".into(),
        parameters: program.parameters.clone(),
        result: program.result.clone(),
        body: if recursive {
            Expr::Call {
                function_id: 0,
                arguments: vec![Expr::Arg { index: 0 }],
            }
        } else {
            Expr::Arg { index: 0 }
        },
    }
}
fn binding(program: &Program, uses_local: bool) -> Expr {
    Expr::Let {
        local_id: 0,
        ty: program.result.clone(),
        value: Box::new(Expr::Arg { index: 0 }),
        body: Box::new(if uses_local {
            Expr::Local { local_id: 0 }
        } else {
            Expr::Arg { index: 0 }
        }),
    }
}
fn runtime_cases(tree: bool) -> Vec<(&'static str, Program)> {
    let base = program(tree);
    let mut table = base.clone();
    table.functions.push(function(&base, false));
    let mut direct = table.clone();
    direct.body = Expr::Call {
        function_id: 0,
        arguments: vec![Expr::Arg { index: 0 }],
    };
    let mut dead_call = table.clone();
    dead_call.body = Expr::If {
        condition: Box::new(Expr::Bool { value: false }),
        then_value: Box::new(direct.body.clone()),
        else_value: Box::new(base.body.clone()),
    };
    let mut unused_recursive = base.clone();
    unused_recursive.functions.push(function(&base, true));
    let mut unused_binding = base.clone();
    unused_binding.body = binding(&base, false);
    let mut local = base.clone();
    local.body = binding(&base, true);
    let mut dead_binding = base.clone();
    dead_binding.body = Expr::If {
        condition: Box::new(Expr::Bool { value: true }),
        then_value: Box::new(base.body.clone()),
        else_value: Box::new(binding(&base, true)),
    };
    vec![
        ("unused function table", table),
        ("direct call", direct),
        ("unreachable call", dead_call),
        ("unused recursive function", unused_recursive),
        ("unused let", unused_binding),
        ("let/local with empty table", local),
        ("unreachable let/local", dead_binding),
    ]
}
fn scalar_specification() -> Specification {
    Specification {
        assumptions: Predicate::default(),
        obligation: Obligation::OnSuccess {
            property: Predicate::default(),
        },
    }
}
fn tree_specification() -> TreeSpecification {
    TreeSpecification {
        acceptance: TreeShape::Pair,
        output: TreeTerm::Input,
    }
}
fn no_solver() -> SolverConfig {
    // If an unsupported program reaches solver invocation, these assertions must
    // fail on the diagnostic: no external executable is needed for these tests.
    SolverConfig {
        executable: PathBuf::from("/nonexistent/chialisp-oxide-runtime-model-solver"),
        timeout: Duration::from_millis(1),
    }
}
fn runtime_error<T>(result: anyhow::Result<T>, label: &str) {
    let error = match result {
        Ok(_) => panic!("{label}: runtime IR was silently modeled"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("does not model runtime"),
        "{label}: {error:#}"
    );
}

#[test]
fn scalar_model_rejects_all_runtime_forms_before_solver_invocation() {
    let specification = scalar_specification();
    assert!(emit(&program(false), &specification).is_ok());
    for (label, source) in runtime_cases(false) {
        source.validate().unwrap();
        assert!(source.uses_runtime(), "{label}");
        runtime_error(emit(&source, &specification), label);
        runtime_error(verify(&source, &specification, &no_solver()), label);
    }
}

#[test]
fn structural_models_reject_runtime_on_either_side_before_solver_invocation() {
    let base = program(true);
    let specification = tree_specification();
    assert!(emit_tree(&base, &specification).is_ok());
    assert!(emit_tree_equivalence(&base, &base).is_ok());
    for (label, source) in runtime_cases(true) {
        source.validate().unwrap();
        runtime_error(emit_tree(&source, &specification), label);
        runtime_error(verify_tree(&source, &specification, &no_solver()), label);
        for (before, after) in [(&source, &base), (&base, &source)] {
            runtime_error(emit_tree_equivalence(before, after), label);
            runtime_error(verify_tree_equivalence(before, after, &no_solver()), label);
        }
    }
}

#[test]
fn runtime_v4_roundtrips_without_turning_into_expression_only_proof_input() {
    for tree in [false, true] {
        for (label, source) in runtime_cases(tree) {
            let wire = serde_json::to_value(&source).unwrap();
            assert_eq!(wire["ir_version"], 4, "{label}");
            assert!(wire["program"]["functions"].is_array(), "{label}");
            let decoded: Program = serde_json::from_value(wire).unwrap();
            assert!(decoded.uses_runtime(), "{label}");
            if tree {
                runtime_error(emit_tree(&decoded, &tree_specification()), label);
            } else {
                runtime_error(emit(&decoded, &scalar_specification()), label);
            }
        }
    }
}

#[test]
fn malformed_free_local_and_unknown_callee_are_not_proof_inputs() {
    for tree in [false, true] {
        for body in [
            Expr::Local { local_id: 0 },
            Expr::Call {
                function_id: 99,
                arguments: vec![Expr::Arg { index: 0 }],
            },
        ] {
            let mut source = program(tree);
            source.body = body;
            assert!(source.uses_runtime());
            assert!(source.validate().is_err());
            assert!(emit(&source, &scalar_specification()).is_err());
            assert!(emit_tree(&source, &tree_specification()).is_err());
            assert!(emit_tree_equivalence(&source, &program(true)).is_err());
            if tree {
                assert!(
                    replay_tree(
                        &source,
                        &TreeWitness {
                            environment: TreeValue::Atom { hex: String::new() }
                        },
                        1_000_000
                    )
                    .is_err()
                );
            } else {
                assert!(
                    replay(
                        &source,
                        &Witness {
                            arguments: vec![WitnessValue::Int { value: "7".into() }]
                        },
                        1_000_000
                    )
                    .is_err()
                );
            }
        }
    }
}

fn accepted(rows: Vec<chialisp_oxide_verifier::Replay>, expected: &str) {
    assert_eq!(rows.len(), 2);
    for row in rows {
        match row.outcome {
            ReplayOutcome::Accepted { output_hex, cost } => {
                assert_eq!(output_hex, expected);
                assert!(cost > 0);
            }
            other => panic!("finite runtime witness failed: {other:?}"),
        }
    }
}

#[test]
fn finite_runtime_replay_is_execution_evidence_without_model_acceptance() {
    for (label, source) in runtime_cases(false) {
        runtime_error(emit(&source, &scalar_specification()), label);
        accepted(
            replay(
                &source,
                &Witness {
                    arguments: vec![WitnessValue::Int { value: "7".into() }],
                },
                1_000_000,
            )
            .unwrap(),
            "07",
        );
    }
    let environment = TreeValue::Pair {
        first: Box::new(TreeValue::Atom { hex: "00".into() }),
        rest: Box::new(TreeValue::Atom { hex: "0001".into() }),
    };
    let witness = TreeWitness { environment };
    for (label, source) in runtime_cases(true) {
        runtime_error(emit_tree(&source, &tree_specification()), label);
        accepted(
            replay_tree(&source, &witness, 1_000_000).unwrap(),
            "ff00820001",
        );
    }
}
