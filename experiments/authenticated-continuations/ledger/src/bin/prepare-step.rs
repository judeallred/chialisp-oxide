//! Closed retained-policy offline preparation, not chain state authentication.
use anyhow::{Context, Result, ensure};
use chialisp_oxide_simulator::{Coin, Signature, Simulator, SpendBundle, coin_spend, puzzle_hash};
use clvmr::{
    Allocator, NodePtr,
    serde::{node_from_bytes, node_to_bytes},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::Read;
#[path = "../../../../spend-costs/src/measurement.rs"]
mod measurement;
const PAYOUT: u64 = 1000;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    schema: u8,
    network: String,
    mode: String,
    module_hex: String,
    recipients: Vec<String>,
    completed: usize,
    funding_parent: String,
    current_coin: InputCoin,
    previous_transaction_height: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputCoin {
    parent_id: String,
    puzzle_hash: String,
    amount: u64,
    coin_id: String,
}
fn fixed(value: &str) -> Result<[u8; 32]> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "canonical hex32"
    );
    hex::decode(value)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("hex32"))
}
fn sha(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for part in parts {
        h.update(part);
    }
    h.finalize().into()
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
fn coin_json(coin: Coin) -> Value {
    json!({"parent_id":hex::encode(coin.parent_coin_info),"puzzle_hash":hex::encode(coin.puzzle_hash),"amount":coin.amount,"coin_id":hex::encode(coin.coin_id())})
}
fn prepare(input: Input) -> Result<Value> {
    ensure!(
        input.schema == 1 && input.network == "testnet11",
        "fixed schema/testnet11"
    );
    let expected = match input.mode.as_str() {
        "on" => "b3329cbb066c43c84501a5aaf128343f052dadc6636a58fc35d02b6d333631c4",
        "off" => "08cad1983f3f7627f8dd5ffc8f03cedd10278c73c1f0df3d24ff7b9e2856ce68",
        _ => anyhow::bail!("exact retained Rust mode"),
    };
    ensure!(
        input.module_hex.len() <= 100_000
            && input
                .module_hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "bounded canonical modulehex"
    );
    let module = hex::decode(&input.module_hex)?;
    let expected_blob = if input.mode == "on" {
        "89aba3c87d0c3fa2f7db62bd0ad3f1074fd4ac49d6f4b245f3d585a4f0c560fb"
    } else {
        "be9f46fefcd859c025a9d9cc753ceb1a608783b85469b8d12d59dc6ad19ac955"
    };
    ensure!(
        sha(&[&module]) == fixed(expected_blob)?,
        "exact reviewed serialized module blob"
    );
    let module_hash = puzzle_hash(&module)?.to_bytes();
    ensure!(
        module_hash == fixed(expected)?,
        "exact reviewed retained module identity"
    );
    let mut a = Allocator::new();
    let root = node_from_bytes(&mut a, &module)?;
    ensure!(
        node_to_bytes(&a, root)? == module,
        "canonical module serialization"
    );
    ensure!(
        (1..=8).contains(&input.recipients.len()) && input.completed < input.recipients.len(),
        "bounded incomplete queue"
    );
    let recipients = input
        .recipients
        .iter()
        .map(|s| fixed(s))
        .collect::<Result<Vec<_>>>()?;
    let count = recipients.len();
    let mut queue = vec![[0; 32]; count + 1];
    queue[count] = sha(&[b"chialisp-oxide/continuation/v1/end:1000"]);
    for index in (0..count).rev() {
        queue[index] = sha(&[
            b"chialisp-oxide/continuation/v1/item:1000",
            &recipients[index],
            &queue[index + 1],
        ]);
    }
    let reveals = (0..count)
        .map(|index| {
            curry(
                &module,
                &terms(&module_hash, &queue[index], u8::try_from(count - index)?)?,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let mut parent = fixed(&input.funding_parent)?.into();
    let mut coins = Vec::new();
    for (index, reveal) in reveals.iter().enumerate() {
        let current = Coin::new(
            parent,
            puzzle_hash(reveal)?,
            PAYOUT * u64::try_from(count - index)?,
        );
        parent = current.coin_id();
        coins.push(current);
    }
    for index in 0..count - 1 {
        let payout = Coin::new(coins[index].coin_id(), recipients[index].into(), PAYOUT);
        ensure!(payout != coins[index + 1], "duplicate output coins");
    }
    let index = input.completed;
    let current = coins[index];
    ensure!(
        fixed(&input.current_coin.parent_id)? == current.parent_coin_info.to_bytes()
            && fixed(&input.current_coin.puzzle_hash)? == current.puzzle_hash.to_bytes()
            && input.current_coin.amount == current.amount
            && fixed(&input.current_coin.coin_id)? == current.coin_id().to_bytes(),
        "exact rooted queue current coin"
    );
    let solution = solution(&recipients[index], &queue[index + 1])?;
    let bundle = SpendBundle::new(
        vec![coin_spend(current, &reveals[index], &solution)?],
        Signature::default(),
    );
    // Exact pinned streamable layout, cross-checked against SDK bundle.name().
    let mut wire = Vec::new();
    wire.extend_from_slice(&1u32.to_be_bytes());
    wire.extend_from_slice(current.parent_coin_info.as_ref());
    wire.extend_from_slice(current.puzzle_hash.as_ref());
    wire.extend_from_slice(&current.amount.to_be_bytes());
    for program in [&reveals[index], &solution] {
        wire.extend_from_slice(program);
    }
    wire.extend_from_slice(&bundle.aggregated_signature.to_bytes());
    ensure!(
        sha(&[&wire]) == bundle.name().to_bytes(),
        "exact streamable bundle binding"
    );
    let measured = measurement::measure(&bundle, input.previous_transaction_height)?;
    let mut sim = Simulator::new();
    sim.insert_coin(current);
    sim.new_transaction(bundle.clone())
        .map_err(|e| anyhow::anyhow!("offline SDK preflight: {e:?}"))?;
    let payout = Coin::new(current.coin_id(), recipients[index].into(), PAYOUT);
    let mut expected_children = vec![payout];
    if index + 1 < count {
        expected_children.push(coins[index + 1]);
    }
    let children = sim.children(current.coin_id());
    ensure!(
        children.len() == expected_children.len()
            && expected_children.iter().all(|coin| children
                .iter()
                .any(|actual| actual.coin == *coin && actual.spent_height.is_none())),
        "exact SDK full output set"
    );
    ensure!(
        sim.coin_state(current.coin_id())
            .context("SDK spent coin")?
            .spent_height
            .is_some(),
        "SDK spent status"
    );
    Ok(
        json!({"schema":1,"scope":"offline supplied-public-coin preflight; no chain-state authentication or submission","network":"testnet11","mode":input.mode,"module_hash":expected,"module_blob_sha256":hex::encode(sha(&[&module])),"completed":index,"current_coin":coin_json(current),"children":expected_children.into_iter().map(coin_json).collect::<Vec<_>>(),"fees":0,"spend_bundle_hex":hex::encode(&wire),"spend_bundle_sha256":hex::encode(sha(&[&wire])),"spend_bundle_id":hex::encode(bundle.name()),"sdk_preflight":{"accepted":true,"inserted_public_coin":true,"simulator_previous_height":0},"costs":measured,"spend_bundle":{"coin_spends":[{"coin":{"parent_coin_info":hex::encode(current.parent_coin_info),"puzzle_hash":hex::encode(current.puzzle_hash),"amount":current.amount},"puzzle_reveal":hex::encode(&reveals[index]),"solution":hex::encode(solution)}],"aggregated_signature":hex::encode(bundle.aggregated_signature.to_bytes())}}),
    )
}
fn main() -> Result<()> {
    let mut input = Vec::new();
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    ensure!(args.len() == 1, "one public input JSON path");
    let mut file = std::fs::File::open(&args[0])?;
    let info = file.metadata()?;
    ensure!(
        info.is_file() && info.len() <= 262144,
        "bounded regular input"
    );
    (&mut file).take(262145).read_to_end(&mut input)?;
    ensure!(input.len() <= 262144, "bounded public JSON input");
    let result = prepare(serde_json::from_slice(&input)?)?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
