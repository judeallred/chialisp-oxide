//! Optional output execution does not admit any existing proof model.
use chialisp_oxide_ir::{Environment, Expr, Function, Program, Type};
use chialisp_oxide_verifier::{
    Obligation, Predicate, SolverConfig, Specification, TreeBooleanSpecification, TreePredicate,
    TreeShape, TreeSpecification, TreeTerm, emit, emit_acyclic_scalar, emit_structural_boolean,
    emit_tree, emit_tree_equivalence, verify, verify_acyclic_scalar, verify_structural_boolean,
    verify_tree, verify_tree_equivalence,
};
use std::{path::PathBuf, time::Duration};

fn optional() -> Type {
    Type::Optional {
        item: Box::new(Type::Bool),
    }
}
fn fixture() -> Program {
    Program {
        name: "unsupported-optional-model".into(),
        parameters: vec![],
        result: Type::Bool,
        environment: Environment::Strict,
        functions: vec![],
        body: Expr::Let {
            local_id: 0,
            ty: optional(),
            value: Box::new(Expr::OptionalSome {
                value: Box::new(Expr::Bool { value: true }),
            }),
            body: Box::new(Expr::Bool { value: true }),
        },
    }
}
fn rejected(source: &Program) {
    source.validate().unwrap();
    let scalar = Specification {
        assumptions: Predicate::default(),
        obligation: Obligation::OnSuccess {
            property: Predicate::default(),
        },
    };
    let tree = TreeSpecification {
        acceptance: TreeShape::Pair,
        output: TreeTerm::Input,
    };
    let boolean = TreeBooleanSpecification {
        acceptance: TreePredicate::Constant { value: true },
        output: TreePredicate::Constant { value: true },
    };
    let no_solver = SolverConfig {
        executable: PathBuf::from("/nonexistent/chialisp-oxide-optional-model-solver"),
        timeout: Duration::from_millis(1),
    };
    let errors = [
        emit(source, &scalar).unwrap_err(),
        emit_acyclic_scalar(source, &scalar).unwrap_err(),
        emit_tree(source, &tree).unwrap_err(),
        emit_structural_boolean(source, &boolean).unwrap_err(),
        emit_tree_equivalence(source, source).unwrap_err(),
        verify(source, &scalar, &no_solver).unwrap_err(),
        verify_acyclic_scalar(source, &scalar, &no_solver).unwrap_err(),
        verify_tree(source, &tree, &no_solver).unwrap_err(),
        verify_structural_boolean(source, &boolean, &no_solver).unwrap_err(),
        verify_tree_equivalence(source, source, &no_solver).unwrap_err(),
    ];
    for error in errors {
        assert!(error.to_string().contains("optional outputs"), "{error:#}");
    }
}

#[test]
fn every_model_rejects_discarded_and_dead_optional_values() {
    rejected(&fixture());
    let mut source = fixture();
    source.body = Expr::If {
        condition: Box::new(Expr::Bool { value: true }),
        then_value: Box::new(Expr::Bool { value: true }),
        else_value: Box::new(source.body),
    };
    rejected(&source);
}

#[test]
fn unused_function_optional_signature_cannot_evade_preflight() {
    let mut source = fixture();
    source.body = Expr::Bool { value: true };
    source.functions.push(Function {
        id: 0,
        name: "unused".into(),
        parameters: vec![],
        result: optional(),
        body: Expr::OptionalNone { item: Type::Bool },
    });
    rejected(&source);
}
