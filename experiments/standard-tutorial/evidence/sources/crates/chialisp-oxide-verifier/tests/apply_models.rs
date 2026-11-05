//! Every proof API rejects dynamic application before touching a solver,
//! including in dead branches, unused bindings and function-table entries.
//! No model substitutes an uninterpreted or deterministic application.
use chialisp_oxide_ir::{Environment, Expr, Function, Parameter, Program, Type};
use chialisp_oxide_verifier::{
    Obligation, Predicate, SolverConfig, Specification, TreeBooleanSpecification, TreePredicate,
    TreeShape, TreeSpecification, TreeTerm, emit, emit_structural_boolean, emit_tree,
    emit_tree_equivalence, verify, verify_structural_boolean, verify_tree, verify_tree_equivalence,
};
use std::{path::PathBuf, time::Duration};

fn tree_atom() -> Expr {
    Expr::TreeAtom {
        value: Box::new(Expr::Atom { hex: "07".into() }),
    }
}
fn apply(program: Expr, environment: Expr) -> Expr {
    Expr::Apply {
        program: Box::new(program),
        environment: Box::new(environment),
    }
}
fn sequence(first: Expr, then_value: Expr) -> Expr {
    Expr::Sequence {
        first: Box::new(first),
        then_value: Box::new(then_value),
    }
}
fn program(tree: bool) -> Program {
    Program {
        name: "unmodeled-apply".into(),
        parameters: vec![Parameter {
            name: "input".into(),
            ty: if tree { Type::Tree } else { Type::Bool },
        }],
        result: if tree { Type::Tree } else { Type::Bool },
        body: Expr::Arg { index: 0 },
        environment: if tree {
            Environment::Raw
        } else {
            Environment::Strict
        },
        functions: vec![],
    }
}
fn cases(tree: bool) -> Vec<(&'static str, Program)> {
    let base = program(tree);
    let mut live = base.clone();
    live.body = sequence(apply(tree_atom(), tree_atom()), base.body.clone());
    let mut dead = base.clone();
    dead.body = Expr::If {
        condition: Box::new(Expr::Bool { value: false }),
        then_value: Box::new(sequence(apply(tree_atom(), tree_atom()), base.body.clone())),
        else_value: Box::new(base.body.clone()),
    };
    let mut unused = base.clone();
    unused.body = Expr::Let {
        local_id: 0,
        ty: Type::Tree,
        value: Box::new(apply(tree_atom(), tree_atom())),
        body: Box::new(base.body.clone()),
    };
    let mut table = base.clone();
    table.functions = vec![Function {
        id: 0,
        name: "unused-helper".into(),
        parameters: vec![],
        result: Type::Tree,
        body: apply(tree_atom(), tree_atom()),
    }];
    vec![
        ("live", live),
        ("dead-branch", dead),
        ("unused-binding", unused),
        ("unused-function", table),
    ]
}
fn scalar_spec() -> Specification {
    Specification {
        assumptions: Predicate::default(),
        obligation: Obligation::OnSuccess {
            property: Predicate::default(),
        },
    }
}
fn tree_spec() -> TreeSpecification {
    TreeSpecification {
        acceptance: TreeShape::Pair,
        output: TreeTerm::Input,
    }
}
fn boolean_spec() -> TreeBooleanSpecification {
    TreeBooleanSpecification {
        acceptance: TreePredicate::Constant { value: true },
        output: TreePredicate::Constant { value: false },
    }
}
fn no_solver() -> SolverConfig {
    SolverConfig {
        executable: PathBuf::from("/nonexistent/chialisp-oxide-apply-model-solver"),
        timeout: Duration::from_millis(1),
    }
}
fn unsupported<T>(result: anyhow::Result<T>, label: &str) {
    let error = match result {
        Ok(_) => panic!("{label}: dynamic application silently gained proof semantics"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("does not model dynamic application"),
        "{label}: {error:#}"
    );
}
fn check_source(program: &Program, label: &str) {
    program.validate().unwrap();
    assert!(program.uses_apply(), "{label}");
    assert!(program.uses_runtime(), "{label}");
    let wire = serde_json::to_value(program).unwrap();
    assert_eq!(wire["ir_version"], 4, "{label}");
    let decoded: Program = serde_json::from_value(wire).unwrap();
    assert!(decoded.uses_apply() && decoded.uses_runtime(), "{label}");
}

#[test]
fn scalar_apis_reject_apply_before_solver_even_in_dead_or_unused_positions() {
    for (label, source) in cases(false) {
        check_source(&source, label);
        unsupported(emit(&source, &scalar_spec()), label);
        unsupported(verify(&source, &scalar_spec(), &no_solver()), label);
    }
}

#[test]
fn tree_and_equivalence_apis_reject_apply_on_either_side_before_solver() {
    let base = program(true);
    for (label, source) in cases(true) {
        check_source(&source, label);
        unsupported(emit_tree(&source, &tree_spec()), label);
        unsupported(verify_tree(&source, &tree_spec(), &no_solver()), label);
        for (before, after) in [(&source, &base), (&base, &source)] {
            unsupported(emit_tree_equivalence(before, after), label);
            unsupported(verify_tree_equivalence(before, after, &no_solver()), label);
        }
    }
}

#[test]
fn structural_boolean_apis_reject_apply_including_dead_and_unused_definitions() {
    for (label, source) in cases(false) {
        check_source(&source, label);
        unsupported(emit_structural_boolean(&source, &boolean_spec()), label);
        unsupported(
            verify_structural_boolean(&source, &boolean_spec(), &no_solver()),
            label,
        );
    }
}
