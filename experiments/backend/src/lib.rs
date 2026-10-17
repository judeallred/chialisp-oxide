//! Backend feasibility harness. This does not compile Rust source yet.
use anyhow::Result;
use chialisp_oxide_vm::classify_failure;
pub use chialisp_oxide_vm::{
    FAILURE_PROTOCOL as VM_FAILURE_PROTOCOL, FailureCode, FailureKind, VmFailure,
};
use clvmr::serde::{node_from_bytes, node_to_bytes};
use clvmr::{Allocator, ChiaDialect, MEMPOOL_MODE, NodePtr, run_program};
use id_arena::Arena;
use rue_lir::{CodegenOptions, Lir, LirId};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const MAX_COST: u64 = 1_000_000;
pub const RUE_REV: &str = "245c107a8e795229ea08097142f005a4c0d91075";
pub const RUE_LOCAL_PATCHES: &[&str] = &[
    "https://github.com/xch-dev/rue/issues/108",
    "https://github.com/xch-dev/rue/issues/109",
    "https://github.com/xch-dev/rue/issues/110",
    "https://github.com/xch-dev/rue/issues/111",
    "https://github.com/xch-dev/rue/issues/112",
    "https://github.com/xch-dev/rue/issues/113",
];

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/capped-payment")
        .join(name)
}

// Explicitly models numeric atoms rather than Rust machine integer arithmetic.
// The VM preserves the submitted amount atom, even when its encoding is padded.
pub fn native(amount: &num_bigint::BigInt) -> bool {
    amount >= &0.into() && amount <= &1000.into()
}

pub fn compile_rue(optimize: bool) -> Result<Vec<u8>> {
    chialisp_oxide_rue::compile_file(&fixture("puzzle.rue"), optimize)
}

fn atom(a: &mut Arena<Lir>, value: i64) -> LirId {
    a.alloc(Lir::Atom(if value == 0 {
        vec![]
    } else {
        num_bigint::BigInt::from(value).to_signed_bytes_be()
    }))
}
fn list(a: &mut Arena<Lir>, values: &[LirId]) -> LirId {
    let mut tail = atom(a, 0);
    for value in values.iter().rev() {
        tail = a.alloc(Lir::Cons(*value, tail));
    }
    tail
}
fn guard(a: &mut Arena<Lir>, condition: LirId, yes: LirId, no: LirId) -> LirId {
    // `false` selects lazy branches; eager evaluation would run Raise on success.
    a.alloc(Lir::If(condition, yes, no, false))
}

pub fn compile_lir(optimize: bool) -> Result<Vec<u8>> {
    let mut a = Arena::new();
    let env = a.alloc(Lir::Path(1u8.into()));
    let amount = a.alloc(Lir::First(env));
    let rest = a.alloc(Lir::Rest(env));
    let nil = atom(&mut a, 0);
    let op = atom(&mut a, 51);
    let destination = a.alloc(Lir::Atom(vec![0; 32]));
    let condition = list(&mut a, &[op, destination, amount]);
    let output = list(&mut a, &[condition]);
    let failure = a.alloc(Lir::Raise(vec![]));
    let minus_one = atom(&mut a, -1);
    let upper = atom(&mut a, 1001);
    let lower_ok = a.alloc(Lir::Gt(amount, minus_one));
    let upper_ok = a.alloc(Lir::Gt(upper, amount));
    let body = guard(&mut a, upper_ok, output, failure);
    let body = guard(&mut a, lower_ok, body, failure);
    let amount_pair = a.alloc(Lir::Listp(amount, true));
    let body = guard(&mut a, amount_pair, failure, body);
    let rest_empty = a.alloc(Lir::Eq(rest, nil));
    let body = guard(&mut a, rest_empty, body, failure);
    let env_pair = a.alloc(Lir::Listp(env, true));
    let mut root = guard(&mut a, env_pair, body, failure);
    if optimize {
        root = rue_lir::optimize(&mut a, root);
    }
    let mut allocator = Allocator::new();
    let program = rue_lir::codegen(
        &a,
        &mut allocator,
        root,
        CodegenOptions {
            optimize_static_pairs: false,
        },
    )?;
    Ok(node_to_bytes(&allocator, program)?)
}

pub fn reference() -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let source = std::fs::read_to_string(fixture("reference.clvm"))?;
    let ptr = rue_clvm::assemble(&mut a, &source)?;
    Ok(node_to_bytes(&a, ptr)?)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Accepted {
        output_hex: String,
    },
    Rejected,
    /// A resource or infrastructure failure cannot confirm policy rejection.
    Inconclusive,
}
#[derive(Debug)]
pub struct Execution {
    pub outcome: Outcome,
    pub cost: Option<u64>,
    pub error: Option<String>,
    pub failure: Option<VmFailure>,
}

impl Serialize for Execution {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::Error as _;
        let valid = match &self.outcome {
            Outcome::Accepted { output_hex } => {
                !output_hex.is_empty()
                    && output_hex.len().is_multiple_of(2)
                    && output_hex
                        .bytes()
                        .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
                    && self.cost.is_some_and(|cost| cost > 0)
                    && self.error.is_none()
                    && self.failure.is_none()
            }
            Outcome::Rejected => {
                self.cost.is_none()
                    && self.error.as_ref().is_some_and(|error| !error.is_empty())
                    && self.failure.is_some_and(VmFailure::is_program_rejection)
            }
            Outcome::Inconclusive => {
                self.cost.is_none()
                    && self.error.as_ref().is_some_and(|error| !error.is_empty())
                    && self
                        .failure
                        .is_some_and(|failure| !failure.is_program_rejection())
            }
        };
        if !valid {
            return Err(S::Error::custom(
                "inconsistent VM execution outcome and failure metadata",
            ));
        }
        #[derive(Serialize)]
        struct Wire<'a> {
            outcome: &'a Outcome,
            cost: Option<u64>,
            error: &'a Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            failure: Option<VmFailure>,
        }
        Wire {
            outcome: &self.outcome,
            cost: self.cost,
            error: &self.error,
            failure: self.failure,
        }
        .serialize(serializer)
    }
}
pub fn execute(program: &[u8], solution: &str, max_cost: u64) -> Result<Execution> {
    anyhow::ensure!(
        max_cost > 0,
        "max_cost must be positive; zero means unlimited in clvmr"
    );
    let mut a = Allocator::new();
    let program = node_from_bytes(&mut a, program)?;
    let env = rue_clvm::assemble(&mut a, solution)?;
    Ok(
        match run_program(
            &mut a,
            &ChiaDialect::new(MEMPOOL_MODE),
            program,
            env,
            max_cost,
        ) {
            Ok(reduction) => Execution {
                outcome: Outcome::Accepted {
                    output_hex: hex::encode(node_to_bytes(&a, reduction.1)?),
                },
                cost: Some(reduction.0),
                error: None,
                failure: None,
            },
            Err(error) => {
                let failure = classify_failure(&error);
                Execution {
                    outcome: if failure.is_program_rejection() {
                        Outcome::Rejected
                    } else {
                        Outcome::Inconclusive
                    },
                    cost: None,
                    error: Some(format!("{error:?}")),
                    failure: Some(failure),
                }
            }
        },
    )
}

fn tree_hash(a: &Allocator, node: NodePtr) -> [u8; 32] {
    use clvmr::allocator::SExp;
    match a.sexp(node) {
        SExp::Atom => {
            let mut h = Sha256::new();
            h.update([1]);
            h.update(a.atom(node).as_ref());
            h.finalize().into()
        }
        SExp::Pair(left, right) => {
            let mut h = Sha256::new();
            h.update([2]);
            h.update(tree_hash(a, left));
            h.update(tree_hash(a, right));
            h.finalize().into()
        }
    }
}
#[derive(Serialize)]
pub struct Artifact {
    pub backend: String,
    pub optimize_lir: bool,
    pub bytes: usize,
    pub tree_hash: String,
    pub clvm_hex: String,
    pub disassembly: String,
}
pub fn artifact(backend: &str, optimize: bool, bytes: &[u8]) -> Result<Artifact> {
    let mut a = Allocator::new();
    let ptr = node_from_bytes(&mut a, bytes)?;
    Ok(Artifact {
        backend: backend.into(),
        optimize_lir: optimize,
        bytes: bytes.len(),
        tree_hash: hex::encode(tree_hash(&a, ptr)),
        clvm_hex: hex::encode(bytes),
        disassembly: rue_clvm::disassemble_program(&a, ptr),
    })
}
pub fn backends() -> Result<Vec<(&'static str, bool, Vec<u8>)>> {
    Ok(vec![
        ("reference_clvm", false, reference()?),
        ("rue_source", false, compile_rue(false)?),
        ("rue_source", true, compile_rue(true)?),
        ("direct_lir", false, compile_lir(false)?),
        ("direct_lir", true, compile_lir(true)?),
    ])
}
