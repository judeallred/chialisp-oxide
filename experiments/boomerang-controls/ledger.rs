//! Finite SDK witnesses: fund each actual mutant's own compiled curried reveal.
use anyhow::{Context, Result, bail, ensure};
use chialisp_oxide_simulator::{
    Bytes32, CONSENSUS_VERSION, ConsensusRejection, NETWORK, SIMULATOR_VERSION, SecretKey,
    Signature, Simulator, SpendBundle, VM_VERSION, coin_spend, create_coin, puzzle_hash,
    require_consensus_rejection, sign_transaction,
};
use serde::Deserialize;
use serde_json::{Value, json};

const AMOUNT: u64 = 1_000;
const DEADLINE: u64 = 100;
const PROTOCOL: &str = "chialisp-oxide-boomerang-source-controls-ledger/v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: String,
    runs: Vec<Input>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    mode: String,
    name: String,
    witness: String,
    module_hex: String,
    instance_hex: String,
}

fn atom(payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() < 64, "fixed fixture codec only");
    if payload.len() == 1 && payload[0] < 128 {
        return payload.to_vec();
    }
    let mut value = vec![0x80 | u8::try_from(payload.len()).unwrap()];
    value.extend_from_slice(payload);
    value
}

fn pair(first: &[u8], rest: &[u8]) -> Vec<u8> {
    let mut value = vec![0xff];
    value.extend_from_slice(first);
    value.extend_from_slice(rest);
    value
}

fn list(values: &[Vec<u8>]) -> Vec<u8> {
    let mut value = vec![0x80];
    for first in values.iter().rev() {
        value = pair(first, &value);
    }
    value
}

fn instance(module: &[u8], key: &SecretKey) -> Vec<u8> {
    let terms = list(&[
        atom(&key.public_key().to_bytes()),
        atom(&[0x42; 32]),
        atom(&[0x43; 32]),
        atom(&[0x03, 0xe8]),
        atom(&[100]),
    ]);
    // Independent byte construction of (a (q . M) (c (q . TERMS) 1)).
    list(&[
        atom(&[2]),
        pair(&atom(&[1]), module),
        list(&[atom(&[4]), pair(&atom(&[1]), &terms), atom(&[1])]),
    ])
}

fn scenario(input: &Input) -> Result<(bool, u64, bool, Option<ConsensusRejection>)> {
    let (claim, timestamp, signed, rejection) = match input.witness.as_str() {
        "timely_unsigned_claim" => (
            true,
            DEADLINE - 1,
            false,
            ConsensusRejection::BadAggregateSignature,
        ),
        "signed_late_claim" => (
            true,
            DEADLINE + 1,
            true,
            ConsensusRejection::AssertBeforeSecondsAbsoluteFailed,
        ),
        "early_unsigned_refund" => (
            false,
            DEADLINE - 1,
            false,
            ConsensusRejection::AssertSecondsAbsoluteFailed,
        ),
        _ => bail!("unknown source-control witness"),
    };
    let expected = match (input.name.as_str(), input.witness.as_str()) {
        ("original" | "equivalent", _) => Some(rejection),
        ("drop_claim_signature", "timely_unsigned_claim")
        | ("late_claim", "signed_late_claim")
        | ("early_refund", "early_unsigned_refund") => None,
        _ => bail!("unexpected mutant/witness combination"),
    };
    Ok((claim, timestamp, signed, expected))
}

fn attempt(input: Input) -> Result<Value> {
    ensure!(matches!(input.mode.as_str(), "off" | "on"));
    ensure!(input.module_hex.len() <= 4_000_000 && input.instance_hex.len() <= 4_001_024);
    let module = hex::decode(&input.module_hex)?;
    let reveal = hex::decode(&input.instance_hex)?;
    let key = SecretKey::from_seed(&[0x17; 32]);
    ensure!(
        reveal == instance(&module, &key),
        "mutant instance terms or curry form changed"
    );
    let module_hash = puzzle_hash(&module)?;
    let instance_hash = puzzle_hash(&reveal)?;
    let (claim, timestamp, signed, expected) = scenario(&input)?;
    let solution = if claim {
        vec![0xff, 1, 0x80]
    } else {
        vec![0xff, 0x80, 0x80]
    };
    let mut simulator = Simulator::new();
    let coin = create_coin(&mut simulator, &reveal, AMOUNT)?;
    simulator.set_next_timestamp(timestamp)?;
    ensure!(simulator.next_timestamp() == timestamp);
    let spend = coin_spend(coin, &reveal, &solution)?;
    let signature = if signed {
        let signature = sign_transaction(std::slice::from_ref(&spend), &[key])?;
        ensure!(
            signature != Signature::default(),
            "late claim must carry an actual signature"
        );
        signature
    } else {
        Signature::default()
    };
    let signature_hex = hex::encode(signature.to_bytes());
    let before = simulator.clone();
    let result = simulator.new_transaction(SpendBundle::new(vec![spend], signature));
    let outcome = if let Some(expected) = expected {
        require_consensus_rejection(result, expected)?;
        ensure!(
            simulator == before,
            "exact typed rejection changed full SDK simulator state"
        );
        ensure!(simulator.children(coin.coin_id()).is_empty());
        json!({
            "status":"consensus_rejected", "code":expected.as_str(),
            "semantic_rejection_credit":true, "full_state_unchanged":true
        })
    } else {
        result.context("source mutant must admit its independently specified forbidden spend")?;
        let input_state = simulator
            .coin_state(coin.coin_id())
            .context("missing input state")?;
        ensure!(
            input_state.spent_height.is_some(),
            "accepted mutant did not spend input"
        );
        let children = simulator.children(coin.coin_id());
        ensure!(
            children.len() == 1,
            "accepted mutant must have exactly one child"
        );
        let child = children[0].coin;
        ensure!(child.parent_coin_info == coin.coin_id());
        ensure!(child.amount == AMOUNT);
        let expected_destination = Bytes32::new(if claim { [0x42; 32] } else { [0x43; 32] });
        ensure!(child.puzzle_hash == expected_destination);
        json!({
            "status":"accepted", "input_spent":true, "child_count":1,
            "child":{
                "coin_id":hex::encode(child.coin_id()),
                "parent_coin_id":hex::encode(child.parent_coin_info),
                "puzzle_hash":hex::encode(child.puzzle_hash), "amount":child.amount
            }
        })
    };
    Ok(json!({
        "mode":input.mode, "name":input.name, "witness":input.witness,
        "claim":claim, "timestamp":timestamp, "signing":if signed { "recipient" } else { "none" },
        "module_tree_hash":hex::encode(module_hash), "instance_tree_hash":hex::encode(instance_hash),
        "instance_hex":input.instance_hex, "solution_hex":hex::encode(solution),
        "signature_hex":signature_hex,
        "input":{
            "coin_id":hex::encode(coin.coin_id()), "parent_coin_id":hex::encode(coin.parent_coin_info),
            "puzzle_hash":hex::encode(coin.puzzle_hash), "amount":coin.amount
        },
        "outcome":outcome
    }))
}

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    ensure!(args.len() == 2, "usage: ledger REQUEST.json");
    let metadata = std::fs::metadata(&args[1])?;
    ensure!(
        metadata.is_file() && metadata.len() <= 32 * 1024 * 1024,
        "bounded regular request"
    );
    let bytes = std::fs::read(&args[1])?;
    ensure!(bytes.len() <= 32 * 1024 * 1024, "bounded request bytes");
    let request: Request = serde_json::from_slice(&bytes)?;
    ensure!(request.protocol == PROTOCOL && request.runs.len() == 18);
    let attempts = request
        .runs
        .into_iter()
        .map(attempt)
        .collect::<Result<Vec<_>>>()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "protocol":PROTOCOL, "simulator":SIMULATOR_VERSION, "consensus":CONSENSUS_VERSION,
            "vm":VM_VERSION, "network":NETWORK,
            "scenario_runs":18, "transaction_attempts":18, "attempts":attempts
        }))?
    );
    Ok(())
}
