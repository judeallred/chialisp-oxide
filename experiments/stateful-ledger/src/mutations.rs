//! Finite own-reveal source-policy controls. Native and primary-VM evidence lives
//! in the capture packet; this lane makes actual SDK submissions independently.
use crate::{common::*, fixture_keys};
use anyhow::{Result, ensure};
use chialisp_oxide_simulator::{
    Coin, ErrorCode, Signature, Simulator, SpendBundle, coin_spend, puzzle_hash, sign_transaction,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    project: String,
    case: String,
    variant: String,
    mode: String,
    module_hex_path: PathBuf,
}
fn terms_batch(module: &[u8], head: &[u8; 32], count: u64) -> Result<Vec<u8>> {
    let m = puzzle_hash(module)?;
    encode(|a| {
        let m = a.new_atom(m.as_ref())?;
        let h = a.new_atom(head)?;
        let n = uint(a, count)?;
        list(a, &[m, h, n])
    })
}
fn batch_solution(
    first: &[u8; 32],
    middle: &[u8; 32],
    second: &[u8; 32],
    next: &[u8; 32],
) -> Result<Vec<u8>> {
    encode(|a| {
        let f = a.new_atom(first)?;
        let m = a.new_atom(middle)?;
        let s = a.new_atom(second)?;
        let n = a.new_atom(next)?;
        let x = list(a, &[f, m])?;
        let y = list(a, &[s, n])?;
        let batch = list(a, &[x, y])?;
        list(a, &[batch])
    })
}
fn batch(module: &[u8], case: &str, mutant: bool) -> Result<Value> {
    let recipients = [[0x11; 32], [0x22; 32], [0x33; 32], [0x44; 32]];
    let mut heads = [[0; 32]; 5];
    heads[4] = sha(&[b"chialisp-oxide/continuation/v1/end:1000"]);
    for i in (0..4).rev() {
        heads[i] = sha(&[
            b"chialisp-oxide/continuation/v1/item:1000",
            &recipients[i],
            &heads[i + 1],
        ]);
    }
    let count = if case == "terminal" { 2 } else { 4 };
    let second = if case == "second_opening" {
        [0x99; 32]
    } else {
        recipients[1]
    };
    let rev = curry(module, &terms_batch(module, &heads[0], count)?)?;
    let sol = batch_solution(&recipients[0], &heads[1], &second, &heads[2])?;
    let funding_amount = if case == "amount" { 4001 } else { count * 1000 };
    let mut sim = Simulator::new();
    let coin = sim.new_coin(puzzle_hash(&rev)?, funding_amount);
    let bundle = SpendBundle::new(vec![coin_spend(coin, &rev, &sol)?], Signature::default());
    if !mutant && matches!(case, "second_opening" | "terminal") {
        return Ok(
            json!({"curried_reveal_hex":hex::encode(&rev),"instance_solution_hex":hex::encode(&sol),"own_reveal_funding":coin_json(coin),"submission":reject(&mut sim,bundle,ErrorCode::GeneratorRuntimeError,"original_policy_runtime_umbrella")?,"native_or_VM_credit":false}),
        );
    }
    let asserted = if case == "amount" && mutant {
        4001
    } else {
        count * 1000
    };
    let mut cs = vec![
        (73, vec![amount(asserted)]),
        (51, vec![recipients[0].to_vec(), amount(1000)]),
        (51, vec![second.to_vec(), amount(1000)]),
    ];
    let mut outputs = vec![
        Coin::new(coin.coin_id(), recipients[0].into(), 1000),
        Coin::new(coin.coin_id(), second.into(), 1000),
    ];
    if count > 2 {
        let successor_head = if case == "successor" && mutant {
            heads[0]
        } else {
            heads[2]
        };
        let next = curry(module, &terms_batch(module, &successor_head, count - 2)?)?;
        let ph = puzzle_hash(&next)?;
        cs.push((51, vec![ph.as_ref().to_vec(), amount((count - 2) * 1000)]));
        outputs.push(Coin::new(coin.coin_id(), ph, (count - 2) * 1000));
    }
    exact_conditions(&rev, &sol, &cs)?;
    let submission = if case == "amount" && !mutant {
        json!({"status":"consensus_rejected","observation":reject(&mut sim,bundle,ErrorCode::AssertMyAmountFailed,"original_exact_amount")?})
    } else {
        json!({"status":"accepted","observation":accept(&mut sim,bundle,&outputs)?})
    };
    Ok(
        json!({"curried_reveal_hex":hex::encode(&rev),"instance_solution_hex":hex::encode(&sol),"own_reveal_funding":coin_json(coin),"submission":submission,"exact_VM_conditions_checked_separately":true}),
    )
}
fn spuds_reveal(module: &[u8], key: &[u8; 48], payout: &[u8; 32], round: u64) -> Result<Vec<u8>> {
    let m = puzzle_hash(module)?;
    let terms = encode(|a| {
        let m = a.new_atom(m.as_ref())?;
        let k = a.new_atom(key)?;
        let p = a.new_atom(payout)?;
        let r = uint(a, round)?;
        list(a, &[m, k, p, r])
    })?;
    curry(module, &terms)
}
fn spuds_solution(take: bool, key: &[u8; 48], payout: &[u8; 32]) -> Result<Vec<u8>> {
    encode(|a| {
        let t = uint(a, u64::from(take))?;
        let k = a.new_atom(key)?;
        let p = a.new_atom(payout)?;
        list(a, &[t, k, p])
    })
}
fn topup(
    sim: &mut Simulator,
    game: Coin,
    message: &[u8; 32],
) -> Result<(chialisp_oxide_simulator::CoinSpend, Coin)> {
    let id = sha(&[game.coin_id().as_ref(), message]);
    let rev = quoted(&[
        (61, vec![id.to_vec()]),
        (51, vec![[0xcc; 32].to_vec(), amount(1000)]),
    ])?;
    let coin = sim.new_coin(puzzle_hash(&rev)?, 2000);
    Ok((
        coin_spend(coin, &rev, &[0x80])?,
        Coin::new(coin.coin_id(), [0xcc; 32].into(), 1000),
    ))
}
fn spuds(module: &[u8], case: &str, mutant: bool) -> Result<Value> {
    let holder = fixture_keys::holder();
    let next = fixture_keys::next();
    let hk = holder.public_key().to_bytes();
    let nk = next.public_key().to_bytes();
    let payout = [0xa1; 32];
    let mut desired = [0xa2; 32];
    let taking = matches!(case, "nextstate" | "signature_binding");
    let rev = spuds_reveal(module, &hk, &payout, 1)?;
    let mut sim = Simulator::new();
    let input_amount = if case == "amount" { 1001 } else { 1000 };
    let coin = sim.new_coin(puzzle_hash(&rev)?, input_amount);
    let target_age = if taking || case == "age" { 1 } else { 5 };
    while sim.height()
        - sim
            .coin_state(coin.coin_id())
            .unwrap()
            .created_height
            .unwrap()
        < target_age
    {
        sim.create_block();
    }
    let claim_message = sha(&[b"chialisp-oxide/spuds/claim:v1"]);
    let mut old_signature = None;
    if case == "signature_binding" {
        let old_sol = spuds_solution(true, &nk, &desired)?;
        let old_spend = coin_spend(coin, &rev, &old_sol)?;
        old_signature = Some(
            sign_transaction(&[old_spend], std::slice::from_ref(&next))
                .map_err(|e| anyhow::anyhow!("old signature: {e:?}"))?,
        );
        desired = [0xa3; 32];
    }
    let sol = spuds_solution(taking, &nk, &desired)?;
    let output_payout = if case == "nextstate" && mutant {
        payout
    } else {
        desired
    };
    let successor = spuds_reveal(module, &nk, &output_payout, 2)?;
    let successor_ph = puzzle_hash(&successor)?;
    let take_message = sha(&[b"chialisp-oxide/spuds/take:v1", successor_ph.as_ref()]);
    let message = if taking && !(case == "signature_binding" && mutant) {
        take_message
    } else {
        claim_message
    };
    let announcement = if case == "announcement" && mutant {
        take_message
    } else {
        message
    };
    let signature_key = if taking { &next } else { &holder };
    let asserted = if case == "amount" && mutant {
        1001
    } else {
        1000
    };
    let boundary = if taking { 86 } else { 82 };
    let boundary_age = if case == "age" && mutant { 0 } else { 5 };
    let destination = if taking {
        successor_ph.to_bytes()
    } else {
        payout
    };
    let output_amount = if taking { 2000 } else { 1000 };
    let cs = vec![
        (
            50,
            vec![
                signature_key.public_key().to_bytes().to_vec(),
                message.to_vec(),
            ],
        ),
        (73, vec![amount(asserted)]),
        (82, vec![amount(1)]),
        (boundary, vec![amount(boundary_age)]),
        (51, vec![destination.to_vec(), amount(output_amount)]),
        (60, vec![announcement.to_vec()]),
    ];
    exact_conditions(&rev, &sol, &cs)?;
    let mut spends = vec![coin_spend(coin, &rev, &sol)?];
    let mut outputs = vec![Coin::new(coin.coin_id(), destination.into(), output_amount)];
    if taking || case == "announcement" {
        let protected_message = if case == "announcement" {
            take_message
        } else {
            message
        };
        let (funding, change) = topup(&mut sim, coin, &protected_message)?;
        spends.push(funding);
        outputs.push(change);
    }
    let sig = if let Some(sig) = old_signature {
        sig
    } else {
        sign_transaction(&spends, std::slice::from_ref(signature_key))
            .map_err(|e| anyhow::anyhow!("sign mutation: {e:?}"))?
    };
    let bundle = SpendBundle::new(spends, sig);
    let rejected = if mutant || case == "nextstate" {
        None
    } else {
        Some(match case {
            "amount" => ErrorCode::AssertMyAmountFailed,
            "age" => ErrorCode::AssertHeightRelativeFailed,
            "announcement" => ErrorCode::AssertCoinAnnouncementFailed,
            "signature_binding" => ErrorCode::BadAggregateSignature,
            _ => unreachable!(),
        })
    };
    let submission = if let Some(code) = rejected {
        json!({"status":"consensus_rejected","observation":reject(&mut sim,bundle,code,"original_policy_enforcement")?})
    } else {
        json!({"status":"accepted","observation":accept(&mut sim,bundle,&outputs)?})
    };
    Ok(
        json!({"curried_reveal_hex":hex::encode(&rev),"instance_solution_hex":hex::encode(&sol),"own_reveal_funding":coin_json(coin),"coin_age":target_age,"submission":submission,"exact_six_condition_VM_output_checked_separately":true,"topup_fixture":"exact announcement and fixed own change; not standard wallet integration"}),
    )
}
pub fn execute(input: &Path, output: &Path) -> Result<()> {
    let input: Input = serde_json::from_slice(&crate::read(input, 1_000_000)?)?;
    ensure!(
        !input.cases.is_empty() && input.cases.len() <= 64,
        "bounded mutation mode inventory"
    );
    let mut records = vec![];
    let mut ids = std::collections::BTreeSet::new();
    for row in input.cases {
        ensure!(
            matches!(row.variant.as_str(), "original" | "mutant")
                && matches!(row.mode.as_str(), "rust_on" | "rust_off"),
            "closed mutation variant/mode"
        );
        ensure!(
            ids.insert((
                row.project.clone(),
                row.case.clone(),
                row.variant.clone(),
                row.mode.clone()
            )),
            "unique mutation observation"
        );
        let names = match row.project.as_str() {
            "continuation-batches" => &["second_opening", "amount", "terminal", "successor"][..],
            "spuds" => &[
                "amount",
                "age",
                "nextstate",
                "announcement",
                "signature_binding",
            ][..],
            _ => anyhow::bail!("unknown mutation project"),
        };
        ensure!(names.contains(&row.case.as_str()), "closed source case");
        let module = crate::hex_program(&row.module_hex_path)?;
        let actual = if row.project == "spuds" {
            spuds(&module, &row.case, row.variant == "mutant")?
        } else {
            batch(&module, &row.case, row.variant == "mutant")?
        };
        records.push(json!({"project":row.project,"case":row.case,"variant":row.variant,"mode":row.mode,"module_hash":hex::encode(puzzle_hash(&module)?),"artifact_blob_sha256":hex::encode(sha(&[&module])),"actual":actual}));
    }
    fs::write(
        output,
        serde_json::to_vec_pretty(
            &json!({"protocol":"chialisp-oxide-stateful-mutation-sdk/v1","status":"passed","scope":"finite SDK submissions under each actual compiled own reveal; native and primary VM credit separately captured","records":records,"compiler_preservation":false,"standard_wallet_integration":false}),
        )?,
    )?;
    Ok(())
}
