//! Backend feasibility harness. This does not compile Rust source yet.
use anyhow::{Context, Result, bail};
use clvmr::serde::{node_from_bytes, node_to_bytes};
use clvmr::{Allocator, ChiaDialect, MEMPOOL_MODE, NodePtr, run_program};
use id_arena::Arena;
use rue_compiler::{Compiler, FileTree, normalize_path};
use rue_diagnostic::DiagnosticSeverity;
use rue_lir::{CodegenOptions, Lir, LirId};
use rue_options::CompilerOptions;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const MAX_COST: u64 = 1_000_000;
pub const RUE_REV: &str = "245c107a8e795229ea08097142f005a4c0d91075";

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
    let path = fixture("puzzle.rue").canonicalize()?;
    let options = CompilerOptions {
        optimize_lir: optimize,
        // Hold other optimization options constant to isolate LIR optimization.
        auto_inline: true,
        optimize_static_pairs: false,
        debug_symbols: false,
        ..CompilerOptions::default()
    };
    let mut compiler = Compiler::new(options);
    let tree = FileTree::compile_file(&mut compiler, &path)?;
    let diagnostics = compiler.take_diagnostics();
    for diagnostic in &diagnostics {
        if diagnostic.kind.severity() == DiagnosticSeverity::Error {
            bail!("{}", diagnostic.message(path.parent().unwrap()));
        }
    }
    let mut allocator = Allocator::new();
    let program = tree
        .main(
            &mut compiler,
            &mut allocator,
            &normalize_path(&path)?,
            path.parent().unwrap().to_owned(),
        )?
        .context("Rue main missing")?;
    Ok(node_to_bytes(&allocator, program)?)
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
    Accepted { output_hex: String },
    Rejected,
}
#[derive(Debug, Serialize)]
pub struct Execution {
    pub outcome: Outcome,
    pub cost: Option<u64>,
    pub error: Option<String>,
}
pub fn execute(program: &[u8], solution: &str, max_cost: u64) -> Result<Execution> {
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
            },
            Err(error) => Execution {
                outcome: Outcome::Rejected,
                cost: None,
                error: Some(format!("{error:?}")),
            },
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
