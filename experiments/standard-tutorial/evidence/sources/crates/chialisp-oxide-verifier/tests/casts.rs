//! Integer casts remain outside every current model, including dead/table casts.
use chialisp_oxide_ir::{Environment, Expr, Function, IntegerCast, Program, Type};
use chialisp_oxide_verifier::{
    Obligation, Predicate, SolverConfig, Specification, TreeBooleanSpecification, TreePredicate,
    TreeShape, TreeSpecification, TreeTerm, emit, emit_acyclic_scalar, emit_structural_boolean,
    emit_tree, emit_tree_equivalence, verify, verify_acyclic_scalar, verify_structural_boolean,
    verify_tree, verify_tree_equivalence,
};
use std::{path::PathBuf, time::Duration};

fn integer() -> Type {
    Type::Int {
        bits: 8,
        signed: false,
    }
}
fn cast() -> Expr {
    Expr::IntCast(IntegerCast {
        value: Box::new(Expr::Int {
            value: "255".into(),
            ty: integer(),
        }),
        source: integer(),
        target: Type::Int {
            bits: 8,
            signed: true,
        },
    })
}
fn fixture() -> Program {
    Program {
        name: "unsupported-cast-model".into(),
        parameters: vec![],
        result: Type::Bool,
        environment: Environment::Strict,
        functions: vec![],
        body: Expr::Sequence {
            first: Box::new(cast()),
            then_value: Box::new(Expr::Bool { value: true }),
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
        executable: PathBuf::from("/nonexistent/chialisp-oxide-cast-model-solver"),
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
        assert!(error.to_string().contains("integer casts"), "{error:#}");
    }
}

#[test]
fn all_model_entry_points_reject_casts_even_when_dead_or_unused() {
    let base = fixture();
    rejected(&base);
    let mut lazy = base.clone();
    lazy.body = Expr::If {
        condition: Box::new(Expr::Bool { value: true }),
        then_value: Box::new(Expr::Bool { value: true }),
        else_value: Box::new(base.body.clone()),
    };
    rejected(&lazy);
    let mut binding = base.clone();
    binding.body = Expr::Let {
        local_id: 0,
        ty: Type::Int {
            bits: 8,
            signed: true,
        },
        value: Box::new(cast()),
        body: Box::new(Expr::Bool { value: true }),
    };
    rejected(&binding);
    let mut table = base;
    table.body = Expr::Bool { value: true };
    table.functions = vec![Function {
        id: 0,
        name: "unused-cast".into(),
        parameters: vec![],
        result: Type::Int {
            bits: 8,
            signed: true,
        },
        body: cast(),
    }];
    rejected(&table);
}
