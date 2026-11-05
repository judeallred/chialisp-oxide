//! Every existing proof API stays closed to retained callable capability.
use chialisp_oxide_ir::{Environment, Expr, Function, Parameter, Program, Type};
use chialisp_oxide_verifier::{
    Obligation, Predicate, SolverConfig, Specification, TreeBooleanSpecification, TreePredicate,
    TreeShape, TreeSpecification, TreeTerm, emit, emit_acyclic_scalar, emit_structural_boolean,
    emit_tree, emit_tree_equivalence, verify, verify_acyclic_scalar, verify_structural_boolean,
    verify_tree, verify_tree_equivalence,
};
use std::{path::PathBuf, time::Duration};
fn callable() -> Type {
    Type::Callable {
        parameters: vec![],
        result: Box::new(Type::Bool),
    }
}
fn fixture() -> Program {
    Program {
        name: "unsupported-callable-model".into(),
        parameters: vec![],
        result: Type::Bool,
        environment: Environment::Strict,
        functions: vec![Function {
            id: 0,
            name: "target".into(),
            parameters: vec![],
            result: Type::Bool,
            body: Expr::Bool { value: true },
        }],
        body: Expr::Let {
            local_id: 0,
            ty: callable(),
            value: Box::new(Expr::FunctionRef { function_id: 0 }),
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
        executable: PathBuf::from("/nonexistent/chialisp-oxide-callable-model-solver"),
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
        assert!(error.to_string().contains("callable"), "{error:#}");
    }
}

#[test]
fn every_model_rejects_discarded_dead_and_unused_callable_capability() {
    rejected(&fixture());
    let mut source = fixture();
    source.body = Expr::If {
        condition: Box::new(Expr::Bool { value: true }),
        then_value: Box::new(Expr::Bool { value: true }),
        else_value: Box::new(source.body),
    };
    rejected(&source);
    source.body = Expr::Bool { value: true };
    source.functions.push(Function {
        id: 1,
        name: "unused".into(),
        parameters: vec![Parameter {
            name: "callback".into(),
            ty: callable(),
        }],
        result: Type::Bool,
        body: Expr::Bool { value: true },
    });
    rejected(&source);
}
