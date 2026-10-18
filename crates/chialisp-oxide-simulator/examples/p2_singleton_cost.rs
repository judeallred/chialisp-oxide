//! Measure retained successful prototype bundles through pinned consensus APIs.
//! This is separate from Simulator admission and the primary raw VM benchmark.
use anyhow::{Context, Result, ensure};
use chia_consensus::{
    solution_generator::{
        calculate_generator_length, solution_generator, solution_generator_backrefs,
    },
    spendbundle_conditions::get_conditions_from_spendbundle,
};
use chia_sdk_types::TESTNET11_CONSTANTS;
use chialisp_oxide_simulator::{Bytes32, Coin, CoinSpend, Signature, SpendBundle, program};
use serde_json::{Value, json};

fn bytes32(value: &Value) -> Result<Bytes32> {
    Ok(Bytes32::new(
        hex::decode(value.as_str().context("32-byte hex")?)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("wrong 32-byte length"))?,
    ))
}

fn bundle(value: &Value) -> Result<SpendBundle> {
    let mut spends = Vec::new();
    for spend in value["spends"].as_array().context("spends")? {
        let coin = &spend["coin"];
        let coin = Coin::new(
            bytes32(&coin["parent_id"])?,
            bytes32(&coin["puzzle_hash"])?,
            coin["amount"].as_u64().context("amount")?,
        );
        ensure!(
            hex::encode(coin.coin_id()) == spend["coin"]["id"],
            "coin identity changed"
        );
        spends.push(CoinSpend::new(
            coin,
            program(&hex::decode(
                spend["reveal_hex"].as_str().context("reveal")?,
            )?)?,
            program(&hex::decode(
                spend["solution_hex"].as_str().context("solution")?,
            )?)?,
        ));
    }
    let signature: [u8; 96] = hex::decode(value["signature_hex"].as_str().context("signature")?)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("wrong signature length"))?;
    Ok(SpendBundle::new(spends, Signature::from_bytes(&signature)?))
}

fn metrics(value: &Value, height: u32) -> Result<Value> {
    let bundle = bundle(value)?;
    let mut allocator = clvmr::Allocator::new();
    let conditions = get_conditions_from_spendbundle(
        &mut allocator,
        &bundle,
        5_500_000_000,
        height,
        &TESTNET11_CONSTANTS,
    )
    .map_err(|error| anyhow::anyhow!("consensus conditions: {error:?}"))?;
    let generator = solution_generator(bundle.coin_spends.iter().map(|spend| {
        (
            spend.coin,
            spend.puzzle_reveal.as_ref(),
            spend.solution.as_ref(),
        )
    }))?;
    let backrefs = solution_generator_backrefs(bundle.coin_spends.iter().map(|spend| {
        (
            spend.coin,
            spend.puzzle_reveal.as_ref(),
            spend.solution.as_ref(),
        )
    }))?;
    let calculated = calculate_generator_length(&bundle.coin_spends);
    ensure!(
        calculated == generator.len(),
        "calculated generator size differs from actual uncompressed bytes"
    );
    // The public helper charges the uncompressed generator minus its two-byte
    // quote wrapper. Record the actual returned cost component and separately
    // check the explicit rule in pinned spendbundle_conditions.rs.
    let observed_byte_cost = conditions
        .cost
        .checked_sub(conditions.execution_cost)
        .and_then(|cost| cost.checked_sub(conditions.condition_cost))
        .context("cost components exceed total")?;
    let charged_bytes = calculated.checked_sub(2).context("missing quote wrapper")?;
    ensure!(
        observed_byte_cost
            == u64::try_from(charged_bytes)?
                .checked_mul(TESTNET11_CONSTANTS.cost_per_byte)
                .context("byte cost overflow")?,
        "pinned quote-exclusion rule disagrees with returned cost"
    );
    let spends=conditions.spends.iter().map(|spend|json!({"coin_id":hex::encode(spend.coin_id.as_ref()),"execution_cost":spend.execution_cost,"condition_cost":spend.condition_cost})).collect::<Vec<_>>();
    Ok(
        json!({"previous_transaction_height":height,"max_cost":5_500_000_000_u64,"total_conditions_cost":conditions.cost,"execution_cost":conditions.execution_cost,"condition_cost":conditions.condition_cost,"per_spend":spends,"observed_byte_cost_component":observed_byte_cost,"cost_per_byte":TESTNET11_CONSTANTS.cost_per_byte,"charged_generator_bytes_pinned_source_rule":charged_bytes,"excluded_quote_wrapper_bytes":2,"quote_rule_source":"chia-consensus0.36.1/src/spendbundle_conditions.rs: const QUOTE_BYTES: usize = 2; calculate_generator_length - QUOTE_BYTES","generator_bytes_calculated":calculated,"generator_uncompressed_hex":hex::encode(&generator),"generator_uncompressed_bytes":generator.len(),"generator_backrefs_hex":hex::encode(&backrefs),"generator_backrefs_bytes":backrefs.len(),"backrefs_scope":"Serialized and measured only; no compressed-generator cost or admission is claimed. The conditions API above charges its own uncompressed length rule.","total_reveal_bytes":bundle.coin_spends.iter().map(|spend|spend.puzzle_reveal.len()).sum::<usize>(),"total_solution_bytes":bundle.coin_spends.iter().map(|spend|spend.solution.len()).sum::<usize>(),"source_api":"chia_consensus::spendbundle_conditions::get_conditions_from_spendbundle / solution_generator::{calculate_generator_length,solution_generator,solution_generator_backrefs}","signature_scope":"This conditions API skips signature verification. These exact successful bundles were separately submitted to the SDK Simulator with signature verification; this metric call is not an additional admission."}),
    )
}

fn main() -> Result<()> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        arguments.len() == 2,
        "usage: p2_singleton_cost LEDGER.json OUTPUT.json"
    );
    let report: Value = serde_json::from_slice(&std::fs::read(&arguments[0])?)?;
    let mut observations = Vec::new();
    for build in report["observations"].as_array().context("builds")? {
        let lifecycle = &build["lifecycle"];
        let genesis = metrics(&lifecycle["genesis"], 0)?;
        let joint = metrics(&lifecycle["joint"], 1)?;
        let lineage = metrics(&lifecycle["lineage"], 2)?;
        println!(
            "{}/{} joint: total {}, execution {}, conditions {}, generator {} B",
            build["backend"],
            build["optimize_lir"],
            joint["total_conditions_cost"],
            joint["execution_cost"],
            joint["condition_cost"],
            joint["generator_uncompressed_bytes"]
        );
        observations.push(json!({"backend":build["backend"],"optimize_lir":build["optimize_lir"],"genesis":genesis,"joint":joint,"lineage":lineage}));
    }
    std::fs::write(
        &arguments[1],
        serde_json::to_vec_pretty(
            &json!({"scope":"Pinned consensus conditions/generator metrics for separately admitted exact canonical composition bundles. Neither primary raw clvmr0.21 costs nor universal block/mainnet performance.","consensus":"chia-consensus 0.36.1","VM":"clvmr 0.16.2","network":"TESTNET11","observations":observations}),
        )?,
    )?;
    Ok(())
}
