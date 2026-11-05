//! SHA256 is an executable primitive boundary, not a modeled cryptographic
//! function. None of these tests introduces hash/collision/injectivity axioms.
use chialisp_oxide_ir::{Environment, Expr, Function, Parameter, Program, Type};
use chialisp_oxide_verifier::{
    Obligation, Predicate, SolverConfig, Specification, TreeBooleanSpecification, TreePredicate,
    TreeShape, TreeSpecification, TreeTerm, emit, emit_structural_boolean, emit_tree,
    emit_tree_equivalence, verify, verify_structural_boolean, verify_tree, verify_tree_equivalence,
};
use serde::de::DeserializeOwned;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

fn atom() -> Expr {
    Expr::Atom { hex: "00ff".into() }
}
fn hash() -> Expr {
    Expr::Sha256 {
        parts: vec![atom()],
    }
}
fn sequence(first: Expr, then_value: Expr) -> Expr {
    Expr::Sequence {
        first: Box::new(first),
        then_value: Box::new(then_value),
    }
}
fn reject() -> Expr {
    Expr::Reject {
        message: "before hash".into(),
    }
}
fn program(tree: bool) -> Program {
    Program {
        name: "unmodeled-hash".into(),
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
    live.body = sequence(hash(), base.body.clone());
    let mut empty = base.clone();
    empty.body = sequence(Expr::Sha256 { parts: vec![] }, base.body.clone());
    let mut maximum = base.clone();
    maximum.body = sequence(
        Expr::Sha256 {
            parts: vec![atom(); 64],
        },
        base.body.clone(),
    );
    let mut bottom = base.clone();
    bottom.body = sequence(
        Expr::Sha256 {
            parts: vec![reject(), atom()],
        },
        base.body.clone(),
    );
    let mut dead = base.clone();
    dead.body = Expr::If {
        condition: Box::new(Expr::Bool { value: true }),
        then_value: Box::new(base.body.clone()),
        else_value: Box::new(live.body.clone()),
    };
    let mut nested = base.clone();
    nested.body = sequence(
        Expr::Sha256 {
            parts: vec![hash()],
        },
        base.body.clone(),
    );
    let mut after_reject = base.clone();
    after_reject.body = sequence(reject(), live.body.clone());
    let mut discarded_tuple = base.clone();
    discarded_tuple.body = Expr::Field {
        value: Box::new(Expr::Tuple {
            items: vec![hash(), base.body.clone()],
        }),
        index: 1,
    };
    let mut unused_local = base.clone();
    unused_local.body = Expr::Let {
        local_id: 7,
        ty: Type::Bytes { len: 32 },
        value: Box::new(hash()),
        body: Box::new(base.body.clone()),
    };
    let mut unused_function = base.clone();
    unused_function.functions.push(Function {
        id: 0,
        name: "unused_hash".into(),
        parameters: vec![],
        result: Type::Bytes { len: 32 },
        body: hash(),
    });
    let mut unused_dead = unused_function.clone();
    unused_dead.functions[0].body = Expr::If {
        condition: Box::new(Expr::Bool { value: false }),
        then_value: Box::new(hash()),
        else_value: Box::new(Expr::Bytes {
            hex: "00".repeat(32),
        }),
    };
    vec![
        ("live discarded hash", live),
        ("empty input hash", empty),
        ("maximum arity hash", maximum),
        ("bottom hash operand", bottom),
        ("unreachable branch", dead),
        ("nested hash operand", nested),
        ("hash following Reject", after_reject),
        ("discarded tuple sibling", discarded_tuple),
        ("unused Let initializer", unused_local),
        ("unused function", unused_function),
        ("dead branch in unused function", unused_dead),
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
        executable: PathBuf::from("/nonexistent/chialisp-oxide-sha-model-solver"),
        timeout: Duration::from_millis(1),
    }
}
fn unsupported<T>(result: anyhow::Result<T>, label: &str) {
    let error = match result {
        Ok(_) => panic!("{label}: SHA256 silently gained proof semantics"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("does not model SHA256 semantics"),
        "{label}: {error:#}"
    );
}
fn check_source(program: &Program, label: &str) {
    program.validate().unwrap();
    assert!(program.uses_sha256(), "{label}");
    assert!(program.uses_runtime(), "{label}");
    let wire = serde_json::to_value(program).unwrap();
    assert_eq!(wire["ir_version"], 4, "{label}");
    let decoded: Program = serde_json::from_value(wire).unwrap();
    assert!(decoded.uses_sha256() && decoded.uses_runtime(), "{label}");
}

#[test]
fn scalar_apis_reject_hash_before_solver_even_when_its_result_cannot_be_observed() {
    for (label, source) in cases(false) {
        check_source(&source, label);
        unsupported(emit(&source, &scalar_spec()), label);
        unsupported(verify(&source, &scalar_spec(), &no_solver()), label);
    }
}

#[test]
fn tree_and_equivalence_apis_reject_hash_on_either_side_before_solver() {
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
fn structural_boolean_apis_reject_hash_including_dead_and_unused_definitions() {
    for (label, source) in cases(false) {
        check_source(&source, label);
        unsupported(emit_structural_boolean(&source, &boolean_spec()), label);
        unsupported(
            verify_structural_boolean(&source, &boolean_spec(), &no_solver()),
            label,
        );
    }
}

fn invalid<T>(result: anyhow::Result<T>, label: &str) {
    let error = match result {
        Ok(_) => panic!("{label}: malformed hash IR was accepted"),
        Err(error) => error,
    };
    let diagnostic = format!("{error:#}");
    assert!(diagnostic.contains("invalid"), "{label}: {diagnostic}");
    assert!(
        !diagnostic.contains("does not model SHA256"),
        "{label}: malformed parts bypassed validation: {diagnostic}"
    );
}

#[test]
fn malformed_hash_parts_are_validated_before_the_unsupported_model_boundary() {
    let bad_parts = [
        Expr::Bool { value: false },
        Expr::Int {
            value: "0".into(),
            ty: Type::Int {
                bits: 8,
                signed: false,
            },
        },
        Expr::TreeAtom {
            value: Box::new(atom()),
        },
        Expr::Tuple {
            items: vec![atom()],
        },
        Expr::Unit,
    ];
    let mut malformed = Vec::new();
    for bad in bad_parts {
        let mut source = program(true);
        // A bottom operand must not conceal a later concrete wrong type.
        source.body = sequence(
            Expr::Sha256 {
                parts: vec![reject(), bad.clone()],
            },
            source.body,
        );
        malformed.push(source);
        let mut unused = program(true);
        unused.functions.push(Function {
            id: 0,
            name: "unused_bad_hash".into(),
            parameters: vec![],
            result: Type::Bytes { len: 32 },
            body: Expr::If {
                condition: Box::new(Expr::Bool { value: false }),
                then_value: Box::new(Expr::Sha256 { parts: vec![bad] }),
                else_value: Box::new(reject()),
            },
        });
        malformed.push(unused);
    }
    let mut wide = program(true);
    wide.body = sequence(
        Expr::Sha256 {
            parts: vec![atom(); 65],
        },
        wide.body,
    );
    malformed.push(wide);
    let base = program(true);
    for (index, source) in malformed.iter().enumerate() {
        let label = format!("malformed hash {index}");
        assert!(source.validate().is_err(), "{label}");
        invalid(emit(source, &scalar_spec()), &label);
        invalid(verify(source, &scalar_spec(), &no_solver()), &label);
        invalid(emit_tree(source, &tree_spec()), &label);
        invalid(verify_tree(source, &tree_spec(), &no_solver()), &label);
        for (before, after) in [(source, &base), (&base, source)] {
            invalid(emit_tree_equivalence(before, after), &label);
            invalid(verify_tree_equivalence(before, after, &no_solver()), &label);
        }
        invalid(emit_structural_boolean(source, &boolean_spec()), &label);
        invalid(
            verify_structural_boolean(source, &boolean_spec(), &no_solver()),
            &label,
        );
    }
}

fn load<T: DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn exact(path: &Path, text: &str) {
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        text,
        "{}",
        path.display()
    );
}

#[test]
fn nonhash_positive_controls_retain_existing_query_text() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../proofs");
    let scalar = root.join("bounded-counter");
    let source: Program = load(&scalar.join("program.ir.json"));
    assert!(!source.uses_sha256() && !source.uses_runtime());
    let queries = emit(&source, &load(&scalar.join("range.spec.json"))).unwrap();
    let combined = format!(
        "; Nonvacuity query\n{}\n(reset)\n; Counterexample query\n{}",
        queries.nonvacuity, queries.counterexample
    );
    exact(
        &scalar.join("range.smt2"),
        &format!("{}\n", combined.trim_end()),
    );
    let tree = root.join("p2-conditions");
    let source: Program = load(&tree.join("program.ir.json"));
    assert!(!source.uses_sha256() && !source.uses_runtime());
    let queries = emit_tree(&source, &load(&tree.join("specification.json"))).unwrap();
    let equivalence =
        emit_tree_equivalence(&source, &load(&tree.join("optimized.ir.json"))).unwrap();
    for (prefix, queries) in [("", queries), ("equivalence-", equivalence)] {
        for (suffix, text) in [
            ("atom-domain", queries.atom_nonvacuity),
            ("atom-case", queries.atom_counterexample),
            ("pair-domain", queries.pair_nonvacuity),
            ("pair-case", queries.pair_counterexample),
        ] {
            exact(&tree.join(format!("{prefix}{suffix}.smt2")), &text);
        }
    }
    let boolean = root.join("atom-not-equal");
    let source: Program = load(&boolean.join("original.ir.json"));
    assert!(!source.uses_sha256() && !source.uses_runtime());
    let queries =
        emit_structural_boolean(&source, &load(&boolean.join("specification.json"))).unwrap();
    for (name, text) in [
        ("abi_nonvacuity", queries.abi_nonvacuity),
        ("acceptance_nonvacuity", queries.acceptance_nonvacuity),
        ("true_nonvacuity", queries.true_nonvacuity),
        ("false_nonvacuity", queries.false_nonvacuity),
        ("counterexample", queries.counterexample),
    ] {
        exact(&boolean.join(format!("original-{name}.smt2")), &text);
    }
}
