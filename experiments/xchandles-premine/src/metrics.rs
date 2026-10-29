//! Complete plain generator measurement; separate from simulator admission/BLS.
use anyhow::{Result, ensure};
use chia_consensus::{
    solution_generator::{calculate_generator_length, solution_generator},
    spendbundle_conditions::get_conditions_from_spendbundle,
};
use chia_protocol::SpendBundle;
use chia_sdk_types::TESTNET11_CONSTANTS;
use serde_json::{Value, json};

pub fn measure(bundle: &SpendBundle, height: u32) -> Result<Value> {
    ensure!(
        bundle.coin_spends.len() <= 128,
        "bounded fixture spend count"
    );
    let mut bytes = 0usize;
    for spend in &bundle.coin_spends {
        for blob in [spend.puzzle_reveal.as_ref(), spend.solution.as_ref()] {
            ensure!(
                blob.len() <= 65536 && blob.len() <= 4 * 1024 * 1024 - bytes,
                "bounded fixture input bytes"
            );
            bytes += blob.len();
        }
    }
    let generator = solution_generator(
        bundle
            .coin_spends
            .iter()
            .map(|s| (s.coin, s.puzzle_reveal.as_ref(), s.solution.as_ref())),
    )?;
    ensure!(
        generator.len() == calculate_generator_length(&bundle.coin_spends),
        "actual plain generator byte calculation"
    );
    let mut allocator = clvmr::Allocator::new_limited(64 * 1024 * 1024);
    let conditions = get_conditions_from_spendbundle(
        &mut allocator,
        bundle,
        5_500_000_000,
        height,
        &TESTNET11_CONSTANTS,
    )?;
    let charged = generator.len().checked_sub(2).unwrap();
    let byte_cost = u64::try_from(charged)?
        .checked_mul(TESTNET11_CONSTANTS.cost_per_byte)
        .unwrap();
    ensure!(
        conditions.cost == conditions.execution_cost + conditions.condition_cost + byte_cost,
        "pinned quote-byte/cost relation"
    );
    let fee = u64::try_from(
        conditions
            .removal_amount
            .checked_sub(conditions.addition_amount)
            .ok_or_else(|| anyhow::anyhow!("negative fee"))?,
    )?;
    Ok(
        json!({"api":"chia-consensus0.36.1/get_conditions_from_spendbundle","height":height,"flags":chia_consensus::spendbundle_validation::get_flags_for_height_and_constants(height,&TESTNET11_CONSTANTS)|chia_consensus::flags::MEMPOOL_MODE|chia_consensus::flags::DONT_VALIDATE_SIGNATURE,"max_cost":5_500_000_000u64,"plain_generator_bytes":generator.len(),"charged_generator_bytes":charged,"cost_per_byte":TESTNET11_CONSTANTS.cost_per_byte,"byte_cost":byte_cost,"execution_cost":conditions.execution_cost,"condition_cost":conditions.condition_cost,"total_cost":conditions.cost,"fee_mojos":fee,"cost_pass_validates_signature":false,"scope":"Complete plain spend-bundle generator measurement excluding two quote bytes/execution overhead; separate from actual SDK signature/ledger admission. Not a full-block/backreference compression cost or network fee quote."}),
    )
}

/// Fixed counterpart modules, separate from curried instances and coin IDs.
pub fn canonical_modules() -> Result<Vec<Value>> {
    use chia_sdk_types::puzzles::*;
    use sha2::{Digest, Sha256};
    let modules: &[(&str, &[u8])] = &[
        ("register", &XCHANDLES_REGISTER_PUZZLE),
        ("action_layer", &ACTION_LAYER_PUZZLE),
        ("slot", &SLOT_PUZZLE),
        ("precommit", &PRECOMMIT_LAYER_PUZZLE),
        ("default_cat_maker", &DEFAULT_CAT_MAKER_PUZZLE),
        ("cat2", &chia_puzzles::CAT_PUZZLE),
        (
            "singleton_top_v1_1",
            &chia_puzzles::SINGLETON_TOP_LAYER_V1_1,
        ),
        ("singleton_launcher", &chia_puzzles::SINGLETON_LAUNCHER),
        ("nft_state", &chia_puzzles::NFT_STATE_LAYER),
        ("nft_ownership", &chia_puzzles::NFT_OWNERSHIP_LAYER),
        (
            "standard",
            &chia_puzzles::P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE,
        ),
    ];
    modules.iter().map(|(name,bytes)|Ok(json!({"name":name,"bytes":bytes.len(),"tree_hash":hex::encode(clvm_utils::tree_hash_from_bytes(bytes)?.to_bytes()),"blob_sha256":hex::encode(Sha256::digest(bytes))}))).collect()
}
