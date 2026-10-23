//! Disposable TESTNET11 SDK fixtures only. Never submits network transactions.
use anyhow::{Context, Result, ensure};
use chialisp_oxide_backend_experiment::{Outcome, execute};
use chialisp_oxide_simulator::{
    CONSENSUS_VERSION, CoinSpend, ConsensusRejection, NETWORK, SIMULATOR_VERSION, SecretKey,
    Signature, Simulator, SimulatorError, SpendBundle, VM_VERSION, coin_spend, create_coin,
    puzzle_hash, require_consensus_rejection, sign_single_requirement_with_wrong_key,
    sign_transaction,
};
use serde::Deserialize;
use serde_json::json;
use std::io::Read;

const LIMIT: usize = 4 * 1024 * 1024;
const DESTINATION: [u8; 32] = [0x42; 32];
const AMOUNT: u64 = 1_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: String,
    canonical_module_hex: String,
    runs: Vec<Input>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    mode: String,
    module_hex: String,
}

fn atom(bytes: &[u8]) -> Vec<u8> {
    assert!(bytes.len() < 64, "fixed witness atom bound");
    if bytes.len() == 1 && bytes[0] < 128 {
        return bytes.to_vec();
    }
    let mut result = vec![0x80 | u8::try_from(bytes.len()).unwrap()];
    result.extend_from_slice(bytes);
    result
}
fn pair(first: &[u8], rest: &[u8]) -> Vec<u8> {
    let mut result = vec![0xff];
    result.extend_from_slice(first);
    result.extend_from_slice(rest);
    result
}
fn list(values: &[Vec<u8>]) -> Vec<u8> {
    values
        .iter()
        .rev()
        .fold(vec![0x80], |tail, value| pair(value, &tail))
}
fn curry(module: &[u8], key: &[u8; 48]) -> Vec<u8> {
    list(&[
        atom(&[2]),
        pair(&atom(&[1]), module),
        list(&[atom(&[4]), pair(&atom(&[1]), &atom(key)), atom(&[1])]),
    ])
}
fn solution() -> Vec<u8> {
    let conditions = list(&[list(&[atom(&[51]), atom(&DESTINATION), atom(&[3, 0xe8])])]);
    list(&[atom(&[]), pair(&atom(&[1]), &conditions), atom(&[])])
}

fn transaction(spends: &[CoinSpend], signature: &Signature) -> serde_json::Value {
    json!({"signature_hex":hex::encode(signature.to_bytes()), "spends":spends.iter().map(|spend|
        json!({"coin":{"coin_id":hex::encode(spend.coin.coin_id()),"parent_coin_id":hex::encode(spend.coin.parent_coin_info),
            "puzzle_hash":hex::encode(spend.coin.puzzle_hash),"amount":spend.coin.amount},
            "reveal_hex":hex::encode(spend.puzzle_reveal.as_ref()),"solution_hex":hex::encode(spend.solution.as_ref())})
        ).collect::<Vec<_>>()})
}

fn main() -> Result<()> {
    let args = std::env::args_os().collect::<Vec<_>>();
    ensure!(args.len() == 2, "usage: ledger REQUEST.json");
    let file = std::fs::File::open(&args[1])?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= LIMIT as u64,
        "bounded regular request"
    );
    let mut bytes = Vec::new();
    file.take((LIMIT + 1) as u64).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= LIMIT, "request grew beyond bound");
    let request: Request = serde_json::from_slice(&bytes)?;
    ensure!(
        request.protocol == "chialisp-oxide-registry-standard-wallet-request/v1",
        "request protocol"
    );
    ensure!(
        request.runs.len() == 2 && request.runs[0].mode == "off" && request.runs[1].mode == "on",
        "paired compiler modes"
    );
    ensure!(
        request.canonical_module_hex.len() == 454,
        "exact canonical module byte width"
    );
    let canonical = hex::decode(request.canonical_module_hex)?;
    ensure!(
        hex::encode(puzzle_hash(&canonical)?)
            == "e9aaa49f45bad5c889b86ee3341550c155cfdd10c3a6757de618d20612fffd52",
        "canonical module identity"
    );
    let owner = SecretKey::from_seed(&[0x53; 32]);
    let wrong_key = SecretKey::from_seed(&[0x54; 32]);
    // This deliberately selected public test secret serves directly as the
    // synthetic key. It is unrelated to the observed mainnet coin's owner.
    let key = owner.public_key().to_bytes();
    let solution = solution();
    let solution_text = format!("(() (q . ((51 0x{} 1000))) ())", hex::encode(DESTINATION));
    let canonical_reveal = curry(&canonical, &key);
    let expected_vm = execute(&canonical_reveal, &solution_text, 10_000_000)?;
    let expected_output = match &expected_vm.outcome {
        Outcome::Accepted { output_hex } => output_hex.clone(),
        _ => anyhow::bail!("canonical witness did not produce conditions"),
    };
    let mut canonical_primary = Some(expected_vm);
    let mut inputs = request.runs;
    inputs.push(Input {
        mode: "canonical_reference".into(),
        module_hex: hex::encode(&canonical),
    });
    let mut builds = Vec::new();
    for input in inputs {
        ensure!(
            !input.module_hex.is_empty() && input.module_hex.len() <= LIMIT / 2,
            "bounded candidate before hex decode"
        );
        let module = hex::decode(input.module_hex)?;
        let module_hash = puzzle_hash(&module)?;
        if input.mode != "canonical_reference" {
            ensure!(
                module_hash != puzzle_hash(&canonical)?,
                "wrapper identity must remain distinct"
            );
        }
        let reveal = curry(&module, &key);
        let reveal_hash = puzzle_hash(&reveal)?;
        let primary = if input.mode == "canonical_reference" {
            canonical_primary
                .take()
                .context("canonical observation already consumed")?
        } else {
            execute(&reveal, &solution_text, 10_000_000)?
        };
        ensure!(
            matches!(&primary.outcome, Outcome::Accepted {output_hex} if output_hex == &expected_output),
            "actual wrapper output differs from independently executed canonical dependency"
        );
        let malformed = execute(&reveal, "()", 10_000_000)?;
        let encoded_malformed = serde_json::to_value(&malformed)?;
        ensure!(
            encoded_malformed["outcome"]["status"] == "rejected"
                && encoded_malformed["failure"]["kind"] == "program"
                && encoded_malformed["failure"]["code"] == "path_into_atom",
            "malformed solution requires typed primary-VM program failure"
        );
        let mut simulator = Simulator::new();
        // The key is already committed in reveal before any coin is created.
        let coin = create_coin(&mut simulator, &reveal, AMOUNT)?;
        let spend = coin_spend(coin, &reveal, &solution)?;
        let spends = vec![spend];
        let invalid_message = chia_bls::sign(&owner, b"registry-standard-wallet/wrong-message");
        let wrong = sign_single_requirement_with_wrong_key(&spends, &wrong_key)?;
        ensure!(
            invalid_message != Signature::default() && wrong != Signature::default(),
            "actual nonempty invalid signatures"
        );
        let mut attempts = Vec::new();
        for (label, signature) in [
            ("empty_signature", Signature::default()),
            ("wrong_key_signature", wrong),
            ("wrong_message_signature", invalid_message),
        ] {
            let submitted = transaction(&spends, &signature);
            let before = simulator.clone();
            require_consensus_rejection(
                simulator.new_transaction(SpendBundle::new(spends.clone(), signature)),
                ConsensusRejection::BadAggregateSignature,
            )?;
            ensure!(
                simulator == before,
                "full simulator state changed on signature rejection"
            );
            attempts.push(json!({"name":label,"status":"consensus_rejected","code":"BadAggregateSignature","full_simulator_equality":true,"transaction":submitted}));
        }
        let malformed_spend = coin_spend(coin, &reveal, &[0x80])?;
        let malformed_transaction = transaction(
            std::slice::from_ref(&malformed_spend),
            &Signature::default(),
        );
        let before = simulator.clone();
        ensure!(
            matches!(
                simulator.new_transaction(SpendBundle::new(
                    vec![malformed_spend],
                    Signature::default()
                )),
                Err(SimulatorError::Validation(
                    chialisp_oxide_simulator::ErrorCode::GeneratorRuntimeError
                ))
            ),
            "expected SDK runtime umbrella"
        );
        ensure!(
            simulator == before,
            "full simulator state changed on runtime umbrella"
        );
        attempts.push(json!({"name":"malformed_solution","status":"runtime_umbrella","code":"GeneratorRuntimeError","semantic_rejection_credit":false,"full_simulator_equality":true,"transaction":malformed_transaction}));
        let signature = sign_transaction(&spends, std::slice::from_ref(&owner))?;
        let signed_transaction = transaction(&spends, &signature);
        simulator.new_transaction(SpendBundle::new(spends.clone(), signature.clone()))?;
        let children = simulator.children(coin.coin_id());
        ensure!(
            children.len() == 1
                && children[0].coin.parent_coin_info == coin.coin_id()
                && children[0].coin.puzzle_hash == DESTINATION.into()
                && children[0].coin.amount == AMOUNT,
            "exact accepted child differs"
        );
        let child = children[0].coin;
        ensure!(
            simulator
                .coin_state(coin.coin_id())
                .context("input state missing")?
                .spent_height
                .is_some(),
            "input must be consumed"
        );
        attempts.push(json!({"name":"signed_complete_spend","status":"accepted","input_coin_id":hex::encode(coin.coin_id()),"transaction":signed_transaction,
            "child":{"coin_id":hex::encode(child.coin_id()),"parent_coin_id":hex::encode(child.parent_coin_info),"puzzle_hash":hex::encode(child.puzzle_hash),"amount":child.amount}}));
        let before = simulator.clone();
        require_consensus_rejection(
            simulator.new_transaction(SpendBundle::new(spends, signature)),
            ConsensusRejection::DoubleSpend,
        )?;
        ensure!(
            simulator == before,
            "full simulator state changed on replay rejection"
        );
        attempts.push(json!({"name":"consumed_input_replay","status":"consensus_rejected","code":"DoubleSpend","full_simulator_equality":true,"transaction":signed_transaction}));
        builds.push(json!({"mode":input.mode,"module_tree_hash":hex::encode(module_hash),"instance_tree_hash":hex::encode(reveal_hash),
            "committed_synthetic_key":hex::encode(key),"solution_hex":hex::encode(&solution),"primary_vm":primary,"malformed_primary_vm":malformed,
            "sdk_transaction_attempts":attempts.len(),"attempts":attempts}));
    }
    println!(
        "{}",
        json!({"protocol":"chialisp-oxide-registry-standard-wallet-ledger/v1","status":"executed",
        "sdk":SIMULATOR_VERSION,"consensus":CONSENSUS_VERSION,"sdk_vm":VM_VERSION,"network":NETWORK,
        "sdk_transaction_attempts":18,"accepted":3,"typed_consensus_rejected":12,"runtime_umbrellas":3,"builds":builds,
        "scope":"Two Rust wrapper modes plus separately executed canonical reference. All coins commit their own reveal and chosen synthetic key before funding; signatures are disposable fixture data. Native, primary-VM and SDK lanes are separate. No network submission, authenticated execution or compiler-preservation claim."})
    );
    Ok(())
}
