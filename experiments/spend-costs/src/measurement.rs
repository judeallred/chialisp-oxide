//! Three distinct pinned APIs/encodings; no ledger state is reconstructed here.
use anyhow::{Context, Result, ensure};
use chia_consensus::{
    conditions::SpendBundleConditions,
    flags::MEMPOOL_MODE,
    run_block_generator::run_block_generator2,
    solution_generator::{
        calculate_generator_length, solution_generator, solution_generator_backrefs,
    },
    spendbundle_conditions::get_conditions_from_spendbundle,
    spendbundle_validation::get_flags_for_height_and_constants,
};
use chia_sdk_types::TESTNET11_CONSTANTS;
use chialisp_oxide_simulator::{ErrorCode, SecretKey, Signature, SpendBundle};
use clvmr::{
    Allocator,
    serde::{node_from_bytes, node_from_bytes_backrefs, node_to_bytes},
};
use serde_json::{Value, json};

pub const MAX_COST: u64 = 5_500_000_000;

fn per_spend(conditions: &SpendBundleConditions) -> Vec<Value> {
    let mut values = conditions
        .spends
        .iter()
        .map(|spend| {
            json!({
                "coin_id": hex::encode(spend.coin_id.as_ref()),
                "execution_cost": spend.execution_cost, "condition_cost": spend.condition_cost,
            })
        })
        .collect::<Vec<_>>();
    values.sort_by(|a, b| a["coin_id"].as_str().cmp(&b["coin_id"].as_str()));
    values
}

fn metrics(conditions: &SpendBundleConditions, charged_bytes: usize) -> Result<Value> {
    let byte_cost = u64::try_from(charged_bytes)?
        .checked_mul(TESTNET11_CONSTANTS.cost_per_byte)
        .context("byte cost overflow")?;
    ensure!(
        conditions.cost
            == conditions
                .execution_cost
                .checked_add(conditions.condition_cost)
                .and_then(|n| n.checked_add(byte_cost))
                .context("total cost overflow")?,
        "incorrect cost decomposition"
    );
    Ok(
        json!({"total_cost": conditions.cost, "execution_cost": conditions.execution_cost,
        "condition_cost": conditions.condition_cost, "byte_cost": byte_cost,
        "charged_bytes": charged_bytes, "signature_verified": conditions.validated_signature,
        "per_spend": per_spend(conditions)}),
    )
}

fn run_generator(
    generator: &[u8],
    signature: &Signature,
    flags: u32,
    max_cost: u64,
) -> Result<SpendBundleConditions, chia_consensus::validation_error::ValidationErr> {
    run_block_generator2(
        &mut Allocator::new(),
        generator,
        std::iter::empty::<&[u8]>(),
        max_cost,
        flags,
        signature,
        None,
        &TESTNET11_CONSTANTS,
    )
}

pub fn measure(bundle: &SpendBundle, height: u32) -> Result<Value> {
    ensure!(
        height < TESTNET11_CONSTANTS.hard_fork2_height,
        "this experiment only measures the pinned pre-hard-fork-2 configuration"
    );
    let spends = || {
        bundle
            .coin_spends
            .iter()
            .map(|s| (s.coin, s.puzzle_reveal.as_ref(), s.solution.as_ref()))
    };
    let uncompressed = solution_generator(spends())?;
    let backrefs = solution_generator_backrefs(spends())?;
    ensure!(
        calculate_generator_length(&bundle.coin_spends) == uncompressed.len(),
        "calculated generator length differs from actual bytes"
    );

    // Compare actual expanded trees, not hashes or successful result flags.
    let mut a = Allocator::new();
    let plain = node_from_bytes(&mut a, &uncompressed)?;
    let compressed = node_from_bytes_backrefs(&mut a, &backrefs)?;
    ensure!(
        node_to_bytes(&a, plain)? == node_to_bytes(&a, compressed)?,
        "backreferences changed the generator tree"
    );

    let helper = get_conditions_from_spendbundle(
        &mut Allocator::new(),
        bundle,
        MAX_COST,
        height,
        &TESTNET11_CONSTANTS,
    )
    .map_err(|e| anyhow::anyhow!("conditions helper: {e:?}"))?;
    ensure!(
        !helper.validated_signature,
        "helper signature contract changed"
    );
    let flags = get_flags_for_height_and_constants(height, &TESTNET11_CONSTANTS) | MEMPOOL_MODE;
    let mut generators = Vec::new();
    // A valid but unrelated, nonempty signature avoids mistaking an encoding
    // error or empty-signature shortcut for signature verification.
    let unrelated = chia_bls::sign(
        &SecretKey::from_seed(&[0x39; 32]),
        b"unrelated spend-cost control",
    );
    for (encoding, generator) in [("uncompressed", &uncompressed), ("backrefs", &backrefs)] {
        let actual = run_generator(generator, &bundle.aggregated_signature, flags, MAX_COST)
            .map_err(|e| anyhow::anyhow!("{encoding} generator: {e:?}"))?;
        ensure!(
            actual.validated_signature,
            "generator skipped signature validation"
        );
        ensure!(
            actual.execution_cost == helper.execution_cost + 20
                && actual.condition_cost == helper.condition_cost
                && per_spend(&actual) == per_spend(&helper)
                && actual.removal_amount == helper.removal_amount
                && actual.addition_amount == helper.addition_amount,
            "generator and helper cost/coin observations disagree"
        );
        let mut controls = Vec::new();
        // Genesis can legitimately have an identity signature. Replacing it
        // with an unrelated signature must still fail, even with no AGG_SIGs.
        for (name, signature) in [
            ("unrelated_nonempty", &unrelated),
            ("identity", &Signature::default()),
        ] {
            if *signature == bundle.aggregated_signature {
                continue;
            }
            let error = run_generator(generator, signature, flags, MAX_COST)
                .expect_err("altered signature unexpectedly accepted");
            ensure!(
                error.1 == ErrorCode::BadAggregateSignature,
                "{encoding}/{name}: expected BadAggregateSignature, got {error:?}"
            );
            controls.push(json!({"signature": name, "code": "BadAggregateSignature"}));
        }
        ensure!(!controls.is_empty(), "no signature negative control");
        let exact = run_generator(generator, &bundle.aggregated_signature, flags, actual.cost)
            .map_err(|e| anyhow::anyhow!("exact cost budget failed: {e:?}"))?;
        ensure!(
            exact.cost == actual.cost && exact.validated_signature,
            "exact budget changed returned cost/signature validation"
        );
        let below = run_generator(
            generator,
            &bundle.aggregated_signature,
            flags,
            actual.cost - 1,
        )
        .expect_err("one-less-than-total cost budget unexpectedly succeeded");
        ensure!(
            below.1 == ErrorCode::CostExceeded,
            "budget control expected CostExceeded, got {below:?}"
        );
        generators.push(
            json!({"encoding": encoding, "generator_hex": hex::encode(generator),
            "generator_bytes": generator.len(), "metrics": metrics(&actual, generator.len())?,
            "signature_controls": controls,
            "budget_control": {"exact_cost_succeeded":true,"one_less_code":"CostExceeded"}}),
        );
    }
    Ok(
        json!({"previous_transaction_height": height, "flags": flags,
        "max_cost": MAX_COST, "cost_per_byte": TESTNET11_CONSTANTS.cost_per_byte,
        "helper": metrics(&helper, uncompressed.len().checked_sub(2).context("quote wrapper")?)?,
        "generators": generators,
        "total_reveal_bytes": bundle.coin_spends.iter().map(|s| s.puzzle_reveal.len()).sum::<usize>(),
        "total_solution_bytes": bundle.coin_spends.iter().map(|s| s.solution.len()).sum::<usize>()}),
    )
}
