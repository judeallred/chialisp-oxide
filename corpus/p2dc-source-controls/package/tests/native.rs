//! Fixed finite source witnesses; no compiler, authorization or state theorem.
use chialisp_oxide::{Cons, Tree};
use chialisp_oxide_p2dc_controls::{
    baseline, reject_ignored_improper_tail, wrong_atom_domain, wrong_conditions_tail,
};
use serde_json::{Value, json};
use std::panic::{AssertUnwindSafe, catch_unwind};

type Policy = for<'a> fn(&'a Tree<'a>) -> Cons<(u64, &'a Tree<'a>, [u8; 32]), &'a Tree<'a>>;
static NIL: Tree<'static> = Tree::Atom(&[]);
static NIL_REST: Tree<'static> = Tree::Pair(&NIL, &NIL);
static NIL_ENV: Tree<'static> = Tree::Pair(&NIL, &NIL_REST);
static KEY: Tree<'static> = Tree::Atom(&[0x42; 48]);
static RECIPIENT: Tree<'static> = Tree::Atom(&[0x42; 32]);
static AMOUNT: Tree<'static> = Tree::Atom(&[0x03, 0xe8]);
static OP: Tree<'static> = Tree::Atom(&[0x33]);
static AMOUNT_TAIL: Tree<'static> = Tree::Pair(&AMOUNT, &NIL);
static DEST_TAIL: Tree<'static> = Tree::Pair(&RECIPIENT, &AMOUNT_TAIL);
static CONDITION: Tree<'static> = Tree::Pair(&OP, &DEST_TAIL);
static CONDITIONS: Tree<'static> = Tree::Pair(&CONDITION, &NIL);
static EIGHT: Tree<'static> = Tree::Atom(&[8]);
static NINE: Tree<'static> = Tree::Atom(&[9]);
static TAIL_REST: Tree<'static> = Tree::Pair(&EIGHT, &NIL);
static TAIL: Tree<'static> = Tree::Pair(&NINE, &TAIL_REST);
static REST: Tree<'static> = Tree::Pair(&CONDITIONS, &TAIL);
static IGNORED_ENV: Tree<'static> = Tree::Pair(&KEY, &REST);

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn atom(payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() < 64, "fixed witness codec only");
    if payload.len() == 1 && payload[0] < 128 {
        return payload.to_vec();
    }
    let mut bytes = vec![0x80 | u8::try_from(payload.len()).unwrap()];
    bytes.extend_from_slice(payload);
    bytes
}
fn pair(first: &[u8], rest: &[u8]) -> Vec<u8> {
    assert!(first.len() + rest.len() < 4096);
    let mut bytes = vec![0xff];
    bytes.extend_from_slice(first);
    bytes.extend_from_slice(rest);
    bytes
}
fn tree(value: &Tree<'_>) -> Vec<u8> {
    // Preflight the complete fixed input/output tree before allocating bytes.
    // Resource/codec assertions remain outside the source-panic adapter.
    let (mut nodes, mut bytes) = (0usize, 0usize);
    let mut work = vec![(value, 0)];
    while let Some((value, depth)) = work.pop() {
        nodes += 1;
        assert!(nodes <= 128 && depth <= 32);
        match value {
            Tree::Atom(payload) => {
                bytes += payload.len();
                assert!(payload.len() < 64 && bytes <= 4096);
            }
            Tree::Pair(first, rest) => work.extend([(*rest, depth + 1), (*first, depth + 1)]),
        }
    }
    let mut result = Vec::new();
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            Tree::Atom(payload) => result.extend(atom(payload)),
            Tree::Pair(first, rest) => {
                result.push(0xff);
                pending.extend([*rest, *first]);
            }
        }
    }
    assert!(result.len() <= 4096);
    result
}
fn observe(policy: Policy, environment: &Tree<'_>) -> Value {
    tree(environment); // Outside catch, including ignored tail.
    let value = catch_unwind(AssertUnwindSafe(|| policy(environment)));
    match value {
        Err(_) => json!({"status":"rejected"}), // Native panic, NOT a VM code.
        Ok(value) => {
            assert_eq!(value.first.0, 50);
            let key = tree(value.first.1);
            let conditions = tree(value.rest);
            let condition = pair(
                &atom(&[50]),
                &pair(&key, &pair(&atom(&value.first.2), &atom(&[]))),
            );
            json!({"status":"accepted","output_hex":hex(&pair(&condition, &conditions))})
        }
    }
}
#[test]
fn actual_same_package_mutants_have_independent_finite_counterexamples() {
    let rows: Value = serde_json::from_str(include_str!("../witnesses.json")).unwrap();
    let policies: [(&str, Policy, &Tree<'_>); 3] = [
        ("wrong_atom_domain", wrong_atom_domain::puzzle, &NIL_ENV),
        (
            "wrong_conditions_tail",
            wrong_conditions_tail::puzzle,
            &NIL_ENV,
        ),
        (
            "reject_ignored_improper_tail",
            reject_ignored_improper_tail::puzzle,
            &IGNORED_ENV,
        ),
    ];
    for ((name, policy, environment), spec) in policies.iter().zip(rows.as_array().unwrap()) {
        assert_eq!(spec["name"], *name);
        assert_eq!(
            hex(&tree(environment)),
            spec["environment_hex"].as_str().unwrap()
        );
        let original = observe(baseline::puzzle, environment);
        let actual = observe(*policy, environment);
        assert_eq!(original, spec["baseline"]);
        assert_eq!(actual, spec["mutant"]);
        assert_ne!(original, actual);
        println!(
            "P2DC_MUTATION_NATIVE {}",
            json!({"name":name,"witness":spec["witness"],"baseline":original,"actual":actual})
        );
    }
}
