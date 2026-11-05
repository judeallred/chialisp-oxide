//! Raw delegated conditions and separately funded owner-committed instances.
//! Finite source/native/VM/SDK observations are not an authorization theorem.
mod ledger;

use anyhow::{Result, ensure};
use chialisp_oxide::Tree;
use chialisp_oxide_backend_experiment::{FailureCode, Outcome, execute};
use clvmr::{Allocator, allocator::SExp, serde::node_to_bytes};
use serde::Deserialize;
use serde_json::{Value, json};

const SLUG: &str = "p2-delegated-conditions";
const MAX_COST: u64 = 1_000_000;
const NAMED: &[&str] = &[
    "named-key0-conditions0",
    "named-key0-conditions1",
    "named-key0-conditions2",
    "named-key0-conditions6",
    "named-key1-conditions4",
    "named-key2-conditions3",
    "named-key3-conditions7",
    "named-key4-conditions0",
    "named-key5-conditions5",
    "named-key6-conditions8",
    "ignored-outer-tail0",
    "ignored-outer-tail1",
    "ignored-outer-tail2",
    "ignored-outer-tail3",
    "generated-127",
    "missing-projection-0",
    "missing-projection-1",
    "missing-projection-2",
    "missing-projection-3",
    "missing-projection-4",
];
const MODES: &[(&str, bool)] = &[
    ("rust_mir", false),
    ("rust_mir", true),
    ("chialisp", true),
    ("rue_source", false),
    ("rue_source", true),
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Vectors {
    schema_version: u32,
    status: String,
    accepted: Vec<Accepted>,
    rejected: Vec<Rejected>,
    scope: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Accepted {
    label: String,
    solution: String,
    environment_hex: String,
    expected_output_hex: String,
    key_hex: String,
    conditions_hex: String,
    conditions_tree_hash: String,
    outer_tail_hex: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rejected {
    label: String,
    solution: String,
    environment_hex: String,
    expected_failure: String,
}
struct DomainCase {
    label: String,
    solution: String,
    environment_hex: String,
    expected: Outcome,
}
fn two_fields(
    a: &Allocator,
    node: clvmr::NodePtr,
) -> Option<(clvmr::NodePtr, clvmr::NodePtr, clvmr::NodePtr)> {
    let SExp::Pair(key, rest) = a.sexp(node) else {
        return None;
    };
    let SExp::Pair(conditions, tail) = a.sexp(rest) else {
        return None;
    };
    Some((key, conditions, tail))
}
fn vectors() -> Result<Vec<DomainCase>> {
    let path = super::root().join("puzzles/p2-delegated-conditions/independent-vectors.json");
    let file = std::fs::read(path)?;
    ensure!(
        file.len() <= 1024 * 1024,
        "delegated-conditions vector size"
    );
    let spec: Vectors = serde_json::from_slice(&file)?;
    ensure!(
        spec.schema_version == 1
            && spec.status == "independent_canonical_reference_preflight_not_Rust_port"
            && !spec.scope.is_empty()
            && spec.accepted.len() == 195
            && spec.rejected.len() == 5,
        "delegated-conditions independent vector contract"
    );
    let mut cases = Vec::new();
    for row in spec.accepted {
        let mut a = Allocator::new();
        let environment = rue_clvm::assemble(&mut a, &row.solution)?;
        ensure!(
            hex::encode(node_to_bytes(&a, environment)?) == row.environment_hex,
            "independent environment serialization differs"
        );
        let (key, conditions, tail) = two_fields(&a, environment)
            .ok_or_else(|| anyhow::anyhow!("positive independent case lacks two projections"))?;
        ensure!(
            hex::encode(node_to_bytes(&a, key)?) == row.key_hex
                && hex::encode(node_to_bytes(&a, conditions)?) == row.conditions_hex
                && hex::encode(node_to_bytes(&a, tail)?) == row.outer_tail_hex
                && hex::encode(chialisp_oxide_simulator::puzzle_hash(&hex::decode(
                    &row.conditions_hex
                )?)?)
                    == row.conditions_tree_hash,
            "independent input decomposition/hash differs"
        );
        ensure!(
            !row.expected_output_hex.is_empty(),
            "missing independent expected output"
        );
        cases.push(DomainCase {
            label: row.label,
            solution: row.solution,
            environment_hex: row.environment_hex,
            expected: Outcome::Accepted {
                output_hex: row.expected_output_hex,
            },
        });
    }
    for row in spec.rejected {
        let mut a = Allocator::new();
        let environment = rue_clvm::assemble(&mut a, &row.solution)?;
        ensure!(
            row.expected_failure == "path_into_atom"
                && hex::encode(node_to_bytes(&a, environment)?) == row.environment_hex
                && two_fields(&a, environment).is_none(),
            "negative independent case must lack a projection"
        );
        cases.push(DomainCase {
            label: row.label,
            solution: row.solution,
            environment_hex: row.environment_hex,
            expected: Outcome::Rejected,
        });
    }
    let mut names = std::collections::BTreeSet::new();
    ensure!(
        cases.iter().all(|row| names.insert(row.label.clone()))
            && NAMED.len() == 20
            && NAMED.iter().all(|name| names.contains(*name)),
        "duplicate/missing delegated-conditions cases"
    );
    Ok(cases)
}

/// Disjoint from the twenty manifest cases; ordinary raw generation remains on.
pub(super) fn generated_cases() -> Result<Vec<super::Case>> {
    let cases: Vec<_> = vectors()?
        .into_iter()
        .filter(|row| !NAMED.contains(&row.label.as_str()))
        .map(|row| super::Case {
            label: row.label,
            solution: row.solution,
            declared_divergence: None,
        })
        .collect();
    ensure!(
        cases.len() == 180,
        "delegated-conditions generated case count"
    );
    Ok(cases)
}

/// A private native-output resource check, outside the source panic adapter.
/// Input construction already preflights the complete environment, including
/// ignored tails. Inspect both returned trees before allocating encoded output.
pub(super) fn output_preflight(
    value: &chialisp_oxide_puzzles::p2_delegated_conditions::DelegatedConditions<'_>,
) -> Result<()> {
    const MAX_NODES: usize = 32_768;
    const MAX_BYTES: usize = 1_048_640;
    const MAX_DEPTH: usize = 260;
    // Seven fixed pair/atom nodes surround the two borrowed trees; the u64
    // opcode uses at most nine payload bytes and the digest has exactly32.
    let (mut nodes, mut bytes) = (7usize, 41usize);
    let mut pending = vec![(value.first.1, 3usize), (value.rest, 1usize)];
    while let Some((node, depth)) = pending.pop() {
        nodes = nodes
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("native output node overflow"))?;
        ensure!(
            nodes <= MAX_NODES && depth <= MAX_DEPTH,
            "native delegated-conditions output node/depth limit"
        );
        match node {
            Tree::Atom(payload) => {
                bytes = bytes
                    .checked_add(payload.len())
                    .ok_or_else(|| anyhow::anyhow!("native output byte overflow"))?;
                ensure!(
                    bytes <= MAX_BYTES,
                    "native delegated-conditions output byte limit"
                );
            }
            Tree::Pair(first, rest) => pending.extend([(*rest, depth + 1), (*first, depth + 1)]),
        }
    }
    Ok(())
}

pub(super) fn validate(programs: &[(&str, bool, Vec<u8>)]) -> Result<Value> {
    ensure!(
        programs.len() == MODES.len()
            && programs
                .iter()
                .zip(MODES)
                .all(|((backend, optimize, _), mode)| (*backend, *optimize) == *mode),
        "delegated-conditions requires all five exact build identities"
    );
    let cases = vectors()?;
    let mut raw_cases = Vec::new();
    for case in &cases {
        let native = super::expectation(SLUG, &case.solution)?;
        ensure!(
            native == case.expected,
            "native source differs from independent vector {}",
            case.label
        );
        let mut runs = Vec::new();
        for (backend, optimize, code) in programs {
            let execution = execute(code, &case.solution, MAX_COST)?;
            ensure!(
                execution.outcome == case.expected,
                "{} {} independent raw outcome mismatch/inconclusive",
                case.label,
                backend
            );
            if case.expected == Outcome::Rejected {
                ensure!(
                    execution.failure.is_some_and(|failure| matches!(
                        failure.code(),
                        FailureCode::Raise | FailureCode::PathIntoAtom
                    )),
                    "missing projection must be conclusive program shape rejection"
                );
                if *backend == "chialisp" {
                    ensure!(
                        execution
                            .failure
                            .is_some_and(|failure| failure.code() == FailureCode::PathIntoAtom),
                        "canonical shape fault changed"
                    );
                }
            }
            runs.push(json!({"backend":backend,"optimize_lir":optimize,"execution":execution}));
        }
        raw_cases.push(json!({"label":case.label,"solution":case.solution,"environment_hex":case.environment_hex,
            "expected":case.expected,"canonical_failure_code":if case.expected == Outcome::Rejected {Some("path_into_atom")} else {None},
            "native":native,"runs":runs}));
    }
    let mut builds = Vec::new();
    for (index, (backend, optimize, code)) in programs.iter().enumerate() {
        // Canonical chooses RueON; every Rust/Rue build chooses canonical.
        // These fixtures intentionally require different compiled identities.
        let alternate = if index == 2 { 4 } else { 2 };
        let (other_backend, other_optimize, other_code) = &programs[alternate];
        builds.push(ledger::validate(
            backend,
            *optimize,
            code,
            other_backend,
            *other_optimize,
            other_code,
        ));
    }
    Ok(
        json!({"schema_version":1,"protocol":"chia-consensus-0.36.1/p2-delegated-conditions-v1","status":"validated",
        "simulator":"chia-sdk-test 0.36.0","consensus":"chia-consensus 0.36.1","vm":"clvmr 0.16.2","network":"TESTNET11",
        "transaction_attempts":60,"accepted":20,"typed_rejected_with_full_rollback":40,"builds":builds,
        "raw_validation":{"max_cost":MAX_COST,"execution_vm":"clvmr 0.21.0 MEMPOOL_MODE","accepted":195,"rejected":5,
            "native_evaluations":200,"VM_evaluations":1000,"cases":raw_cases,
            "scope":"Additional retained independent200-row native/five-build replay, separate from ordinary20named+180disjoint generated and2059generic raw comparisons. Actual fault codes/text are retained and not normalized. No SDK attempts arise from these raw VM calls."},
        "scope":"Raw template plus separately funded owner-key instances in five builds.60 actual SDK fixture submissions, not60generic ledger groups. Full simulator equality is checked in memory, not serialized as Debug or proved by its recordedBoolean. Unsafe uncurried key selection is intentionally admitted. Source freshness and finite native/VM/SDK checks are not execution authentication, compiler preservation, exhaustive authorization, BLS proof or production readiness."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_and_generated_cover_exact_disjoint_independent_suite() {
        let all = vectors().unwrap();
        let generated = generated_cases().unwrap();
        assert_eq!(all.len(), 200);
        assert_eq!(generated.len(), 180);
        assert!(
            generated
                .iter()
                .all(|row| !NAMED.contains(&row.label.as_str()))
        );
        assert_eq!(
            all.iter()
                .filter(|row| row.expected == Outcome::Rejected)
                .count(),
            5
        );
        let manifest: Value = serde_json::from_slice(
            &std::fs::read(
                super::super::root().join("puzzles/p2-delegated-conditions/puzzle.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            manifest["cases"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["label"].as_str().unwrap())
                .collect::<Vec<_>>(),
            NAMED
        );
    }
    #[test]
    fn actual_native_export_matches_all_independent_raw_vectors() {
        for case in vectors().unwrap() {
            assert_eq!(
                super::super::expectation(SLUG, &case.solution).unwrap(),
                case.expected,
                "{}",
                case.label
            );
        }
    }
    #[test]
    fn ignored_tail_host_resource_error_is_not_a_policy_rejection() {
        let mut tail = "()".to_owned();
        for _ in 0..257 {
            tail = format!("(0x00 . {tail})");
        }
        // The raw source ignores this tail and would return normally. Its host
        // oracle still preflights the entire input outside the panic adapter.
        let error = super::super::native::execute(SLUG, &format!("(() . (() . {tail}))"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("input depth exceeds"), "{error}");
    }
    #[test]
    fn output_resource_error_is_outside_policy_and_handles_cyclic_host_tree() {
        static NIL: Tree<'static> = Tree::Atom(&[]);
        static LOOP: Tree<'static> = Tree::Pair(&NIL, &LOOP);
        let value = chialisp_oxide::Cons {
            first: (50, &NIL, [0; 32]),
            rest: &LOOP,
        };
        assert!(output_preflight(&value).is_err());
    }
}
