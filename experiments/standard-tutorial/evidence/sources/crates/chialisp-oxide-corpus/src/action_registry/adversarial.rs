//! Causal five-build submissions with separately typed SDK and primary outcomes.
use super::lifecycle::*;
use super::wire::*;
use super::{ACTIONS, RegistryAdversarialCounts, RegistryCausalCounts, RegistrySetupCounts};
use anyhow::{Context, Result, bail, ensure};
use chia_puzzle_types::{LineageProof, Proof, singleton::SingletonSolution};
use chialisp_oxide_simulator::{
    Bytes32, Coin, CoinSpend, ErrorCode, SecretKey, Signature, SimulatorError, SpendBundle,
    coin_spend, create_coin, sign_transaction,
};
use clvm_traits::ToClvm;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct AdversarialJournal(pub(crate) Value);

fn full_with_inner(policy: &Policy, c: Controller, inner: &[u8]) -> Result<CoinSpend> {
    let (parent, parent_state) = c.parent.context("initialized child lineage")?;
    let proof = Proof::Lineage(LineageProof {
        parent_parent_coin_info: parent.parent_coin_info,
        parent_inner_puzzle_hash: hash(&policy.inner(parent_state)?)?,
        parent_amount: parent.amount,
    });
    let mut a = clvmr_sdk::Allocator::new();
    let node = clvmr_sdk::serde::node_from_bytes(&mut a, inner)?;
    let s = SingletonSolution {
        lineage_proof: proof,
        amount: 1,
        inner_solution: node,
    }
    .to_clvm(&mut a)?;
    let raw = clvmr_sdk::serde::node_to_bytes_limit(&a, s, LIMIT)?;
    coin_spend(c.coin, &policy.full(c.state)?, &raw)
}
fn reject(
    ledger: &mut Ledger,
    name: &str,
    spends: Vec<CoinSpend>,
    signature: Signature,
    expected_outputs: &[Option<Vec<u8>>],
    mutation: Value,
) -> Result<()> {
    ensure!(
        spends.len() == expected_outputs.len(),
        "all primary expectations"
    );
    let mut executions = Vec::new();
    for (spend, expected) in spends.iter().zip(expected_outputs) {
        let execution = primary(spend.puzzle_reveal.as_ref(), spend.solution.as_ref())?;
        match expected {
            Some(bytes) => ensure!(
                execution["outcome"]["status"] == "accepted"
                    && execution["outcome"]["output_hex"] == hex::encode(bytes),
                "exact accepted primary for {name}"
            ),
            None => ensure!(
                execution["outcome"]["status"] == "rejected"
                    && execution["failure"]["code"] == "raise",
                "exact primary Raise for {name}: {execution}"
            ),
        }
        executions.push(execution);
    }
    let transaction = json!({"signature_hex":hex::encode(signature.to_bytes()),"spends":spends.iter().map(|s|json!({"coin":coin(s.coin),"reveal_hex":hex::encode(s.puzzle_reveal.as_ref()),"solution_hex":hex::encode(s.solution.as_ref())})).collect::<Vec<_>>()});
    let before = ledger.sim.clone();
    let code = match ledger
        .sim
        .new_transaction(SpendBundle::new(spends, signature))
    {
        Err(SimulatorError::Validation(code)) => code,
        other => bail!("expected validation failure for {name}, got {other:?}"),
    };
    ensure!(ledger.sim == before, "full simulator Eq after {name}");
    ensure!(
        !matches!(code, ErrorCode::CostExceeded | ErrorCode::Unknown),
        "inconclusive SDK resource/infrastructure: {code:?}"
    );
    let expected_code = match name {
        "wrong_membership_sibling"
        | "missing_membership_sibling"
        | "wrong_old_value"
        | "reinitialize"
        | "two_registry_actions_one_spend" => ErrorCode::GeneratorRuntimeError,
        "wrong_creator_proof" | "independently_funded_lookalike_parent" => {
            ErrorCode::AssertMyParentIdFailed
        }
        "different_current_spender_inner_hash" => ErrorCode::MessageNotSentOrReceived,
        _ => bail!("unknown causal negative {name}"),
    };
    ensure!(
        code == expected_code,
        "exact observed typed outcome inventory for {name}"
    );
    let umbrella = code == ErrorCode::GeneratorRuntimeError;
    ensure!(
        umbrella == expected_outputs.iter().any(Option::is_none),
        "runtime umbrella must have independent primary program rejection; typed consensus cases have accepted primaries"
    );
    let stage = if umbrella {
        "consensus_runtime_umbrella"
    } else {
        "consensus"
    };
    ledger.attempts.push(json!({"name":name,"mutation":mutation,"transaction":transaction,"primary_vm":executions,"outcome":{"status":"rejected","stage":stage,"code":format!("{code:?}"),"complete_state_unchanged":true}}));
    Ok(())
}
fn proof_wire(policy: &Policy, action: usize) -> Result<Vec<u8>> {
    let proof = &policy.proofs[action];
    let mut fields = vec![integer(u64::from(proof.path))?];
    for sibling in &proof.proof {
        fields.push(atom(sibling.as_ref())?);
    }
    list(&fields)
}
pub fn run(
    artifacts: &[(String, String, Vec<u8>)],
    mode: &str,
    owner: &SecretKey,
) -> Result<(
    AdversarialJournal,
    RegistryAdversarialCounts,
    RegistrySetupCounts,
    RegistryCausalCounts,
)> {
    let mut templates = Vec::new();
    for action in ACTIONS {
        let selected = artifacts
            .iter()
            .filter(|r| r.0 == action && r.1 == mode)
            .collect::<Vec<_>>();
        ensure!(selected.len() == 1, "unique adversarial artifact");
        templates.push(selected[0].2.as_slice());
    }
    let Start {
        mut ledger,
        policy,
        genesis,
        launcher,
        eve,
        c1,
        a0,
        b0,
    } = start([templates[0], templates[1], templates[2]], owner)?;
    let setup = std::mem::take(&mut ledger.attempts);
    let old = [0; 32];
    let new = [0x11; 32];
    let next = c1.state.set(0, &new);
    let cs = controller_spend(&policy, c1, 1, &old, &new, genesis)?;
    let ss = slot_spend(&policy, a0, c1)?;
    let out = controller_output(&policy, c1, next, 1, &old, &new, owner)?;
    let slot_out = slot_output(a0, c1)?;
    let valid_signature = sign_transaction(&[cs.clone(), ss.clone()], std::slice::from_ref(owner))?;
    for missing in [false, true] {
        let mut altered = policy.clone();
        let before = altered.proofs[1]
            .proof
            .iter()
            .map(hex::encode)
            .collect::<Vec<_>>();
        if missing {
            altered.proofs[1].proof.pop();
        } else {
            let mut bytes: [u8; 32] = altered.proofs[1].proof[0].into();
            bytes[0] ^= 1;
            altered.proofs[1].proof[0] = Bytes32::new(bytes);
        }
        let bad = controller_spend(&altered, c1, 1, &old, &new, genesis)?;
        reject(
            &mut ledger,
            if missing {
                "missing_membership_sibling"
            } else {
                "wrong_membership_sibling"
            },
            vec![bad, ss.clone()],
            valid_signature.clone(),
            &[None, Some(slot_out.clone())],
            json!({"kind":if missing {"remove_last_membership_sibling"} else {"xor_first_sibling_byte0"},"action":1,"path":1,"before_siblings_hex":before,"after_siblings_hex":altered.proofs[1].proof.iter().map(hex::encode).collect::<Vec<_>>(),"signature_source":"valid_same_coin_alpha_transition"}),
        )?;
    }
    let mut fake_creator = a0.creator;
    let mut bytes: [u8; 32] = fake_creator.parent_coin_info.into();
    bytes[0] ^= 1;
    fake_creator.parent_coin_info = Bytes32::new(bytes);
    let bad_slot = Slot {
        creator: fake_creator,
        ..a0
    };
    let bad_spend = coin_spend(
        a0.coin,
        &policy.slot(a0.entry)?,
        &policy.slot_solution(fake_creator, a0.creator_state, c1.state)?,
    )?;
    reject(
        &mut ledger,
        "wrong_creator_proof",
        vec![cs.clone(), bad_spend],
        valid_signature.clone(),
        &[Some(out.clone()), Some(slot_output(bad_slot, c1)?)],
        json!({"kind":"xor_creator_parent_parent_byte0","original_creator":coin(a0.creator),"mutated_creator":coin(fake_creator),"unchanged_current_controller":coin(c1.coin)}),
    )?;
    let altered_state = c1.state.set(0, &[0x44; 32]);
    let wrong_full = hash(&policy.full(altered_state)?)?;
    ensure!(
        wrong_full != c1.coin.puzzle_hash,
        "genuinely different sender full puzzle hash"
    );
    let wrong_current = Controller {
        state: altered_state,
        coin: Coin::new(c1.coin.parent_coin_info, wrong_full, 1),
        ..c1
    };
    let bad_spend = coin_spend(
        a0.coin,
        &policy.slot(a0.entry)?,
        &policy.slot_solution(a0.creator, a0.creator_state, altered_state)?,
    )?;
    reject(
        &mut ledger,
        "different_current_spender_inner_hash",
        vec![cs.clone(), bad_spend],
        valid_signature.clone(),
        &[Some(out.clone()), Some(slot_output(a0, wrong_current)?)],
        json!({"kind":"replace_spender_inner_hash","actual_state":state(c1.state),"mutated_state":state(altered_state),"actual_full_hash":hex::encode(c1.coin.puzzle_hash),"mutated_full_hash":hex::encode(wrong_full)}),
    )?;
    let wrong_old = [0x33; 32];
    let bad = controller_spend(&policy, c1, 1, &wrong_old, &new, genesis)?;
    reject(
        &mut ledger,
        "wrong_old_value",
        vec![bad, ss.clone()],
        valid_signature.clone(),
        &[None, Some(slot_out.clone())],
        json!({"kind":"replace_old_value","original_hex":hex::encode(old),"mutated_hex":hex::encode(wrong_old),"new_hex":hex::encode(new),"signature_source":"valid_same_coin_alpha_transition; rejected program emits no signature conditions"}),
    )?;
    let bad = controller_spend(&policy, c1, 0, &old, &old, genesis)?;
    reject(
        &mut ledger,
        "reinitialize",
        vec![bad],
        Signature::default(),
        &[None],
        json!({"kind":"initialize_initialized_state","state":state(c1.state)}),
    )?;
    let proof = proof_wire(&policy, 1)?;
    let two_inner = list(&[
        list(&[policy.actions[1].clone()])?,
        list(&[
            pair(&integer(2)?, &proof)?,
            pair(&integer(2)?, &atom(&[])?)?,
        ])?,
        list(&[
            list(&[atom(&old)?, atom(&new)?])?,
            list(&[atom(&new)?, atom(&[0x22; 32])?])?,
        ])?,
    ])?;
    let bad = full_with_inner(&policy, c1, &two_inner)?;
    reject(
        &mut ledger,
        "two_registry_actions_one_spend",
        vec![bad, ss.clone()],
        valid_signature.clone(),
        &[None, Some(slot_out.clone())],
        json!({"kind":"repeat_alpha_action_with_verified_selector_reuse","first_old_hex":hex::encode(old),"first_new_hex":hex::encode(new),"second_old_hex":hex::encode(new),"second_new_hex":hex::encode([0x22;32]),"proof_reuse_selector":2}),
    )?;
    let first_env = list(&[
        pair(&atom(&[])?, &c1.state.encode()?)?,
        atom(&old)?,
        atom(&new)?,
    ])?;
    let blocked_env = list(&[
        pair(&atom(&[1])?, &next.encode()?)?,
        atom(&new)?,
        atom(&[0x22; 32])?,
    ])?;
    let admitted_env = list(&[
        pair(&atom(&[])?, &next.encode()?)?,
        atom(&new)?,
        atom(&[0x22; 32])?,
    ])?;
    let mut causal = Vec::new();
    for (label, env, accepted) in [
        ("first_action_nil", first_env, true),
        ("second_action_used_ephemeral", blocked_env, false),
        ("second_action_nil_only_change", admitted_env, true),
    ] {
        let execution = primary(&policy.actions[1], &env)?;
        ensure!(
            (execution["outcome"]["status"] == "accepted") == accepted,
            "exact one-action causal primary"
        );
        causal.push(json!({"name":label,"reveal_hex":hex::encode(&policy.actions[1]),"environment_hex":hex::encode(env),"execution":execution}));
    }
    ledger.unspent(c1.coin)?;
    ledger.unspent(a0.coin)?;
    ledger.unspent(b0.coin)?;
    // Actual independently funded lookalike, not a synthetic slot injected by create_coin.
    let wallet = curry(
        &chia_puzzles::P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE,
        &[atom(&owner.public_key().to_bytes())?],
    )?;
    let source = create_coin(&mut ledger.sim, &wallet, 0)?;
    let fund_conditions = list(&[list(&[
        integer(51)?,
        atom(a0.coin.puzzle_hash.as_ref())?,
        integer(0)?,
    ])?])?;
    let delegated = pair(&integer(1)?, &fund_conditions)?;
    let funding_solution = list(&[atom(&[])?, delegated.clone(), atom(&[])?])?;
    let funding = coin_spend(source, &wallet, &funding_solution)?;
    let sig = sign_transaction(std::slice::from_ref(&funding), std::slice::from_ref(owner))?;
    let output = pair(
        &list(&[
            integer(50)?,
            atom(&owner.public_key().to_bytes())?,
            atom(hash(&delegated)?.as_ref())?,
        ])?,
        &fund_conditions,
    )?;
    ledger.submit(
        "fund_independent_lookalike_slot",
        vec![funding],
        sig,
        vec![output],
        None,
    )?;
    let lookalike = ledger.children(source, &[(policy.slot(a0.entry)?, 0)])?[0];
    ensure!(
        lookalike.puzzle_hash == a0.coin.puzzle_hash
            && lookalike.amount == a0.coin.amount
            && lookalike.parent_coin_info != a0.coin.parent_coin_info,
        "same reveal/amount, independent actual funding parent"
    );
    let fake_slot = Slot {
        coin: lookalike,
        ..a0
    };
    let fake_spend = slot_spend(&policy, fake_slot, c1)?;
    reject(
        &mut ledger,
        "independently_funded_lookalike_parent",
        vec![cs.clone(), fake_spend],
        valid_signature.clone(),
        &[Some(out.clone()), Some(slot_out.clone())],
        json!({"kind":"replace_only_slot_coin_with_independently_funded_lookalike","funding_source":coin(source),"original_slot":coin(a0.coin),"lookalike_slot":coin(lookalike),"creator_proof_source":"unchanged canonical registry creator proof; its emitted parent assertion must fail for unrelated funding parent","same_controller_spend_and_signature":true}),
    )?;
    ledger.unspent(lookalike)?;
    ledger.unspent(c1.coin)?;
    ledger.unspent(a0.coin)?;
    let (_c2, _a1) = accepted_update(
        &mut ledger,
        &policy,
        c1,
        a0,
        Update {
            action: 1,
            old: &old,
            new: &new,
            name: "valid_alpha_after_all_rejections",
        },
        owner,
        genesis,
    )?;
    ledger.unspent(lookalike)?;
    let typed = ledger
        .attempts
        .iter()
        .filter(|a| a["outcome"]["stage"] == "consensus")
        .count();
    let runtime = ledger
        .attempts
        .iter()
        .filter(|a| a["outcome"]["stage"] == "consensus_runtime_umbrella")
        .count();
    let accepted = ledger
        .attempts
        .iter()
        .filter(|a| a["outcome"]["status"] == "accepted")
        .count();
    let primary_count: usize = ledger
        .attempts
        .iter()
        .map(|a| a["primary_vm"].as_array().unwrap().len())
        .sum();
    ensure!(
        ledger.attempts.len() == 10
            && typed == 3
            && runtime == 5
            && accepted == 2
            && primary_count == 18,
        "adversarial inventory"
    );
    let counts = RegistryAdversarialCounts {
        scenario_runs: 1,
        sdk_transaction_attempts: ledger.attempts.len(),
        accepted,
        typed_consensus_rejections: typed,
        runtime_umbrellas: runtime,
        component_primary_observations: primary_count,
    };
    let setup_counts = RegistrySetupCounts {
        sdk_transaction_attempts: setup.len(),
        accepted: setup
            .iter()
            .filter(|a| a["outcome"]["status"] == "accepted")
            .count(),
        component_primary_observations: setup
            .iter()
            .map(|a| a["primary_vm"].as_array().unwrap().len())
            .sum(),
    };
    let causal_counts = RegistryCausalCounts {
        primary_observations: causal.len(),
        accepted: causal
            .iter()
            .filter(|a| a["execution"]["outcome"]["status"] == "accepted")
            .count(),
        typed_program_rejections: causal
            .iter()
            .filter(|a| a["execution"]["outcome"]["status"] == "rejected")
            .count(),
    };
    Ok((
        AdversarialJournal(
            json!({"mode":mode,"owner_hex":hex::encode(owner.public_key().to_bytes()),"genesis":coin(genesis),"launcher":coin(launcher),"eve":{"coin":coin(eve.coin),"state":state(eve.state)},"controller":{"coin":coin(c1.coin),"state":state(c1.state)},"alpha0":slot_record(a0),"beta0":slot_record(b0),"setup_attempts":setup,"setup_sdk_submissions":2,"setup_primary_observations":3,"attempts":ledger.attempts,"sdk_transaction_attempts":10,"accepted":accepted,"typed_consensus_rejections":typed,"runtime_umbrellas":runtime,"component_primary_vm_runs":primary_count,"causal_primary":causal,"causal_primary_observations":3,"scope":if mode.starts_with("rust-") {"Separate actual retained Rust artifact causal adversarial SDK lane. Runtime umbrellas independently resolved as primary program rejection, not exact semantic consensus/resource rejection. No new frontend execution by this producer, BLS proof or live claim."} else {"Separate reference-only causal adversarial lane. Runtime umbrellas independently resolved as primary program rejection, not exact semantic consensus or resource rejection. No frontend, BLS proof or live claim."}}),
        ),
        counts,
        setup_counts,
        causal_counts,
    ))
}
