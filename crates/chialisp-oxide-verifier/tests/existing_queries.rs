//! Existing executed proof artifacts remain byte-identical when their program
//! uses no new operation. This is query compatibility, not a new proof result.
use chialisp_oxide_ir::Program;
use chialisp_oxide_verifier::{
    Specification, TreeBooleanSpecification, TreeQueries, TreeSpecification, emit,
    emit_structural_boolean, emit_tree, emit_tree_equivalence,
};
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

fn proofs(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../proofs")
        .join(name)
}
fn load<T: DeserializeOwned>(path: impl AsRef<Path>) -> T {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn exact(path: impl AsRef<Path>, actual: &str) {
    let path = path.as_ref();
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        actual,
        "changed retained query {}",
        path.display()
    );
}

#[test]
fn all_existing_scalar_queries_keep_their_exact_headers_symbols_and_obligations() {
    let directory = proofs("bounded-counter");
    for (program, specification, filename) in [
        ("program", "range", "range"),
        ("program", "acceptance", "acceptance"),
        ("mutant", "range", "mutation"),
    ] {
        let program: Program = load(directory.join(format!("{program}.ir.json")));
        let spec: Specification = load(directory.join(format!("{specification}.spec.json")));
        let queries = emit(&program, &spec).unwrap();
        let actual = format!(
            "; Nonvacuity query\n{}\n(reset)\n; Counterexample query\n{}",
            queries.nonvacuity, queries.counterexample
        );
        exact(
            directory.join(format!("{filename}.smt2")),
            &format!("{}\n", actual.trim_end()),
        );
    }
}

fn tree_set(directory: &Path, prefix: &str, queries: TreeQueries) {
    for (name, text) in [
        ("atom-domain", queries.atom_nonvacuity),
        ("atom-case", queries.atom_counterexample),
        ("pair-domain", queries.pair_nonvacuity),
        ("pair-case", queries.pair_counterexample),
    ] {
        exact(directory.join(format!("{prefix}{name}.smt2")), &text);
    }
}

#[test]
fn all_existing_structural_queries_keep_their_exact_text() {
    let directory = proofs("p2-conditions");
    let spec: TreeSpecification = load(directory.join("specification.json"));
    let before: Program = load(directory.join("program.ir.json"));
    let after: Program = load(directory.join("optimized.ir.json"));
    tree_set(&directory, "", emit_tree(&before, &spec).unwrap());
    tree_set(&directory, "optimized-", emit_tree(&after, &spec).unwrap());
    tree_set(
        &directory,
        "equivalence-",
        emit_tree_equivalence(&before, &after).unwrap(),
    );
    let accept_atom = load(directory.join("accept-atom.ir.json"));
    exact(
        directory.join("accept-atom.smt2"),
        &emit_tree(&accept_atom, &spec).unwrap().atom_counterexample,
    );
    let wrong_quote = load(directory.join("wrong-quote.ir.json"));
    exact(
        directory.join("wrong-quote.smt2"),
        &emit_tree(&wrong_quote, &spec).unwrap().pair_counterexample,
    );
    let unsafe_before = load(directory.join("unsafe-guard-before.ir.json"));
    let unsafe_after = load(directory.join("unsafe-guard-after.ir.json"));
    exact(
        directory.join("unsafe-guard.smt2"),
        &emit_tree_equivalence(&unsafe_before, &unsafe_after)
            .unwrap()
            .atom_counterexample,
    );

    let directory = proofs("atom-not-equal");
    let spec: TreeBooleanSpecification = load(directory.join("specification.json"));
    for model in [
        "original",
        "operator_eq",
        "self_inequality",
        "constant_true",
        "constant_false",
        "swap_operands",
        "fixed_length_narrowing",
        "loose_environment",
    ] {
        let program = load(directory.join(format!("{model}.ir.json")));
        let queries = emit_structural_boolean(&program, &spec).unwrap();
        for (name, text) in [
            ("abi_nonvacuity", queries.abi_nonvacuity),
            ("acceptance_nonvacuity", queries.acceptance_nonvacuity),
            ("true_nonvacuity", queries.true_nonvacuity),
            ("false_nonvacuity", queries.false_nonvacuity),
            ("counterexample", queries.counterexample),
        ] {
            exact(directory.join(format!("{model}-{name}.smt2")), &text);
        }
    }
}
