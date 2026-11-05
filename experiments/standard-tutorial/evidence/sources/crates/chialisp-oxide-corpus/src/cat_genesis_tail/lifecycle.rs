//! Shared CAT2 simulator lifecycle. A TAIL is a component, never a whole CAT2 port.
//! No network I/O. Resource/codec failures are inconclusive; every rejected
//! actual submission checks exact typed outcomes and full in-memory state equality.
use anyhow::{Context, Result, ensure};
use chia_bls::PublicKey;
use chia_puzzle_types::{
    CoinProof, LineageProof, Memos,
    cat::{CatArgs, CatSolution, GenesisByCoinIdTailArgs},
    standard::{StandardArgs, StandardSolution},
};
use chia_sdk_types::{
    Conditions,
    conditions::{CreateCoin, RunCatTail},
};
use chialisp_oxide_simulator::{
    Bytes32, Coin, CoinSpend, ErrorCode, SecretKey, Signature, Simulator, SimulatorError,
    SpendBundle, coin_spend, create_coin, puzzle_hash, sign_single_requirement_with_wrong_key,
    sign_transaction,
};
use chialisp_oxide_vm::classify_failure;
use clvm_traits::ToClvm;
use clvm_utils::TreeHash;
use clvmr_sdk::{
    Allocator, NodePtr,
    serde::{node_from_bytes, node_to_bytes},
};
use serde_json::{Value, json};

const AMOUNT: u64 = 1_000;
/// Immutable pinned SDK bytes; callers separately compile original source.
pub const CANONICAL_CAT_MODULE: &[u8] = &chia_puzzles::CAT_PUZZLE;
pub const CANONICAL_TAIL_MODULE: &[u8] = &chia_puzzles::GENESIS_BY_COIN_ID;

/// Exactly the original fifteen-attempt main lifecycle record, without mutants.
/// Construction is restricted to actual successful execution of this harness.
#[derive(serde::Serialize)]
#[serde(transparent)]
pub struct LifecycleJournal(Value);
impl LifecycleJournal {
    pub fn as_json(&self) -> &Value {
        &self.0
    }
    pub fn into_json(self) -> Value {
        self.0
    }
}
impl std::ops::Deref for LifecycleJournal {
    type Target = Value;
    fn deref(&self) -> &Value {
        &self.0
    }
}

fn admit_module(module: &[u8], max_cost: u64) -> Result<()> {
    ensure!(
        max_cost > 0 && max_cost <= 250_000_000,
        "explicit bounded primary component cost"
    );
    // Reserve the closed one-Bytes32-atom curry before any SDK tree copies.
    // This host admission is not a policy guard or consensus rejection.
    chialisp_oxide_package::artifact::inspect_artifact(
        module,
        chialisp_oxide_package::artifact::Encoding::Plain,
        &chialisp_oxide_package::Limits {
            max_blob_bytes: 1024 * 1024 - 128,
            max_nodes: 65_536 - 32,
            max_depth: 512 - 3,
            max_atom_bytes: 1024 * 1024 - 128,
            max_atom_heap_bytes: 2 * 1024 * 1024,
            max_expanded_bytes: 1024 * 1024 - 128,
            max_work: 16 * 1024 * 1024,
            ..Default::default()
        },
    )?;
    Ok(())
}

/// Execute one main protocol lifecycle with this selected TAIL module.
pub fn run_lifecycle(
    module: &[u8],
    owner: &SecretKey,
    recipient: &SecretKey,
    max_cost: u64,
) -> Result<LifecycleJournal> {
    admit_module(module, max_cost)?;
    let journal = canonical_lifecycle(module, owner, recipient, max_cost)?;
    ensure!(
        journal["attempts"]
            .as_array()
            .is_some_and(|rows| rows.len() == 15),
        "exact main lifecycle attempt inventory"
    );
    Ok(LifecycleJournal(journal))
}

fn encode<T: ToClvm<Allocator>>(allocator: &mut Allocator, value: &T) -> Result<Vec<u8>> {
    let node = value.to_clvm(allocator)?;
    Ok(node_to_bytes(allocator, node)?)
}

fn standard(key: PublicKey) -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let program = node_from_bytes(&mut a, &chia_puzzles::P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE)?;
    let bytes = encode(
        &mut a,
        &clvm_utils::CurriedProgram {
            program,
            args: StandardArgs::new(key),
        },
    )?;
    ensure!(
        puzzle_hash(&bytes)?.to_bytes() == StandardArgs::curry_tree_hash(key).to_bytes(),
        "standard curry/hash disagreement"
    );
    Ok(bytes)
}

fn tail(module: &[u8], genesis: Bytes32) -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let program = node_from_bytes(&mut a, module)?;
    encode(
        &mut a,
        &clvm_utils::CurriedProgram {
            program,
            args: GenesisByCoinIdTailArgs::new(genesis),
        },
    )
}

fn cat(asset_id: Bytes32, inner: &[u8]) -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let program = node_from_bytes(&mut a, &chia_puzzles::CAT_PUZZLE)?;
    let inner_puzzle = node_from_bytes(&mut a, inner)?;
    let bytes = encode(
        &mut a,
        &clvm_utils::CurriedProgram {
            program,
            args: CatArgs::new(asset_id, inner_puzzle),
        },
    )?;
    let independently_hashed =
        CatArgs::curry_tree_hash(asset_id, TreeHash::new(puzzle_hash(inner)?.to_bytes()));
    ensure!(
        puzzle_hash(&bytes)?.to_bytes() == independently_hashed.to_bytes(),
        "CAT curry/hash disagreement"
    );
    Ok(bytes)
}

fn standard_solution(
    destination: Option<(Bytes32, u64)>,
    run_tail: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let mut conditions = Conditions::new();
    if let Some((puzzle_hash, amount)) = destination {
        conditions = conditions.with(CreateCoin::new(puzzle_hash, amount, Memos::<NodePtr>::None));
    }
    if let Some(reveal) = run_tail {
        let program = node_from_bytes(&mut a, reveal)?;
        conditions = conditions.with(RunCatTail::new(program, NodePtr::NIL));
    }
    encode(&mut a, &StandardSolution::from_conditions(conditions))
}

#[derive(Clone, Copy)]
struct CatWitness {
    lineage: Option<LineageProof>,
    prev: Bytes32,
    next: CoinProof,
    extra_delta: i64,
}

fn self_ring(coin: Coin, inner: &[u8], lineage: Option<LineageProof>) -> Result<CatWitness> {
    Ok(CatWitness {
        lineage,
        prev: coin.coin_id(),
        next: CoinProof {
            parent_coin_info: coin.parent_coin_info,
            inner_puzzle_hash: puzzle_hash(inner)?,
            amount: coin.amount,
        },
        extra_delta: 0,
    })
}

fn cat_solution(coin: Coin, witness: CatWitness, inner_solution: &[u8]) -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let inner_puzzle_solution = node_from_bytes(&mut a, inner_solution)?;
    encode(
        &mut a,
        &CatSolution {
            inner_puzzle_solution,
            lineage_proof: witness.lineage,
            prev_coin_id: witness.prev,
            this_coin_info: coin,
            next_coin_proof: witness.next,
            prev_subtotal: 0,
            extra_delta: witness.extra_delta,
        },
    )
}

fn record_coin(coin: Coin) -> Value {
    json!({"coin_id":hex::encode(coin.coin_id()),"parent_coin_id":hex::encode(coin.parent_coin_info),
        "puzzle_hash":hex::encode(coin.puzzle_hash),"amount":coin.amount})
}

fn transaction(spends: &[CoinSpend], signature: &Signature) -> Value {
    json!({"signature_hex":hex::encode(signature.to_bytes()),"spends":spends.iter().map(|spend|json!({
        "coin":record_coin(spend.coin),"reveal_hex":hex::encode(spend.puzzle_reveal.as_ref()),
        "solution_hex":hex::encode(spend.solution.as_ref())})).collect::<Vec<_>>()})
}

fn primary(spend: &CoinSpend, max_cost: u64) -> Result<Value> {
    let mut a = clvmr::Allocator::new_limited(8 * 1024 * 1024);
    let program = clvmr::serde::node_from_bytes(&mut a, spend.puzzle_reveal.as_ref())?;
    let solution = clvmr::serde::node_from_bytes(&mut a, spend.solution.as_ref())?;
    let result = clvmr::run_program(
        &mut a,
        &clvmr::ChiaDialect::new(clvmr::chia_dialect::MEMPOOL_MODE),
        program,
        solution,
        max_cost,
    );
    Ok(match result {
        Ok(result) => {
            json!({"outcome":{"status":"accepted","output_hex":hex::encode(clvmr::serde::node_to_bytes(&a,result.1)?)},"cost":result.0,"failure":null})
        }
        Err(error) => {
            let failure = classify_failure(&error);
            ensure!(
                failure.is_program_rejection(),
                "inconclusive primary VM resource/infrastructure failure: {error}"
            );
            json!({"outcome":{"status":"rejected"},"cost":null,"failure":failure})
        }
    })
}

enum Expected {
    Accepted,
    Consensus(ErrorCode),
    Runtime(&'static str),
}

fn submit(
    simulator: &mut Simulator,
    attempts: &mut Vec<Value>,
    label: &str,
    spends: Vec<CoinSpend>,
    signature: Signature,
    expected: Expected,
    max_cost: u64,
) -> Result<()> {
    let executions = spends
        .iter()
        .map(|spend| primary(spend, max_cost))
        .collect::<Result<Vec<_>>>()?;
    let before = simulator.clone();
    let retained = transaction(&spends, &signature);
    eprintln!(
        "CAT_LIFECYCLE_PENDING {}",
        json!({"name":label,"stage":"before_submission","transaction":retained,"primary_vm":executions})
    );
    let result = simulator.new_transaction(SpendBundle::new(spends, signature));
    let observed = match &result {
        Ok(_) => "accepted".to_owned(),
        Err(error) => format!("{error:?}"),
    };
    let outcome = match expected {
        Expected::Accepted => {
            result?;
            ensure!(
                executions
                    .iter()
                    .all(|row| row["outcome"]["status"] == "accepted"),
                "accepted SDK bundle had failed primary VM"
            );
            json!({"status":"accepted"})
        }
        Expected::Consensus(expected) => {
            ensure!(
                matches!(result, Err(SimulatorError::Validation(actual)) if actual==expected),
                "{label}: expected exact consensus {expected:?}, got {observed}"
            );
            ensure!(
                *simulator == before,
                "{label}: full simulator changed on consensus rejection"
            );
            ensure!(
                executions
                    .iter()
                    .all(|row| row["outcome"]["status"] == "accepted"),
                "typed consensus rejection must reach executable conditions"
            );
            json!({"status":"consensus_rejected","code":format!("{expected:?}"),"full_simulator_equality":true})
        }
        Expected::Runtime(code) => {
            ensure!(
                matches!(
                    result,
                    Err(SimulatorError::Validation(ErrorCode::GeneratorRuntimeError))
                ),
                "{label}: expected SDK runtime umbrella, got {observed}"
            );
            ensure!(
                *simulator == before,
                "{label}: full simulator changed on runtime umbrella"
            );
            ensure!(
                executions.iter().any(
                    |row| row["failure"]["kind"] == "program" && row["failure"]["code"] == code
                ),
                "{label}: intended typed primary VM failure missing"
            );
            json!({"status":"runtime_umbrella","code":"GeneratorRuntimeError","semantic_rejection_credit":false,"primary_program_code":code,"full_simulator_equality":true})
        }
    };
    let record =
        json!({"name":label,"transaction":retained,"primary_vm":executions,"outcome":outcome});
    eprintln!("CAT_LIFECYCLE_ATTEMPT {record}");
    attempts.push(record);
    Ok(())
}

fn signed(spends: &[CoinSpend], key: &SecretKey) -> Result<Signature> {
    Ok(sign_transaction(spends, std::slice::from_ref(key))?)
}

fn child(simulator: &Simulator, parent: Coin, reveal: &[u8], amount: u64) -> Result<Coin> {
    let children = simulator.children(parent.coin_id());
    ensure!(children.len() == 1, "expected exactly one child");
    let coin = children[0].coin;
    ensure!(
        coin.parent_coin_info == parent.coin_id()
            && coin.puzzle_hash == puzzle_hash(reveal)?
            && coin.amount == amount,
        "exact child/asset/amount differs"
    );
    ensure!(
        simulator
            .coin_state(parent.coin_id())
            .context("missing spent input")?
            .spent_height
            .is_some(),
        "input not consumed"
    );
    Ok(coin)
}

fn lineage(parent: Coin, parent_inner: &[u8]) -> Result<LineageProof> {
    Ok(LineageProof {
        parent_parent_coin_info: parent.parent_coin_info,
        parent_inner_puzzle_hash: puzzle_hash(parent_inner)?,
        parent_amount: parent.amount,
    })
}

fn canonical_lifecycle(
    tail_module: &[u8],
    owner: &SecretKey,
    recipient: &SecretKey,
    max_cost: u64,
) -> Result<Value> {
    let mut simulator = Simulator::new();
    let owner_inner = standard(owner.public_key())?;
    let recipient_inner = standard(recipient.public_key())?;
    let genesis = create_coin(&mut simulator, &owner_inner, AMOUNT)?;
    let tail_reveal = tail(tail_module, genesis.coin_id())?;
    let asset_id = puzzle_hash(&tail_reveal)?;
    let owner_cat = cat(asset_id, &owner_inner)?;
    let recipient_cat = cat(asset_id, &recipient_inner)?;
    let mut attempts = Vec::new();
    let funding = vec![coin_spend(
        genesis,
        &owner_inner,
        &standard_solution(Some((puzzle_hash(&owner_cat)?, AMOUNT)), None)?,
    )?];
    let funding_signature = signed(&funding, owner)?;
    submit(
        &mut simulator,
        &mut attempts,
        "fund_genesis_to_cat_eve",
        funding.clone(),
        funding_signature.clone(),
        Expected::Accepted,
        max_cost,
    )?;
    let eve = child(&simulator, genesis, &owner_cat, AMOUNT)?;
    let ring = self_ring(eve, &owner_inner, None)?;
    let issuance_inner = standard_solution(
        Some((puzzle_hash(&owner_inner)?, AMOUNT)),
        Some(&tail_reveal),
    )?;
    let issuance_solution = cat_solution(eve, ring, &issuance_inner)?;
    let issuance = vec![coin_spend(eve, &owner_cat, &issuance_solution)?];
    submit(
        &mut simulator,
        &mut attempts,
        "eve_empty_owner_signature",
        issuance.clone(),
        Signature::default(),
        Expected::Consensus(ErrorCode::BadAggregateSignature),
        max_cost,
    )?;
    let wrong = sign_single_requirement_with_wrong_key(&issuance, recipient)?;
    submit(
        &mut simulator,
        &mut attempts,
        "eve_nonempty_wrong_key_signature",
        issuance.clone(),
        wrong,
        Expected::Consensus(ErrorCode::BadAggregateSignature),
        max_cost,
    )?;
    let wrong_message = chia_bls::sign(owner, b"cat-genesis-lifecycle/wrong-message");
    ensure!(
        wrong_message != Signature::default(),
        "actual nonempty invalid signature"
    );
    submit(
        &mut simulator,
        &mut attempts,
        "eve_nonempty_wrong_message_signature",
        issuance.clone(),
        wrong_message,
        Expected::Consensus(ErrorCode::BadAggregateSignature),
        max_cost,
    )?;
    let mut wrong_ring = ring;
    wrong_ring.next.parent_coin_info = Bytes32::new([0x91; 32]);
    let bad = vec![coin_spend(
        eve,
        &owner_cat,
        &cat_solution(eve, wrong_ring, &issuance_inner)?,
    )?];
    let sig = signed(&bad, owner)?;
    submit(
        &mut simulator,
        &mut attempts,
        "eve_wrong_ring_successor",
        bad,
        sig,
        Expected::Consensus(ErrorCode::AssertCoinAnnouncementFailed),
        max_cost,
    )?;
    let mut nonzero = ring;
    nonzero.extra_delta = 1;
    let bad = vec![coin_spend(
        eve,
        &owner_cat,
        &cat_solution(eve, nonzero, &issuance_inner)?,
    )?];
    submit(
        &mut simulator,
        &mut attempts,
        "eve_nonzero_extra_delta",
        bad,
        Signature::default(),
        Expected::Runtime("raise"),
        max_cost,
    )?;
    let bad_inner = standard_solution(
        Some((puzzle_hash(&owner_inner)?, AMOUNT)),
        Some(tail_module),
    )?;
    let bad = vec![coin_spend(
        eve,
        &owner_cat,
        &cat_solution(eve, ring, &bad_inner)?,
    )?];
    submit(
        &mut simulator,
        &mut attempts,
        "eve_wrong_tail_reveal_hash",
        bad,
        Signature::default(),
        Expected::Runtime("raise"),
        max_cost,
    )?;
    let bad = vec![coin_spend(eve, &owner_cat, &[0x80])?];
    submit(
        &mut simulator,
        &mut attempts,
        "eve_malformed_solution",
        bad,
        Signature::default(),
        Expected::Runtime("path_into_atom"),
        max_cost,
    )?;
    let sig = signed(&issuance, owner)?;
    submit(
        &mut simulator,
        &mut attempts,
        "zero_delta_tail_issuance",
        issuance,
        sig,
        Expected::Accepted,
        max_cost,
    )?;
    let issued = child(&simulator, eve, &owner_cat, AMOUNT)?;
    let issued_lineage = lineage(eve, &owner_inner)?;
    let transfer_inner = standard_solution(Some((puzzle_hash(&recipient_inner)?, AMOUNT)), None)?;
    let transfer_ring = self_ring(issued, &owner_inner, Some(issued_lineage))?;
    let mut bad_lineage = transfer_ring;
    bad_lineage
        .lineage
        .as_mut()
        .unwrap()
        .parent_parent_coin_info = Bytes32::new([0x92; 32]);
    let bad = vec![coin_spend(
        issued,
        &owner_cat,
        &cat_solution(issued, bad_lineage, &transfer_inner)?,
    )?];
    submit(
        &mut simulator,
        &mut attempts,
        "transfer_wrong_parent_lineage",
        bad,
        Signature::default(),
        Expected::Runtime("raise"),
        max_cost,
    )?;
    // Burning one mojo would be an XCH fee, but CAT's self-ring must still
    // reject the nonzero debt. Avoid MintingCoin masking the ring witness.
    let imbalance_inner =
        standard_solution(Some((puzzle_hash(&recipient_inner)?, AMOUNT - 1)), None)?;
    let bad = vec![coin_spend(
        issued,
        &owner_cat,
        &cat_solution(issued, transfer_ring, &imbalance_inner)?,
    )?];
    let sig = signed(&bad, owner)?;
    submit(
        &mut simulator,
        &mut attempts,
        "transfer_cat_ring_imbalance",
        bad,
        sig,
        Expected::Consensus(ErrorCode::AssertCoinAnnouncementFailed),
        max_cost,
    )?;
    let transfer = vec![coin_spend(
        issued,
        &owner_cat,
        &cat_solution(issued, transfer_ring, &transfer_inner)?,
    )?];
    let sig = signed(&transfer, owner)?;
    submit(
        &mut simulator,
        &mut attempts,
        "owner_transfers_issued_cat",
        transfer,
        sig,
        Expected::Accepted,
        max_cost,
    )?;
    let received = child(&simulator, issued, &recipient_cat, AMOUNT)?;
    let recipient_ring = self_ring(
        received,
        &recipient_inner,
        Some(lineage(issued, &owner_inner)?),
    )?;
    let recipient_solution =
        standard_solution(Some((puzzle_hash(&recipient_inner)?, AMOUNT)), None)?;
    let final_spend = vec![coin_spend(
        received,
        &recipient_cat,
        &cat_solution(received, recipient_ring, &recipient_solution)?,
    )?];
    let wrong = sign_single_requirement_with_wrong_key(&final_spend, owner)?;
    submit(
        &mut simulator,
        &mut attempts,
        "recipient_rejects_previous_owner_signature",
        final_spend.clone(),
        wrong,
        Expected::Consensus(ErrorCode::BadAggregateSignature),
        max_cost,
    )?;
    let sig = signed(&final_spend, recipient)?;
    submit(
        &mut simulator,
        &mut attempts,
        "recipient_spends_with_valid_lineage",
        final_spend,
        sig,
        Expected::Accepted,
        max_cost,
    )?;
    let continuation = child(&simulator, received, &recipient_cat, AMOUNT)?;
    submit(
        &mut simulator,
        &mut attempts,
        "consumed_genesis_input_replay",
        funding,
        funding_signature,
        Expected::Consensus(ErrorCode::DoubleSpend),
        max_cost,
    )?;
    Ok(
        json!({"asset_id":hex::encode(asset_id),"tail_reveal_hex":hex::encode(tail_reveal),
        "owner_inner_reveal_hex":hex::encode(owner_inner),"recipient_inner_reveal_hex":hex::encode(recipient_inner),
        "owner_cat_reveal_hex":hex::encode(owner_cat),"recipient_cat_reveal_hex":hex::encode(recipient_cat),
        "genesis":record_coin(genesis),"eve":record_coin(eve),"issued":record_coin(issued),"received":record_coin(received),
        "continuation":record_coin(continuation),"attempts":attempts}),
    )
}

pub fn run_source_control(
    module: &[u8],
    name: &str,
    wrong_genesis: bool,
    delta: bool,
    accepted: bool,
    owner: &SecretKey,
    max_cost: u64,
) -> Result<Value> {
    ensure!(
        name.len() <= 96 && name.is_ascii(),
        "bounded source-control label"
    );
    admit_module(module, max_cost)?;
    let mut simulator = Simulator::new();
    let inner = standard(owner.public_key())?;
    let genesis = create_coin(&mut simulator, &inner, AMOUNT)?;
    let declared = if wrong_genesis {
        Bytes32::new([0xa7; 32])
    } else {
        genesis.coin_id()
    };
    let tail_reveal = tail(module, declared)?;
    let asset_id = puzzle_hash(&tail_reveal)?;
    let reveal = cat(asset_id, &inner)?;
    let mut attempts = Vec::new();
    let funding = vec![coin_spend(
        genesis,
        &inner,
        &standard_solution(Some((puzzle_hash(&reveal)?, AMOUNT)), None)?,
    )?];
    let sig = signed(&funding, owner)?;
    submit(
        &mut simulator,
        &mut attempts,
        "fund_own_tail_asset_eve",
        funding,
        sig,
        Expected::Accepted,
        max_cost,
    )?;
    let eve = child(&simulator, genesis, &reveal, AMOUNT)?;
    let output = AMOUNT + u64::from(delta);
    let inner_solution =
        standard_solution(Some((puzzle_hash(&inner)?, output)), Some(&tail_reveal))?;
    let mut ring = self_ring(eve, &inner, None)?;
    ring.extra_delta = i64::from(delta);
    let mut spends = vec![coin_spend(
        eve,
        &reveal,
        &cat_solution(eve, ring, &inner_solution)?,
    )?];
    let subsidy = if delta {
        let coin = create_coin(&mut simulator, &inner, 1)?;
        spends.push(coin_spend(coin, &inner, &standard_solution(None, None)?)?);
        Some(coin)
    } else {
        None
    };
    // Source CAT2 subtotal: prev + input_amount - output_sum + extra_delta.
    // With delta=1/output=1001, the CAT ring balances; actual XCH input1001
    // (Eve1000 + standard subsidy1) equals output1001. Only TAIL policy differs.
    let signature = if accepted {
        signed(&spends, owner)?
    } else {
        Signature::default()
    };
    submit(
        &mut simulator,
        &mut attempts,
        name,
        spends,
        signature,
        if accepted {
            Expected::Accepted
        } else {
            Expected::Runtime("raise")
        },
        max_cost,
    )?;
    let child = if accepted {
        Some(record_coin(child(&simulator, eve, &reveal, output)?))
    } else {
        None
    };
    Ok(
        json!({"name":name,"declared_genesis_id":hex::encode(declared),"actual_genesis":record_coin(genesis),
        "module_hex":hex::encode(module),"module_tree_hash":hex::encode(puzzle_hash(module)?),
        "asset_id":hex::encode(asset_id),"tail_reveal_hex":hex::encode(tail_reveal),"cat_reveal_hex":hex::encode(reveal),
        "eve":record_coin(eve),"subsidy":subsidy.map(record_coin),"extra_delta":ring.extra_delta,
        "output_amount":output,"child":child,"attempts":attempts}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_plain_admission_is_outside_policy_execution() {
        assert!(admit_module(&[0x80], 50_000_000).is_ok());
        // A backreference marker is refused structurally, but identical atom
        // payload bytes are ordinary admitted data, not forbidden markers.
        assert!(admit_module(&[0xfe, 0x01], 50_000_000).is_err());
        assert!(admit_module(&[0x81, 0xfe], 50_000_000).is_ok());
        assert!(admit_module(&[0x80, 0x80], 50_000_000).is_err());
        assert!(admit_module(&[0x80], 0).is_err());
        assert!(admit_module(&[0x80], 250_000_001).is_err());
    }

    #[test]
    fn curry_reservation_rejects_deep_or_oversized_inputs_before_sdk_copies() {
        let mut deep = Vec::new();
        for _ in 0..510 {
            deep.extend_from_slice(&[0xff, 0x80]);
        }
        deep.push(0x80);
        assert!(admit_module(&deep, 250_000_000).is_err());
        assert!(admit_module(&vec![0x80; 1024 * 1024], 250_000_000).is_err());
    }
}
