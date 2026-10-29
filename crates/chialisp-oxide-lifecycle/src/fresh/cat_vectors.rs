//! Independent raw-environment oracle and actual paired-library native calls.
//! Host codec bounds are inconclusive errors outside the policy panic adapter.
use super::super::*;
use chialisp_oxide::Tree;
use chialisp_oxide_vm::{FailureCode, FailureKind};
use clvmr::{
    Allocator, NodePtr,
    allocator::SExp,
    serde::{node_from_bytes, node_to_bytes},
};
use serde::{Deserialize, Serialize};
use std::cell::OnceCell;

const MAX_INPUT: usize = 32 * 1024;
const MAX_NODES: usize = 1024;
const MAX_DEPTH: usize = 64;
const MAX_ATOMS: usize = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Outcome {
    Accepted { output_hex: String },
    Rejected,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FailureSpec {
    kind: FailureKind,
    code: FailureCode,
    scope: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Case {
    pub name: String,
    pub environment_hex: String,
    pub expected: Outcome,
    why: String,
    reference_failure_specification: Option<FailureSpec>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    protocol: String,
    status: String,
    source_role: String,
    scope: String,
    cases: Vec<Case>,
}
#[derive(Serialize)]
pub(super) struct NativeObservation {
    pub name: String,
    pub actual: Outcome,
    pub expected: Outcome,
    pub reference_failure: Option<FailureCode>,
}
#[derive(Serialize)]
pub(super) struct VmObservation {
    pub lane: String,
    pub name: String,
    pub cost: Option<u64>,
    pub outcome: super::super::VmOutcome,
}

fn input_limits() -> Limits {
    Limits {
        max_blob_bytes: MAX_INPUT,
        max_expanded_bytes: MAX_INPUT,
        max_nodes: MAX_NODES,
        max_depth: MAX_DEPTH,
        max_atom_bytes: MAX_ATOMS,
        max_atom_heap_bytes: 64 * 1024,
        max_work: 1024 * 1024,
        ..Limits::default()
    }
}
fn decode(raw: &[u8]) -> Result<(Allocator, NodePtr)> {
    inspect_artifact(raw, Encoding::Plain, &input_limits())?;
    let mut a = Allocator::new_limited(4 * 1024 * 1024);
    let n = node_from_bytes(&mut a, raw)?;
    Ok((a, n))
}
fn path(
    a: &Allocator,
    mut n: NodePtr,
    mut number: u32,
) -> std::result::Result<NodePtr, FailureCode> {
    while number > 1 {
        let SExp::Pair(left, right) = a.sexp(n) else {
            return Err(FailureCode::PathIntoAtom);
        };
        n = if number & 1 == 0 { left } else { right };
        number >>= 1;
    }
    Ok(n)
}
/// Specify actual canonical paths and byte equality; never call native policy.
fn oracle(a: &Allocator, n: NodePtr) -> std::result::Result<(), FailureCode> {
    let delta = path(a, n, 47)?;
    if !matches!(a.sexp(delta), SExp::Atom) || !a.atom(delta).as_ref().is_empty() {
        return Err(FailureCode::Raise);
    }
    let genesis = path(a, n, 2)?;
    let parent = path(a, n, 45)?;
    if !matches!(a.sexp(genesis), SExp::Atom) || !matches!(a.sexp(parent), SExp::Atom) {
        return Err(FailureCode::InvalidOpArg);
    }
    if a.atom(genesis).as_ref() != a.atom(parent).as_ref() {
        return Err(FailureCode::Raise);
    }
    Ok(())
}
pub(super) fn cases() -> Result<Vec<Case>> {
    let raw = include_bytes!("../../../../experiments/cat-genesis-tail/vectors.json");
    ensure!(raw.len() <= 128 * 1024, "bounded CAT vector source");
    let document: Document = serde_json::from_slice(raw)?;
    ensure!(
        document.protocol == "chialisp-oxide-cat-genesis-tail-vectors/v1"
            && document.status == "specified_not_executed"
            && document.source_role
                == "raw whole-environment translation of unchanged canonical genesis_by_coin_id"
            && document.scope.len() <= 1024
            && document.cases.len() == 37,
        "exact declared CAT vector source"
    );
    let mut names = std::collections::BTreeSet::new();
    let mut total = 0;
    for case in &document.cases {
        ensure!(
            !case.name.is_empty()
                && case.name.len() <= 96
                && case.why.len() <= 512
                && names.insert(&case.name),
            "unique bounded CAT vector"
        );
        ensure!(
            case.environment_hex.len() <= MAX_INPUT * 2
                && case.environment_hex.len().is_multiple_of(2),
            "CAT vector byte precharge"
        );
        total += case.environment_hex.len() / 2;
        ensure!(total <= 512 * 1024, "complete CAT vector byte bound");
        let raw = hex::decode(&case.environment_hex)?;
        let (a, n) = decode(&raw)?;
        let expected = oracle(&a, n);
        match (
            &case.expected,
            &case.reference_failure_specification,
            expected,
        ) {
            (Outcome::Accepted { output_hex }, None, Ok(())) => {
                ensure!(output_hex == "80", "exact nil TAIL output")
            }
            (Outcome::Rejected, Some(spec), Err(code)) => ensure!(
                spec.kind == FailureKind::Program && spec.code == code && spec.scope.len() <= 512,
                "independent canonical failure specification"
            ),
            _ => anyhow::bail!("CAT vector differs from independent whole-environment oracle"),
        }
    }
    Ok(document.cases)
}
enum Flat {
    Atom(usize),
    Pair(usize, usize),
}
enum Task {
    Visit(NodePtr, usize),
    Pair,
}
fn native(a: &Allocator, n: NodePtr) -> Result<Outcome> {
    // The serialized complete input was admitted before allocator conversion.
    // Recheck expanded dimensions before allocating any native Atom/Pair data.
    let mut tasks = vec![Task::Visit(n, 0)];
    let mut values = Vec::with_capacity(MAX_NODES);
    let mut flat = Vec::with_capacity(MAX_NODES);
    let mut atoms = Vec::with_capacity(MAX_NODES);
    let mut nodes = 0;
    let mut atom_bytes = 0;
    while let Some(task) = tasks.pop() {
        match task {
            Task::Visit(node, depth) => {
                nodes += 1;
                ensure!(
                    nodes <= MAX_NODES && depth <= MAX_DEPTH,
                    "native CAT complete-tree host bound"
                );
                match a.sexp(node) {
                    SExp::Atom => {
                        let atom = a.atom(node);
                        atom_bytes += atom.as_ref().len();
                        ensure!(
                            atom_bytes <= MAX_ATOMS,
                            "native CAT aggregate atom byte bound"
                        );
                        let id = atoms.len();
                        atoms.push(atom.as_ref().to_vec());
                        values.push(flat.len());
                        flat.push(Flat::Atom(id));
                    }
                    SExp::Pair(left, right) => {
                        ensure!(
                            tasks.len() + 3 <= MAX_NODES * 3,
                            "native CAT traversal work bound"
                        );
                        tasks.push(Task::Pair);
                        tasks.push(Task::Visit(right, depth + 1));
                        tasks.push(Task::Visit(left, depth + 1));
                    }
                }
            }
            Task::Pair => {
                let right = values.pop().context("native right child")?;
                let left = values.pop().context("native left child")?;
                values.push(flat.len());
                flat.push(Flat::Pair(left, right));
            }
        }
    }
    ensure!(
        values.len() == 1 && flat.len() <= MAX_NODES,
        "complete native CAT postorder"
    );
    // Preallocated OnceCells provide stable immutable references without unsafe
    // self-referential storage or a new arena dependency. No Vec grows here.
    let cells: Vec<OnceCell<Tree<'_>>> = (0..flat.len()).map(|_| OnceCell::new()).collect();
    for (index, node) in flat.iter().enumerate() {
        let value = match node {
            Flat::Atom(atom) => Tree::Atom(&atoms[*atom]),
            Flat::Pair(left, right) => Tree::Pair(
                cells[*left].get().context("native first initialized")?,
                cells[*right].get().context("native rest initialized")?,
            ),
        };
        cells[index]
            .set(value)
            .map_err(|_| anyhow::anyhow!("duplicate native cell"))?;
    }
    let environment = cells[values[0]]
        .get()
        .context("native complete environment")?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        chialisp_oxide_puzzles::cat_genesis_by_coin_id::puzzle(environment)
    }));
    match result {
        Ok(Tree::Atom([])) => Ok(Outcome::Accepted {
            output_hex: "80".into(),
        }),
        Ok(_) => anyhow::bail!("actual paired Rust TAIL returned unexpected output"),
        Err(_) => Ok(Outcome::Rejected),
    }
}
pub(super) fn native_vectors(cases: &[Case]) -> Result<Vec<NativeObservation>> {
    let mut result = vec![];
    for case in cases {
        let raw = hex::decode(&case.environment_hex)?;
        let (a, n) = decode(&raw)?;
        let actual = native(&a, n)?;
        ensure!(
            actual == case.expected,
            "actual paired Cargo Rust TAIL native output differs: {}",
            case.name
        );
        result.push(NativeObservation {
            name: case.name.clone(),
            actual,
            expected: case.expected.clone(),
            reference_failure: case
                .reference_failure_specification
                .as_ref()
                .map(|s| s.code),
        });
    }
    Ok(result)
}
pub(super) fn vm_vectors(
    module: &[u8],
    lane: &str,
    canonical: bool,
    cases: &[Case],
) -> Result<Vec<VmObservation>> {
    super::super::inspect(module)?;
    ensure!(module.len() <= 16 * 1024, "CAT module host bound");
    let mut result = vec![];
    for case in cases {
        let raw = hex::decode(&case.environment_hex)?;
        inspect_artifact(&raw, Encoding::Plain, &input_limits())?;
        let mut a = Allocator::new_limited(16 * 1024 * 1024);
        let p = node_from_bytes(&mut a, module)?;
        let env = node_from_bytes(&mut a, &raw)?;
        let (outcome, cost) = match clvmr::run_program(
            &mut a,
            &clvmr::ChiaDialect::new(clvmr::chia_dialect::MEMPOOL_MODE),
            p,
            env,
            10_000_000,
        ) {
            Ok(value) => {
                ensure!(
                    case.expected
                        == Outcome::Accepted {
                            output_hex: "80".into()
                        },
                    "VM accepted independently rejected CAT input"
                );
                let output = node_to_bytes(&a, value.1)?;
                ensure!(output == [0x80], "VM exact nil TAIL result");
                (
                    super::super::VmOutcome::Accepted {
                        output_hex: hex::encode(output),
                    },
                    Some(value.0),
                )
            }
            Err(error) => {
                let failure = chialisp_oxide_vm::classify_failure(&error);
                ensure!(
                    failure.is_program_rejection(),
                    "CAT primary VM resource/infrastructure result is inconclusive"
                );
                ensure!(
                    case.expected == Outcome::Rejected,
                    "VM rejected independently accepted CAT input"
                );
                let expected = if canonical {
                    case.reference_failure_specification
                        .as_ref()
                        .context("reference failure")?
                        .code
                } else {
                    FailureCode::Raise
                };
                ensure!(
                    failure.code() == expected,
                    "exact separate Rust/canonical failure code"
                );
                (super::super::VmOutcome::Rejected { failure }, None)
            }
        };
        result.push(VmObservation {
            lane: lane.into(),
            name: case.name.clone(),
            cost,
            outcome,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_paired_native_and_independent_oracle_agree_on_all_37_cases() {
        let rows = cases().unwrap();
        assert_eq!(native_vectors(&rows).unwrap().len(), 37);
    }
    #[test]
    fn complete_host_preflight_rejects_deep_unused_input_outside_policy_panic() {
        let mut raw = vec![0xff; MAX_DEPTH + 2];
        raw.extend(vec![0x80; MAX_DEPTH + 3]);
        assert!(decode(&raw).is_err());
        let oversized = vec![0x80; MAX_INPUT + 1];
        assert!(decode(&oversized).is_err());
    }
}
