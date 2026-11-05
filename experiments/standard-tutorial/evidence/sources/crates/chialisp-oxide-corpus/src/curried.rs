//! Standard curried instances of a pure module, built independently of the SDK
//! and executed in every build.
//!
//! The instance `(a (q . M) (c (q . A) 1))` is constructed with clvmr, then
//! compared with the SDK's owned `curry` constructor and its hash arithmetic.
//! Solutions run against each build's instance and must equal the native Rust
//! result of the uncurried environment `(A . solution)`; no divergence may be
//! declared here. A page-spelled curried result binds only the Chialisp build,
//! whose module is the independently compiled original.
use super::{Artifact, CaseReport, Outcome, Result, Run, artifact, execute, expectation};
use anyhow::{Context, ensure};
use bumpalo::Bump;
use chialisp_oxide::{
    OwnedTree,
    curry::{curried_puzzle_hash, curry},
};
use clvmr::{
    Allocator, NodePtr, SExp,
    serde::{node_from_bytes, node_to_bytes},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEME: &str = "(a (q . M) (c (q . A) 1))";
const CONSTRUCTOR: &str =
    "independent clvmr standard-form construction cross-checked with chialisp_oxide::curry";

#[derive(Clone, Deserialize, Serialize)]
pub struct Solution {
    pub label: String,
    pub solution: String,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Spec {
    pub label: String,
    pub argument: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_result: Option<String>,
    pub solutions: Vec<Solution>,
}
#[derive(Serialize)]
pub struct Instance {
    pub backend: String,
    pub optimize_lir: bool,
    pub module_tree_hash: String,
    pub artifact: Artifact,
    pub sdk_owned_curry_bytes_equal: bool,
    pub sdk_hash_equal: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_result_equal: Option<bool>,
}
#[derive(Serialize)]
pub struct Example {
    pub label: String,
    pub argument: String,
    pub argument_hex: String,
    pub argument_tree_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_result_hex: Option<String>,
    pub scheme: &'static str,
    pub constructor: &'static str,
    pub instances: Vec<Instance>,
    pub cases: Vec<CaseReport>,
}

fn list(a: &mut Allocator, values: &[NodePtr]) -> Result<NodePtr> {
    let mut tail = NodePtr::NIL;
    for value in values.iter().rev() {
        tail = a.new_pair(*value, tail)?;
    }
    Ok(tail)
}

/// Independent standard form from serialized module and argument trees.
pub(super) fn standard_form(module: &[u8], argument: &[u8]) -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let module = node_from_bytes(&mut a, module)?;
    let argument = node_from_bytes(&mut a, argument)?;
    let one = a.new_atom(&[1])?;
    let two = a.new_atom(&[2])?;
    let four = a.new_atom(&[4])?;
    let quoted_module = a.new_pair(one, module)?;
    let quoted_argument = a.new_pair(one, argument)?;
    let environment = list(&mut a, &[four, quoted_argument, one])?;
    let result = list(&mut a, &[two, quoted_module, environment])?;
    Ok(node_to_bytes(&a, result)?)
}

pub(super) fn tree_hash(a: &Allocator, node: NodePtr) -> [u8; 32] {
    let mut hasher = Sha256::new();
    match a.sexp(node) {
        SExp::Atom => {
            hasher.update([1]);
            hasher.update(a.atom(node).as_ref());
        }
        SExp::Pair(first, rest) => {
            hasher.update([2]);
            hasher.update(tree_hash(a, first));
            hasher.update(tree_hash(a, rest));
        }
    }
    hasher.finalize().into()
}

pub(super) fn owned<'b>(arena: &'b Bump, a: &Allocator, node: NodePtr) -> OwnedTree<'b> {
    match a.sexp(node) {
        SExp::Atom => OwnedTree::Atom(arena.alloc_slice_copy(a.atom(node).as_ref())),
        SExp::Pair(first, rest) => OwnedTree::Pair(
            Box::new(owned(arena, a, first)),
            Box::new(owned(arena, a, rest)),
        ),
    }
}

pub(super) fn encode(a: &mut Allocator, tree: &OwnedTree<'_>) -> Result<NodePtr> {
    Ok(match tree {
        OwnedTree::Atom(bytes) => a.new_atom(bytes)?,
        OwnedTree::Pair(first, rest) => {
            let first = encode(a, first)?;
            let rest = encode(a, rest)?;
            a.new_pair(first, rest)?
        }
    })
}

fn digest(text: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(text)?;
    bytes
        .as_slice()
        .try_into()
        .context("tree hash must be 32 bytes")
}

pub(super) fn validate(
    slug: &str,
    specs: &[Spec],
    programs: &[(&str, bool, Vec<u8>)],
    max_cost: u64,
) -> Result<Vec<Example>> {
    let mut examples = vec![];
    for spec in specs {
        let mut a = Allocator::new();
        let argument = rue_clvm::assemble(&mut a, &spec.argument)?;
        let argument_bytes = node_to_bytes(&a, argument)?;
        let argument_hash = tree_hash(&a, argument);
        let page_result_hex = match &spec.page_result {
            Some(text) => {
                let node = rue_clvm::assemble(&mut a, text)?;
                Some(hex::encode(node_to_bytes(&a, node)?))
            }
            None => None,
        };
        let mut instances = vec![];
        let mut codes = vec![];
        for (backend, optimize, module) in programs {
            let code = standard_form(module, &argument_bytes)?;
            let sdk_bytes = {
                let mut b = Allocator::new();
                let module_node = node_from_bytes(&mut b, module)?;
                let argument_node = node_from_bytes(&mut b, &argument_bytes)?;
                let arena = Bump::new();
                let tree = curry(
                    owned(&arena, &b, module_node),
                    owned(&arena, &b, argument_node),
                );
                let mut c = Allocator::new();
                let encoded = encode(&mut c, &tree)?;
                node_to_bytes(&c, encoded)?
            };
            ensure!(
                sdk_bytes == code,
                "{slug}: SDK owned curry differs from the independent standard form for {backend}"
            );
            let module_artifact = artifact(backend, *optimize, module)?;
            let instance = artifact(backend, *optimize, &code)?;
            let sdk_hash =
                curried_puzzle_hash(&digest(&module_artifact.tree_hash)?, &argument_hash);
            ensure!(
                hex::encode(sdk_hash) == instance.tree_hash,
                "{slug}: SDK curried puzzle hash differs from the instance tree hash for {backend}"
            );
            let page_result_equal = match (&page_result_hex, *backend) {
                (Some(expected), "chialisp") => {
                    ensure!(
                        *expected == hex::encode(&code),
                        "{slug}: the Chialisp build's curried instance differs from the page's curried result"
                    );
                    Some(true)
                }
                _ => None,
            };
            instances.push(Instance {
                backend: (*backend).into(),
                optimize_lir: *optimize,
                module_tree_hash: module_artifact.tree_hash,
                artifact: instance,
                sdk_owned_curry_bytes_equal: true,
                sdk_hash_equal: true,
                page_result_equal,
            });
            codes.push((*backend, *optimize, code));
        }
        let mut cases = vec![];
        for solution in &spec.solutions {
            let mut a = Allocator::new();
            let argument = rue_clvm::assemble(&mut a, &spec.argument)?;
            let raw = rue_clvm::assemble(&mut a, &solution.solution)?;
            let environment = a.new_pair(argument, raw)?;
            let environment = rue_clvm::disassemble_value(&a, environment);
            let native = expectation(slug, &environment)?;
            let mut runs = vec![];
            for (backend, optimize, code) in &codes {
                let execution = execute(code, &solution.solution, max_cost)?;
                ensure!(
                    execution.outcome != Outcome::Inconclusive,
                    "{slug}: inconclusive curried execution for {backend}: {execution:?}"
                );
                ensure!(
                    execution.outcome == native,
                    "{slug}: curried {backend} result differs from the native uncurried expectation for {environment}: {execution:?}"
                );
                runs.push(Run {
                    backend: (*backend).into(),
                    optimize_lir: *optimize,
                    execution,
                });
            }
            cases.push(CaseReport {
                label: solution.label.clone(),
                solution: solution.solution.clone(),
                native,
                runs,
                declared_divergence: None,
            });
        }
        examples.push(Example {
            label: spec.label.clone(),
            argument: spec.argument.clone(),
            argument_hex: hex::encode(&argument_bytes),
            argument_tree_hash: hex::encode(argument_hash),
            page_result: spec.page_result.clone(),
            page_result_hex,
            scheme: SCHEME,
            constructor: CONSTRUCTOR,
            instances,
            cases,
        });
    }
    Ok(examples)
}
