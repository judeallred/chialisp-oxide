//! Structural byte-sequence properties for AtomIsEmpty. This model coverage
//! does not admit runtime calls or prove the Rust/CLVM translation correct.
use chialisp_oxide_ir::{Environment, Expr, Function, Parameter, Program, Type};
use chialisp_oxide_verifier::{
    Obligation, Predicate, ReplayOutcome, SolverConfig, Specification, TreeShape,
    TreeSpecification, TreeTerm, TreeValue, TreeVerification, emit, emit_tree, replay_tree,
    verify_tree, verify_tree_equivalence,
};
use std::sync::Mutex;

// Avoid CPU scheduling contention against another solver-heavy test in this
// binary; retain the production wall-clock timeout and failure behavior.
static SOLVER: Mutex<()> = Mutex::new(());
fn atom(hex: &str) -> Expr {
    Expr::Atom { hex: hex.into() }
}
fn tree_atom(hex: &str) -> Expr {
    Expr::TreeAtom {
        value: Box::new(atom(hex)),
    }
}
fn arg() -> Expr {
    Expr::Arg { index: 0 }
}
fn empty(value: Expr) -> Expr {
    Expr::AtomIsEmpty {
        value: Box::new(value),
    }
}
fn atom_value() -> Expr {
    Expr::TreeAtomValue {
        value: Box::new(arg()),
    }
}
fn branch(condition: Expr, yes: Expr, no: Expr) -> Expr {
    Expr::If {
        condition: Box::new(condition),
        then_value: Box::new(yes),
        else_value: Box::new(no),
    }
}
fn reject() -> Expr {
    Expr::Reject {
        message: "atom domain required".into(),
    }
}
fn program(body: Expr) -> Program {
    Program {
        name: "symbolic-atom-empty".into(),
        environment: Environment::Raw,
        parameters: vec![Parameter {
            name: "environment".into(),
            ty: Type::Tree,
        }],
        result: Type::Tree,
        body,
        functions: vec![],
    }
}
fn only_atoms(body: Expr) -> Expr {
    branch(
        Expr::TreeIsPair {
            value: Box::new(arg()),
        },
        reject(),
        body,
    )
}
fn identity() -> Program {
    program(only_atoms(branch(
        empty(atom_value()),
        tree_atom(""),
        arg(),
    )))
}
fn identity_spec() -> TreeSpecification {
    TreeSpecification {
        acceptance: TreeShape::Atom,
        output: TreeTerm::Input,
    }
}
fn proven(result: TreeVerification) {
    let TreeVerification::Proven { cases } = result else {
        panic!("{result:?}")
    };
    assert_eq!(cases.len(), 2);
    assert!(cases.iter().all(|case| case.counterexample.is_none()));
}
fn replay_accepted(
    program: &Program,
    witness: &chialisp_oxide_verifier::TreeWitness,
    expected: &str,
) {
    for row in replay_tree(program, witness, 1_000_000).unwrap() {
        match row.outcome {
            ReplayOutcome::Accepted { output_hex, .. } => assert_eq!(output_hex, expected),
            other => panic!("unexpected replay: {other:?}"),
        }
    }
}

#[test]
fn symbolic_atom_predicate_preserves_arbitrary_bytes_and_refutes_a_truthy_mutation() {
    let _guard = SOLVER.lock().unwrap();
    let p = identity();
    let queries = emit_tree(&p, &identity_spec()).unwrap();
    assert!(queries.atom_counterexample.contains("(seq.len "));
    assert!(queries.atom_counterexample.contains("(Seq (_ BitVec 8))"));
    proven(verify_tree(&p, &identity_spec(), &SolverConfig::default()).unwrap());

    let mutant = program(only_atoms(branch(
        Expr::Bool { value: true },
        tree_atom(""),
        arg(),
    )));
    let TreeVerification::Refuted { cases } =
        verify_tree(&mutant, &identity_spec(), &SolverConfig::default()).unwrap()
    else {
        panic!("dropping the empty check must lose nonempty atom bytes")
    };
    let witness = cases[0].counterexample.clone().unwrap();
    let TreeValue::Atom { hex } = &witness.environment else {
        panic!("atom witness required")
    };
    assert!(
        !hex.is_empty(),
        "only a nonempty atom detects this mutation"
    );
    replay_accepted(&p, &witness, &witness.environment.serialized_hex().unwrap());
    replay_accepted(&mutant, &witness, "80");
}

#[test]
fn symbolic_empty_literal_is_distinct_from_zero_and_padded_byte_atoms() {
    let _guard = SOLVER.lock().unwrap();
    for (payload, expected) in [("", "aa"), ("00", "bb"), ("0000", "bb"), ("0001", "bb")] {
        let p = program(only_atoms(branch(
            empty(atom(payload)),
            tree_atom("aa"),
            tree_atom("bb"),
        )));
        let spec = TreeSpecification {
            acceptance: TreeShape::Atom,
            output: TreeTerm::Atom {
                hex: expected.into(),
            },
        };
        proven(verify_tree(&p, &spec, &SolverConfig::default()).unwrap());
    }
}

#[test]
fn checked_atom_extraction_remains_strict_under_the_symbolic_boolean_predicate() {
    let _guard = SOLVER.lock().unwrap();
    // A checked atom extraction inside the predicate suffices to reject pairs;
    // no explicit outer pair guard is present in the second program.
    let checked = program(branch(empty(atom_value()), tree_atom(""), arg()));
    proven(verify_tree_equivalence(&identity(), &checked, &SolverConfig::default()).unwrap());

    let invalid = program(branch(empty(tree_atom("")), tree_atom(""), arg()));
    assert!(invalid.validate().is_err());
    assert!(emit_tree(&invalid, &identity_spec()).is_err());

    // A bottom-typed operand is valid IR and rejects in the VM, but the
    // structural translator has no value to put through seq.len. It must fail
    // closed rather than invent a successful empty atom in its place.
    let no_value = program(branch(empty(reject()), tree_atom(""), arg()));
    no_value.validate().unwrap();
    assert!(
        emit_tree(&no_value, &identity_spec())
            .unwrap_err()
            .to_string()
            .contains("diverging atom emptiness test")
    );
}

#[test]
fn atom_predicates_do_not_extend_the_scalar_or_runtime_models() {
    let specification = Specification {
        assumptions: Predicate::default(),
        obligation: Obligation::OnSuccess {
            property: Predicate::default(),
        },
    };
    for body in [
        empty(atom("")),
        branch(
            Expr::Bool { value: true },
            Expr::Bool { value: true },
            empty(atom("00")),
        ),
    ] {
        let p = Program {
            name: "scalar-byte-rejection".into(),
            environment: Environment::Strict,
            parameters: vec![],
            result: Type::Bool,
            body,
            functions: vec![],
        };
        p.validate().unwrap();
        assert!(
            emit(&p, &specification)
                .unwrap_err()
                .to_string()
                .contains("scalar SMT does not model tree expressions")
        );
    }
    let mut runtime = identity();
    runtime.functions.push(Function {
        id: 0,
        name: "unused".into(),
        parameters: vec![],
        result: Type::Unit,
        body: Expr::Unit,
    });
    runtime.validate().unwrap();
    assert!(
        emit_tree(&runtime, &identity_spec())
            .unwrap_err()
            .to_string()
            .contains("does not model runtime")
    );
}
