//! Five independently compiled TAILs composed with canonical CAT2 counterparts.
//! These local simulator observations do not establish live or Sage acceptance.
pub mod lifecycle;

use anyhow::{Result, ensure};
use chialisp_oxide_simulator::{SecretKey, puzzle_hash};
use serde::Serialize;

const MODES: &[(&str, bool)] = &[
    ("rust_mir", false),
    ("rust_mir", true),
    ("chialisp", true),
    ("rue_source", false),
    ("rue_source", true),
];
const OWNER_SEED: [u8; 32] = [0x63; 32];
const RECIPIENT_SEED: [u8; 32] = [0x64; 32];
const PRIMARY_MAX_COST: u64 = 250_000_000;
const UPSTREAM: &str = "corpus/upstream/chia-puzzles/d122456feeef55708d354a065623037bfa010714";

#[derive(Serialize)]
struct Build {
    backend: String,
    optimize_lir: bool,
    module_tree_hash: String,
    lifecycle: lifecycle::LifecycleJournal,
}
#[derive(Serialize)]
struct CatProtocolValidation {
    schema_version: u32,
    protocol: &'static str,
    status: &'static str,
    sdk: &'static str,
    consensus: &'static str,
    sdk_vm: &'static str,
    network: &'static str,
    primary_vm: &'static str,
    primary_protocol: &'static str,
    primary_flags: u32,
    primary_max_cost: u64,
    sdk_max_block_cost_clvm: u64,
    sdk_transaction_max_cost: u64,
    cat_module_hex: String,
    cat_module_tree_hash: String,
    canonical_tail_hex: String,
    canonical_tail_tree_hash: String,
    owner_public_key: String,
    recipient_public_key: String,
    builds: Vec<Build>,
    sdk_transaction_attempts: usize,
    accepted: usize,
    typed_rejected: usize,
    runtime_umbrellas: usize,
    component_primary_vm_runs: usize,
    scope: &'static str,
}

fn compile_original(name: &str) -> Result<Vec<u8>> {
    let base = super::root().join(UPSTREAM);
    let source = std::fs::read_to_string(base.join("puzzles/cat_puzzles").join(name))?;
    let mut includes = Vec::new();
    for (name, path) in [
        ("cat_truths.clib", "puzzles/cat_puzzles/cat_truths.clib"),
        ("condition_codes.clib", "puzzles/condition_codes.clib"),
        ("curry-and-treehash.clib", "puzzles/curry-and-treehash.clib"),
        ("utility_macros.clib", "puzzles/utility_macros.clib"),
    ] {
        includes.push((name.to_string(), std::fs::read_to_string(base.join(path))?));
    }
    let borrowed = includes
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    chialisp_oxide_reference::compile_with_includes(&source, &borrowed)
}

pub(super) fn validate(programs: &[(&str, bool, Vec<u8>)]) -> Result<serde_json::Value> {
    ensure!(
        programs.len() == MODES.len(),
        "CAT requires exactly five compiled modes"
    );
    let tail = compile_original("genesis_by_coin_id.clsp")?;
    let cat = compile_original("cat_v2.clsp")?;
    ensure!(
        tail == lifecycle::CANONICAL_TAIL_MODULE && cat == lifecycle::CANONICAL_CAT_MODULE,
        "independent canonical CAT/TAIL compilation differs from pinned SDK artifacts"
    );
    let owner = SecretKey::from_seed(&OWNER_SEED);
    let recipient = SecretKey::from_seed(&RECIPIENT_SEED);
    let mut builds = Vec::new();
    for ((backend, optimize_lir, module), expected) in programs.iter().zip(MODES) {
        ensure!(
            (*backend, *optimize_lir) == *expected,
            "CAT build order or mode changed"
        );
        if *backend == "chialisp" {
            ensure!(*module == tail, "canonical corpus TAIL differs");
        }
        builds.push(Build {
            backend: backend.to_string(),
            optimize_lir: *optimize_lir,
            module_tree_hash: hex::encode(puzzle_hash(module)?.to_bytes()),
            lifecycle: lifecycle::run_lifecycle(module, &owner, &recipient, PRIMARY_MAX_COST)?,
        });
    }
    let evidence = CatProtocolValidation {
        schema_version: 1,
        protocol: "chialisp-oxide-cat-genesis-by-coin-id/v1",
        status: "validated",
        sdk: "chia-sdk-test 0.36.0",
        consensus: "chia-consensus 0.36.1",
        sdk_vm: "clvmr 0.16.2",
        network: "TESTNET11",
        primary_vm: "clvmr 0.21.0",
        primary_protocol: chialisp_oxide_vm::FAILURE_PROTOCOL,
        primary_flags: clvmr::MEMPOOL_MODE.bits(),
        primary_max_cost: PRIMARY_MAX_COST,
        sdk_max_block_cost_clvm: chia_sdk_types::TESTNET11_CONSTANTS.max_block_cost_clvm,
        sdk_transaction_max_cost: 5_500_000_000,
        cat_module_hex: hex::encode(&cat),
        cat_module_tree_hash: hex::encode(puzzle_hash(&cat)?.to_bytes()),
        canonical_tail_hex: hex::encode(&tail),
        canonical_tail_tree_hash: hex::encode(puzzle_hash(&tail)?.to_bytes()),
        owner_public_key: hex::encode(owner.public_key().to_bytes()),
        recipient_public_key: hex::encode(recipient.public_key().to_bytes()),
        builds,
        sdk_transaction_attempts: 75,
        accepted: 20,
        typed_rejected: 35,
        runtime_umbrellas: 20,
        component_primary_vm_runs: 75,
        scope: "Five independently compiled TAIL modules, each curried and funded under its own asset ID, composed with canonical CAT2 and signed standard-wallet counterparts. Fifteen actual SDK submissions per build with full in-memory simulator equality after every rejection; component VM runs are separate from raw vectors. Runtime umbrellas carry no exact consensus rejection credit. Local finite evidence only; no Sage, live Testnet11, authenticated execution or compiler-preservation claim.",
    };
    Ok(serde_json::to_value(evidence)?)
}
