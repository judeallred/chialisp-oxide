//! Original counter actions composed with independently compiled Chialisp peers.
//! This lane selects one action per language build, not both actions together.
mod lifecycle;
mod wire;

use anyhow::{Result, bail, ensure};
use chialisp_oxide_simulator::{SecretKey, puzzle_hash};
use serde_json::{Value, json};

const MODES: &[(&str, bool)] = &[
    ("rust_mir", false),
    ("rust_mir", true),
    ("chialisp", true),
    ("rue_source", false),
    ("rue_source", true),
];
const PRIMARY_MAX_COST: u64 = 250_000_000;

pub(super) fn validate(slug: &str, programs: &[(&str, bool, Vec<u8>)]) -> Result<Value> {
    let selected = match slug {
        "counter-increment" => 0,
        "counter-reset" => 1,
        _ => bail!("unknown counter action {slug}"),
    };
    ensure!(programs.len() == MODES.len(), "counter needs five builds");
    let canonical_components = lifecycle::canonical_components()?;
    let mut reference = Vec::new();
    for name in ["counter-increment", "counter-reset"] {
        let source = std::fs::read_to_string(
            super::root()
                .join("puzzles")
                .join(name)
                .join("reference.clsp"),
        )?;
        reference.push(chialisp_oxide_reference::compile(&source)?);
    }
    let owner = SecretKey::from_seed(&[0x14; 32]);
    let mut builds = Vec::new();
    for ((backend, optimize, module), expected) in programs.iter().zip(MODES) {
        ensure!((*backend, *optimize) == *expected, "counter build order");
        if *backend == "chialisp" {
            ensure!(
                module == &reference[selected],
                "selected Chialisp reference differs"
            );
        }
        let mut components = reference.clone();
        components[selected] = module.clone();
        let journal = lifecycle::run(
            slug,
            &components[0],
            &components[1],
            &owner,
            PRIMARY_MAX_COST,
        )?;
        builds.push(json!({
            "backend":backend,"optimize_lir":optimize,
            "selected_action":slug,
            "module_tree_hash":hex::encode(puzzle_hash(module)?),
            "counterpart":{"action":if selected==0 {"counter-reset"} else {"counter-increment"},"backend":"chialisp","optimize_lir":true,"module_hex":hex::encode(&reference[1-selected]),"module_tree_hash":hex::encode(puzzle_hash(&reference[1-selected])?)},
            "lifecycle":journal
        }));
    }
    let attempts = if selected == 0 { 25 } else { 35 };
    let typed = if selected == 0 { 5 } else { 15 };
    let primary = if selected == 0 { 30 } else { 40 };
    Ok(json!({
        "schema_version":1,"protocol":"chialisp-oxide-action-counter/v1","status":"validated","selected_action":slug,
        "sdk":"chia-sdk-test 0.36.0","consensus":"chia-consensus 0.36.1","sdk_vm":"clvmr 0.16.2","network":"TESTNET11",
        "primary_vm":"clvmr 0.21.0","primary_protocol":chialisp_oxide_vm::FAILURE_PROTOCOL,"primary_flags":clvmr::MEMPOOL_MODE.bits(),"primary_max_cost":PRIMARY_MAX_COST,
        "sdk_max_block_cost_clvm":chia_sdk_types::TESTNET11_CONSTANTS.max_block_cost_clvm,"sdk_transaction_max_cost":5_500_000_000u64,
        "owner_public_key":hex::encode(owner.public_key().to_bytes()),
        "canonical_components":canonical_components,
        "builds":builds,"sdk_transaction_attempts":attempts,"accepted":15,"typed_rejected":typed,"runtime_umbrellas":5,"component_primary_vm_runs":primary,
        "scope":"Five builds of the selected original teaching action, each composed with the other action's independently compiled Chialisp reference and canonical action-layer/finalizer/singleton programs. Each build funds its own committed instance at state 5, executes the selected action, then increments its actual child. Rejected submissions require exact typed outcomes and full in-memory simulator equality. Runtime umbrellas are separately resolved primary-VM program failures, not exact semantic consensus rejection evidence. This paired main lane is separate from dedicated source-control and broader two-action lifecycle evidence. No slot policy, complete issue #14, live network, authenticated execution or compiler-preservation claim."
    }))
}
