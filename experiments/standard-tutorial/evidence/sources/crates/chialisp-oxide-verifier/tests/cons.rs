//! Typed wire pairs are executable IR, not an admitted mathematical model.
use chialisp_oxide_ir::{Environment, Expr, Function, Program, Type};
use chialisp_oxide_verifier::{
    Obligation, Predicate, Specification, TreeBooleanSpecification, TreePredicate, TreeShape,
    TreeSpecification, TreeTerm, emit, emit_acyclic_scalar, emit_structural_boolean, emit_tree,
    emit_tree_equivalence,
};

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
        output: TreePredicate::Constant { value: true },
    }
}
fn fixture() -> Program {
    Program {
        name: "unsupported-pair-model".into(),
        parameters: vec![],
        result: Type::Bool,
        environment: Environment::Strict,
        functions: vec![],
        body: Expr::Let {
            local_id: 0,
            ty: Type::Pair {
                first: Box::new(Type::Bool),
                rest: Box::new(Type::Tree),
            },
            value: Box::new(Expr::Pair {
                first: Box::new(Expr::Bool { value: true }),
                rest: Box::new(Expr::TreeAtom {
                    value: Box::new(Expr::Atom { hex: "00".into() }),
                }),
            }),
            body: Box::new(Expr::Bool { value: true }),
        },
    }
}
fn rejected(p: &Program) {
    p.validate().unwrap();
    let errors = [
        emit(p, &scalar_spec()).unwrap_err(),
        emit_acyclic_scalar(p, &scalar_spec()).unwrap_err(),
        emit_tree(p, &tree_spec()).unwrap_err(),
        emit_structural_boolean(p, &boolean_spec()).unwrap_err(),
        emit_tree_equivalence(p, p).unwrap_err(),
    ];
    for error in errors {
        assert!(error.to_string().contains("typed wire pairs"), "{error:#}");
    }
}

#[test]
fn every_model_rejects_even_a_discarded_executable_pair() {
    rejected(&fixture());
}

#[test]
fn unused_function_pair_signature_cannot_evade_model_preflight() {
    let mut p = fixture();
    p.body = Expr::Bool { value: true };
    p.functions.push(Function {
        id: 0,
        name: "unused".into(),
        parameters: vec![],
        result: Type::Tuple {
            items: vec![Type::Pair {
                first: Box::new(Type::Bool),
                rest: Box::new(Type::Tree),
            }],
        },
        body: Expr::Reject {
            message: "unused".into(),
        },
    });
    rejected(&p);
}
