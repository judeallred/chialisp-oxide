//! Offline, finite SDK successor-chain experiment; no wallet/network authority.
use anyhow::{Context, Result, ensure};
use chialisp_oxide_simulator::{
    CONSENSUS_VERSION, Coin, CoinSpend, ErrorCode, NETWORK, SIMULATOR_VERSION, Signature,
    Simulator, SimulatorError, SpendBundle, VM_VERSION, coin_spend, program, puzzle_hash,
};
use clvmr::{
    Allocator, NodePtr,
    serde::{node_from_bytes, node_to_bytes},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

// Reuse the reviewed actual generator/helper/BLS/backreference measurement.
// This is a separate host program; it does not relink the compiler driver.
mod controls;
#[path = "../../../spend-costs/src/measurement.rs"]
mod measurement;

const PAYOUT: u64 = 1_000;
const WRONG_SIGNATURE_SEED: [u8; 32] = [0x73; 32];
const RECIPIENTS: [[u8; 32]; 4] = [[0x11; 32], [0x22; 32], [0x33; 32], [0x44; 32]];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    programs: Vec<InputProgram>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputProgram {
    name: String,
    hex_path: PathBuf,
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= u64::try_from(limit)?,
        "bounded regular input before allocation"
    );
    let mut bytes = Vec::new();
    (&mut file)
        .take(u64::try_from(limit)? + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "input grew past byte budget");
    Ok(bytes)
}

fn sha(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for part in parts {
        h.update(part);
    }
    h.finalize().into()
}
fn heads() -> [[u8; 32]; 5] {
    let mut out = [[0; 32]; 5];
    out[4] = sha(&[b"chialisp-oxide/continuation/v1/end:1000"]);
    for i in (0..4).rev() {
        out[i] = sha(&[
            b"chialisp-oxide/continuation/v1/item:1000",
            &RECIPIENTS[i],
            &out[i + 1],
        ]);
    }
    out
}
fn list(a: &mut Allocator, items: &[NodePtr]) -> Result<NodePtr> {
    let mut tail = NodePtr::NIL;
    for value in items.iter().rev() {
        tail = a.new_pair(*value, tail)?;
    }
    Ok(tail)
}
fn terms(module: &[u8; 32], head: &[u8; 32], count: u8) -> Result<Vec<u8>> {
    ensure!((1..=8).contains(&count), "finite count");
    let mut a = Allocator::new();
    let m = a.new_atom(module)?;
    let h = a.new_atom(head)?;
    let n = a.new_atom(&[count])?;
    let root = list(&mut a, &[m, h, n])?;
    Ok(node_to_bytes(&a, root)?)
}
fn solution(recipient: &[u8; 32], next: &[u8; 32]) -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let to = a.new_atom(recipient)?;
    let digest = a.new_atom(next)?;
    let opening = list(&mut a, &[to, digest])?;
    let root = list(&mut a, &[opening])?;
    Ok(node_to_bytes(&a, root)?)
}
fn curry(module: &[u8], argument: &[u8]) -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let m = node_from_bytes(&mut a, module)?;
    let p = node_from_bytes(&mut a, argument)?;
    let one = a.new_atom(&[1])?;
    let two = a.new_atom(&[2])?;
    let four = a.new_atom(&[4])?;
    let qm = a.new_pair(one, m)?;
    let qp = a.new_pair(one, p)?;
    let env = list(&mut a, &[four, qp, one])?;
    let root = list(&mut a, &[two, qm, env])?;
    Ok(node_to_bytes(&a, root)?)
}
fn reveal(module: &[u8], queue: &[[u8; 32]; 5], index: usize) -> Result<Vec<u8>> {
    let m = puzzle_hash(module)?.to_bytes();
    curry(module, &terms(&m, &queue[index], u8::try_from(4 - index)?)?)
}
fn bundle(coin: Coin, module: &[u8], queue: &[[u8; 32]; 5], index: usize) -> Result<SpendBundle> {
    Ok(SpendBundle::new(
        vec![coin_spend(
            coin,
            &reveal(module, queue, index)?,
            &solution(&RECIPIENTS[index], &queue[index + 1])?,
        )?],
        Signature::default(),
    ))
}
fn coin_json(coin: Coin) -> Value {
    json!({"coin_id":hex::encode(coin.coin_id()),"parent_id":hex::encode(coin.parent_coin_info),
           "puzzle_hash":hex::encode(coin.puzzle_hash),"amount":coin.amount})
}
fn exact_rejection(
    sim: &mut Simulator,
    b: SpendBundle,
    code: ErrorCode,
    label: &str,
) -> Result<Value> {
    let before = sim.clone();
    let actual = sim.new_transaction(b);
    ensure!(
        matches!(actual,Err(SimulatorError::Validation(c)) if c==code),
        "{label}: expected {code:?}, got {actual:?}"
    );
    ensure!(
        *sim == before,
        "{label}: rejected transaction changed full simulator"
    );
    Ok(json!({"label":label,"code":format!("{code:?}"),"full_simulator_equal":true}))
}
fn runtime_umbrella(sim: &mut Simulator, b: SpendBundle, label: &str) -> Result<Value> {
    let before = sim.clone();
    let actual = sim.new_transaction(b);
    ensure!(
        matches!(
            actual,
            Err(SimulatorError::Validation(ErrorCode::GeneratorRuntimeError))
        ),
        "{label}: expected explicit runtime umbrella, got {actual:?}"
    );
    ensure!(
        *sim == before,
        "{label}: runtime failure changed full simulator"
    );
    Ok(
        json!({"label":label,"code":"GeneratorRuntimeError","classification":"inconclusive_runtime_umbrella",
        "full_simulator_equal":true,"policy_rejection_credit":false}),
    )
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    schema: u8,
    module_hash: [u8; 32],
    index: usize,
    coin_id: [u8; 32],
    amount: u64,
}
impl Cursor {
    fn new(module: &[u8], index: usize, coin: Coin) -> Result<Self> {
        Ok(Self {
            schema: 1,
            module_hash: puzzle_hash(module)?.to_bytes(),
            index,
            coin_id: coin.coin_id().to_bytes(),
            amount: coin.amount,
        })
    }
    fn verify(&self, module: &[u8], coin: Coin) -> Result<()> {
        ensure!(
            self.schema == 1
                && self.index < 4
                && self.module_hash == puzzle_hash(module)?.to_bytes()
                && self.coin_id == coin.coin_id().to_bytes()
                && self.amount == coin.amount
                && self.amount == PAYOUT * u64::try_from(4 - self.index)?,
            "cursor recipe/coin binding"
        );
        Ok(())
    }
}

fn lane(name: &str, module: &[u8], out: &Path) -> Result<Value> {
    let queue = heads();
    let mut sim = Simulator::new();
    let initial_reveal = reveal(module, &queue, 0)?;
    let mut current = sim.new_coin(puzzle_hash(&initial_reveal)?, 4 * PAYOUT);
    let initial = current;
    let mut negatives = Vec::new();
    let mut runtime = Vec::new();
    let mut steps = Vec::new();
    let mut resume_records = Vec::new();
    // Actual invalid-signature submission, even though no policy signatures
    // are required. An unrelated nonempty signature is not the valid identity.
    let valid = bundle(current, module, &queue, 0)?;
    let invalid_signature = chia_bls::sign(
        &chia_bls::SecretKey::from_seed(&WRONG_SIGNATURE_SEED),
        b"continuation wrong signature",
    );
    let mut bad = valid.clone();
    bad.aggregated_signature = invalid_signature;
    negatives.push(exact_rejection(
        &mut sim,
        bad,
        ErrorCode::BadAggregateSignature,
        "unrelated BLS aggregate",
    )?);
    // Wrong committed amount, with its own actually funded reveal.
    let wrong_amount = sim.new_coin(puzzle_hash(&initial_reveal)?, 4 * PAYOUT + 1);
    negatives.push(exact_rejection(
        &mut sim,
        bundle(wrong_amount, module, &queue, 0)?,
        ErrorCode::AssertMyAmountFailed,
        "wrong current amount",
    )?);
    // Bypass the host reveal helper deliberately: consensus sees the mismatch.
    let wrong_reveal = curry(module, &terms(&[0xbb; 32], &queue[0], 4)?)?;
    let bad = CoinSpend::new(
        current,
        program(&wrong_reveal)?,
        program(&solution(&RECIPIENTS[0], &queue[1])?)?,
    );
    negatives.push(exact_rejection(
        &mut sim,
        SpendBundle::new(vec![bad], Signature::default()),
        ErrorCode::WrongPuzzleHash,
        "wrong committed reveal",
    )?);
    for (label, recipient, next) in [
        ("skipped item", RECIPIENTS[1], queue[2]),
        ("reordered recipient", RECIPIENTS[1], queue[1]),
        ("injected recipient", [0x55; 32], queue[1]),
        ("wrong next digest", RECIPIENTS[0], queue[2]),
    ] {
        let bad = SpendBundle::new(
            vec![coin_spend(
                current,
                &initial_reveal,
                &solution(&recipient, &next)?,
            )?],
            Signature::default(),
        );
        runtime.push(runtime_umbrella(&mut sim, bad, label)?);
    }
    let mut cursor = Cursor::new(module, 0, current)?;
    for (index, recipient) in RECIPIENTS.iter().enumerate() {
        cursor.verify(module, current)?;
        ensure!(cursor.index == index, "ordered cursor");
        let before_height = sim.height();
        let accepted = bundle(current, module, &queue, index)?;
        let measured = measurement::measure(&accepted, before_height)?;
        let before = sim.clone();
        sim.new_transaction(accepted.clone())
            .map_err(|e| anyhow::anyhow!("{name} step {index}: {e:?}"))?;
        ensure!(
            sim.height() == before_height + 1 && sim != before,
            "accepted spend mines one simulator block"
        );
        let spent = sim
            .coin_state(current.coin_id())
            .context("spent coin state")?;
        ensure!(
            spent.spent_height == Some(before_height),
            "exact spent height"
        );
        let children = sim.children(current.coin_id());
        let payout = Coin::new(current.coin_id(), (*recipient).into(), PAYOUT);
        ensure!(
            children
                .iter()
                .any(|s| s.coin == payout && s.spent_height.is_none()),
            "exact payout child"
        );
        let next = if index < 3 {
            let expected = Coin::new(
                current.coin_id(),
                puzzle_hash(&reveal(module, &queue, index + 1)?)?,
                PAYOUT * u64::try_from(3 - index)?,
            );
            ensure!(
                children.len() == 2
                    && children
                        .iter()
                        .any(|s| s.coin == expected && s.spent_height.is_none()),
                "exact successor child"
            );
            Some(expected)
        } else {
            ensure!(children.len() == 1, "no terminal continuation");
            None
        };
        let record = json!({"index":index,"previous_transaction_height":before_height,"coin":coin_json(current),
            "payout":coin_json(payout),"successor":next.map(coin_json),"fees":0,"costs":measured});
        steps.push(record);
        negatives.push(exact_rejection(
            &mut sim,
            accepted,
            ErrorCode::DoubleSpend,
            "consumed coin resubmission",
        )?);
        if index == 0 {
            let next = next.context("checkpoint successor")?;
            let checkpoint = Cursor::new(module, index + 1, next)?;
            let path = out.join(format!("{name}-checkpoint.json"));
            fs::write(&path, serde_json::to_vec_pretty(&checkpoint)?)?;
            // Reconstruct the host cursor from persisted public bytes. The
            // simulator remains in memory: this is NOT durable node recovery.
            let restored: Cursor = serde_json::from_slice(&read_bounded(&path, 4096)?)?;
            restored.verify(module, next)?;
            ensure!(
                sim.coin_state(next.coin_id())
                    .context("resume coin")?
                    .spent_height
                    .is_none(),
                "unspent checkpoint"
            );
            cursor = restored;
            resume_records.push(json!({"kind":"persisted_cursor_in_process_resume","index":1,"coin_id":hex::encode(next.coin_id())}));
        } else if index == 1 {
            // Simulate another permissionless cranker consuming the checkpoint
            // coin while our restored cursor remains at that coin/index.
            ensure!(
                cursor.index == 1
                    && sim
                        .coin_state(current.coin_id())
                        .context("consumed cursor")?
                        .spent_height
                        .is_some(),
                "stale cursor consumed"
            );
            let actual = sim
                .coin_spend(current.coin_id())
                .context("retained consumed spend")?;
            ensure!(
                actual == bundle(current, module, &queue, index)?.coin_spends[0],
                "exact known consuming spend"
            );
            let next = next.context("reconciled successor")?;
            cursor = Cursor::new(module, index + 1, next)?;
            resume_records.push(json!({"kind":"consumed_coin_reconciled_without_push","from_index":1,"to_index":2,"coin_id":hex::encode(next.coin_id())}));
        } else if let Some(next) = next {
            cursor = Cursor::new(module, index + 1, next)?;
        }
        if let Some(next) = next {
            current = next;
        }
    }
    let total_fees = steps
        .iter()
        .map(|s| s["fees"].as_u64().unwrap())
        .sum::<u64>();
    let mut totals = serde_json::Map::new();
    for encoding in ["uncompressed", "backrefs"] {
        let costs = steps
            .iter()
            .map(|s| {
                s["costs"]["generators"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| r["encoding"] == encoding)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        totals.insert(encoding.into(),json!({"charged_bytes":costs.iter().map(|c|c["metrics"]["charged_bytes"].as_u64().unwrap()).sum::<u64>(),
            "total_cost":costs.iter().map(|c|c["metrics"]["total_cost"].as_u64().unwrap()).sum::<u64>()}));
    }
    Ok(
        json!({"name":name,"module_hash":hex::encode(puzzle_hash(module)?),"module_bytes":module.len(),
        "initial_coin":coin_json(initial),"steps":steps,"total_fees":total_fees,"chain_totals":totals,
        "resume":resume_records,"typed_rejections":negatives,"runtime_umbrellas":runtime,
        "counts":{"accepted_submissions":4,"typed_rejected_submissions":7,"runtime_umbrella_submissions":4,"successor_coins":3,"total_sdk_attempts":15}}),
    )
}

fn main() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() == 3 && args[0] == "--controls" {
        return controls::run(Path::new(&args[1]), Path::new(&args[2]));
    }
    ensure!(
        args.len() == 2,
        "usage: continuation-ledger INPUT.json NEW_OUTPUT_DIR"
    );
    let output = Path::new(&args[1]);
    ensure!(!output.exists(), "preserve prior output");
    fs::create_dir_all(output)?;
    let raw = read_bounded(Path::new(&args[0]), 16 * 1024)?;
    ensure!(raw.len() <= 16 * 1024, "bounded input inventory");
    let input: Input = serde_json::from_slice(&raw)?;
    let mut names = input
        .programs
        .iter()
        .map(|r| r.name.as_str())
        .collect::<Vec<_>>();
    names.sort_unstable();
    ensure!(
        names == ["chialisp", "rust_off", "rust_on"],
        "exact original three-build inventory"
    );
    let mut lanes = Vec::new();
    for row in input.programs {
        let text = String::from_utf8(read_bounded(&row.hex_path, 2 * 1024 * 1024)?)?;
        ensure!(text.len() <= 2 * 1024 * 1024, "bounded program input");
        let bytes = hex::decode(text.trim())?;
        if row.name == "chialisp" {
            ensure!(
                bytes == chialisp_oxide_reference::compile(include_str!("../../reference.clsp"))?,
                "fresh independent reference source"
            );
        }
        lanes.push(lane(&row.name, &bytes, output)?);
    }
    let report = json!({"schema":1,"scope":"finite offline simulator chain, not live network, generic continuation SDK or compiler preservation",
        "sdk":SIMULATOR_VERSION,"consensus":CONSENSUS_VERSION,"vm":VM_VERSION,"network":NETWORK,
        "lanes":lanes,"counts":{"scenario_runs":3,"accepted_submissions":12,"typed_rejected_submissions":21,
            "runtime_umbrella_submissions":12,"total_sdk_attempts":45,"successor_coins":9}});
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "3 builds,12 accepted,21 exact typed rejected,12 explicit runtime umbrellas;45 SDK attempts"
    );
    Ok(())
}
