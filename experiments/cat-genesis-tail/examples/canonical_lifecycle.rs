//! Canonical CAT2/TAIL SDK experiment. Shared actual simulator implementation.
use anyhow::{Result, ensure};
pub(crate) use chialisp_oxide_corpus::cat_genesis_tail::lifecycle::{
    run_lifecycle as canonical_lifecycle, run_source_control as source_control,
};
use chialisp_oxide_simulator::{
    CONSENSUS_VERSION, NETWORK, SIMULATOR_VERSION, SecretKey, VM_VERSION, puzzle_hash,
};
use serde_json::json;
use sha2::{Digest, Sha256};

const PRIMARY_COST: u64 = 50_000_000;
const TAIL_SOURCE: &str = include_str!("../upstream/genesis_by_coin_id.clsp");
pub(crate) const CAT_SOURCE: &str = include_str!("../upstream/cat_v2.clsp");

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(crate) fn compile(source: &str) -> Result<Vec<u8>> {
    chialisp_oxide_reference::compile_with_includes(
        source,
        &[
            (
                "cat_truths.clib",
                include_str!("../upstream/cat_truths.clib"),
            ),
            (
                "condition_codes.clib",
                include_str!("../upstream/condition_codes.clib"),
            ),
            (
                "curry-and-treehash.clib",
                include_str!("../upstream/curry-and-treehash.clib"),
            ),
            (
                "utility_macros.clib",
                include_str!("../upstream/utility_macros.clib"),
            ),
        ],
    )
}

#[allow(dead_code)] // Also imported as a reviewed helper module by the Rust-artifact lane.
fn main() -> Result<()> {
    let canonical = compile(TAIL_SOURCE)?;
    ensure!(
        canonical == hex::decode(include_str!("../upstream/genesis_by_coin_id.clsp.hex").trim())?
            && canonical == chia_puzzles::GENESIS_BY_COIN_ID,
        "independent TAIL source/hex/SDK mismatch"
    );
    let cat = compile(CAT_SOURCE)?;
    ensure!(
        cat == hex::decode(include_str!("../upstream/cat_v2.clsp.hex").trim())?
            && cat == chia_puzzles::CAT_PUZZLE,
        "independent CAT2 source/hex/SDK mismatch"
    );
    ensure!(
        puzzle_hash(&cat)?.to_bytes() == chia_puzzles::CAT_PUZZLE_HASH,
        "CAT2 module identity"
    );
    let drop_genesis_before = "(if (= (my_parent_cat_truth Truths) GENESIS_ID)";
    let drop_delta_before = "(if delta";
    ensure!(
        TAIL_SOURCE.matches(drop_genesis_before).count() == 1
            && TAIL_SOURCE.matches(drop_delta_before).count() == 1,
        "unique exact canonical source recipes"
    );
    let drop_genesis = TAIL_SOURCE.replace(drop_genesis_before, "(if 1");
    let drop_delta = TAIL_SOURCE.replace(drop_delta_before, "(if ()");
    let owner = SecretKey::from_seed(&[0x63; 32]);
    let recipient = SecretKey::from_seed(&[0x64; 32]);
    let lifecycle = canonical_lifecycle(&canonical, &owner, &recipient, PRIMARY_COST)?;
    let mut controls = Vec::new();
    for (name, source, wrong_genesis, delta, accepted) in [
        ("original_wrong_genesis", TAIL_SOURCE, true, false, false),
        (
            "drop_genesis_wrong_genesis",
            drop_genesis.as_str(),
            true,
            false,
            true,
        ),
        ("original_nonzero_delta", TAIL_SOURCE, false, true, false),
        (
            "drop_delta_nonzero_delta",
            drop_delta.as_str(),
            false,
            true,
            true,
        ),
    ] {
        let module = compile(source)?;
        let mut control = source_control(
            &module,
            name,
            wrong_genesis,
            delta,
            accepted,
            &owner,
            PRIMARY_COST,
        )?;
        control["source"] = json!(source);
        control["source_sha256"] = json!(digest(source.as_bytes()));
        controls.push(control);
    }
    let all = lifecycle["attempts"].as_array().unwrap().iter().chain(
        controls
            .iter()
            .flat_map(|row| row["attempts"].as_array().unwrap()),
    );
    let attempts = all.cloned().collect::<Vec<_>>();
    let count = |status| {
        attempts
            .iter()
            .filter(|row| row["outcome"]["status"] == status)
            .count()
    };
    let vm_runs = attempts
        .iter()
        .map(|row| row["primary_vm"].as_array().unwrap().len())
        .sum::<usize>();
    println!(
        "{}",
        json!({"protocol":"chialisp-oxide-canonical-cat-genesis-lifecycle/v1","status":"executed",
        "sdk":SIMULATOR_VERSION,"consensus":CONSENSUS_VERSION,"sdk_vm":VM_VERSION,"network":NETWORK,
        "primary_vm":"clvmr 0.21.0","primary_protocol":chialisp_oxide_vm::FAILURE_PROTOCOL,"primary_max_cost":PRIMARY_COST,
        "cat_module_hex":hex::encode(cat),"cat_module_tree_hash":hex::encode(chia_puzzles::CAT_PUZZLE_HASH),
        "tail_module_hex":hex::encode(canonical),"owner_public_key":hex::encode(owner.public_key().to_bytes()),
        "recipient_public_key":hex::encode(recipient.public_key().to_bytes()),"lifecycle":lifecycle,"source_controls":controls,
        "sdk_transaction_attempts":attempts.len(),"accepted":count("accepted"),"typed_rejected":count("consensus_rejected"),
        "runtime_umbrellas":count("runtime_umbrella"),"component_primary_vm_runs":vm_runs,
        "scope":"Canonical CAT2 and independently compiled Chialisp TAIL/mutant sources only. Component VM outputs, SDK attempts and independent raw TAIL vectors are distinct. Every rejection checks full SDK state in memory; runtime umbrellas gain no consensus-rejection credit. No Rust TAIL artifacts, Sage, live network, corpus completion, authenticated execution or compiler-preservation claim."})
    );
    Ok(())
}
