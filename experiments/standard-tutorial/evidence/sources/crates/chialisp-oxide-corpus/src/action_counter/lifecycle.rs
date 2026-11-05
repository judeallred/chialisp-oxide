//! Bounded local singleton lifecycle, with exact typed rejection journals.
use super::wire::*;
use anyhow::{Context, Result, bail, ensure};
use chia_puzzle_types::{EveProof, LineageProof, Proof, standard::StandardArgs};
use chia_sdk_types::puzzles::{ACTION_LAYER_PUZZLE, DEFAULT_FINALIZER_PUZZLE};
use chialisp_oxide_simulator::{
    Bytes32, Coin, CoinSpend, ErrorCode, SecretKey, Signature, Simulator, SimulatorError,
    SpendBundle, coin_spend, create_coin, sign_single_requirement_with_wrong_key, sign_transaction,
};
use serde::Serialize;
use serde_json::{Value, json};

const INTENT: &[u8] = b"chialisp-oxide/counter/reset:v1!";

#[derive(Serialize)]
#[serde(transparent)]
pub(super) struct LifecycleJournal(Value);

pub(super) fn canonical_components() -> Result<Value> {
    let mut rows = Vec::new();
    for (name, module, expected_bytes, expected_hash) in [
        (
            "action_layer",
            ACTION_LAYER_PUZZLE.as_slice(),
            630,
            "afa03f2903f9e4a293523237799074b13ab2361f250df10de5df884b0b09a22b",
        ),
        (
            "default_finalizer",
            DEFAULT_FINALIZER_PUZZLE.as_slice(),
            494,
            "9d274de59aeaa128d1b0a802b52911838420ef9720d605c78942e8dadfc0810b",
        ),
        (
            "singleton_launcher",
            chia_puzzles::SINGLETON_LAUNCHER.as_slice(),
            175,
            "eff07522495060c066f66f32acc2a77e3a3e737aca8baea4d1a64ea4cdc13da9",
        ),
        (
            "singleton_top_layer_v1_1",
            chia_puzzles::SINGLETON_TOP_LAYER_V1_1.as_slice(),
            967,
            "7faa3253bfddd1e0decb0906b2dc6247bbc4cf608f58345d173adb63e8b47c9f",
        ),
    ] {
        admit(module)?;
        let actual = hex::encode(hash(module)?);
        ensure!(
            module.len() == expected_bytes && actual == expected_hash,
            "canonical {name} identity differs"
        );
        rows.push(json!({"name":name,"module_hex":hex::encode(module),"bytes":module.len(),"module_tree_hash":actual}));
    }
    Ok(Value::Array(rows))
}

fn primary(reveal: &[u8], solution: &[u8], max_cost: u64) -> Result<Value> {
    admit(reveal)?;
    admit(solution)?;
    let mut allocator = clvmr::Allocator::new_limited(32 * 1024 * 1024);
    let program = clvmr::serde::node_from_bytes(&mut allocator, reveal)?;
    let environment = clvmr::serde::node_from_bytes(&mut allocator, solution)?;
    match clvmr::run_program(
        &mut allocator,
        &clvmr::ChiaDialect::new(clvmr::MEMPOOL_MODE),
        program,
        environment,
        max_cost,
    ) {
        Ok(result) => {
            let output = clvmr::serde::node_to_bytes_limit(&allocator, result.1, LIMIT)?;
            admit(&output)?;
            Ok(
                json!({"outcome":{"status":"accepted","output_hex":hex::encode(output)},"cost":result.0,"failure":null}),
            )
        }
        Err(error) => {
            let failure = chialisp_oxide_vm::classify_failure(&error);
            ensure!(
                failure.is_program_rejection(),
                "primary resource/infrastructure failure is inconclusive: {error}"
            );
            Ok(json!({"outcome":{"status":"rejected"},"cost":null,"failure":failure}))
        }
    }
}

fn coin(coin: Coin) -> Value {
    json!({"coin_id":hex::encode(coin.coin_id()),"parent_coin_id":hex::encode(coin.parent_coin_info),"puzzle_hash":hex::encode(coin.puzzle_hash),"amount":coin.amount})
}

struct Ledger {
    simulator: Simulator,
    attempts: Vec<Value>,
    max_cost: u64,
}
impl Ledger {
    fn submit(
        &mut self,
        name: &str,
        spends: Vec<CoinSpend>,
        signature: Signature,
        expected: Option<ErrorCode>,
    ) -> Result<()> {
        let executions = spends
            .iter()
            .map(|spend| {
                primary(
                    spend.puzzle_reveal.as_ref(),
                    spend.solution.as_ref(),
                    self.max_cost,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let runtime = expected == Some(ErrorCode::GeneratorRuntimeError);
        ensure!(
            if runtime {
                executions
                    .iter()
                    .any(|e| e["outcome"]["status"] == "rejected")
            } else {
                executions
                    .iter()
                    .all(|e| e["outcome"]["status"] == "accepted")
            },
            "primary acceptance/runtime failure boundary"
        );
        let transaction = json!({"signature_hex":hex::encode(signature.to_bytes()),"spends":spends.iter().map(|spend| json!({"coin":coin(spend.coin),"reveal_hex":hex::encode(spend.puzzle_reveal.as_ref()),"solution_hex":hex::encode(spend.solution.as_ref())})).collect::<Vec<_>>()});
        let outcome = if let Some(code) = expected {
            let before = self.simulator.clone();
            match self
                .simulator
                .new_transaction(SpendBundle::new(spends, signature))
            {
                Err(SimulatorError::Validation(actual)) if actual == code => {}
                other => bail!("expected exact {code:?} for {name}, got {other:?}"),
            }
            ensure!(
                self.simulator == before,
                "complete simulator changed after {name} rejection"
            );
            json!({"status":"rejected","stage":if runtime {"runtime_umbrella"} else {"consensus"},"code":format!("{code:?}"),"complete_state_unchanged":true})
        } else {
            self.simulator
                .new_transaction(SpendBundle::new(spends, signature))?;
            json!({"status":"accepted"})
        };
        self.attempts.push(json!({"name":name,"transaction":transaction,"primary_vm":executions,"outcome":outcome}));
        Ok(())
    }

    fn child(&self, parent: Coin, reveal: &[u8]) -> Result<Coin> {
        let children = self.simulator.children(parent.coin_id());
        ensure!(children.len() == 1, "exact unique singleton child");
        let child = children[0].coin;
        ensure!(
            child.parent_coin_info == parent.coin_id()
                && child.puzzle_hash == hash(reveal)?
                && child.amount == 1,
            "actual child commitment differs"
        );
        ensure!(
            self.simulator
                .coin_state(parent.coin_id())
                .context("parent state")?
                .spent_height
                .is_some(),
            "actual parent was not consumed"
        );
        Ok(child)
    }

    fn expect_output(&self, expected: &[u8]) -> Result<()> {
        let last = self.attempts.last().context("actual ledger observation")?;
        let outputs = last["primary_vm"]
            .as_array()
            .context("primary executions")?;
        ensure!(
            outputs.len() == 1 && outputs[0]["outcome"]["output_hex"] == hex::encode(expected),
            "independent singleton conditions differ"
        );
        Ok(())
    }
}

fn expected_output(
    policy: &Policy,
    state: u64,
    parent: Bytes32,
    owner: &SecretKey,
    reset: bool,
) -> Result<Vec<u8>> {
    let mut conditions = vec![
        list(&[integer(73)?, integer(1)?])?,
        list(&[integer(71)?, atom(parent.as_ref())?])?,
        list(&[
            integer(51)?,
            atom(hash(&policy.full(state)?)?.as_ref())?,
            integer(1)?,
            list(&[atom(policy.launcher.as_ref())?])?,
        ])?,
    ];
    if reset {
        conditions.push(list(&[
            integer(50)?,
            atom(&owner.public_key().to_bytes())?,
            atom(INTENT)?,
        ])?);
    }
    list(&conditions)
}

pub(super) fn run(
    selected_action: &str,
    increment: &[u8],
    reset: &[u8],
    owner: &SecretKey,
    max_cost: u64,
) -> Result<LifecycleJournal> {
    ensure!(
        max_cost > 0 && max_cost <= 250_000_000,
        "bounded primary VM cost"
    );
    let selected = match selected_action {
        "counter-increment" => 0,
        "counter-reset" => 1,
        _ => bail!("unknown selected counter action"),
    };
    admit(increment)?;
    admit(reset)?;
    let mut ledger = Ledger {
        simulator: Simulator::new(),
        attempts: Vec::new(),
        max_cost,
    };
    let wallet = curry(
        &chia_puzzles::P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE,
        &[atom(&owner.public_key().to_bytes())?],
    )?;
    ensure!(
        hash(&wallet)?.to_bytes() == StandardArgs::curry_tree_hash(owner.public_key()).to_bytes(),
        "standard wallet curry identity"
    );
    let genesis = create_coin(&mut ledger.simulator, &wallet, 1)?;
    let launcher = Coin::new(
        genesis.coin_id(),
        Bytes32::new(chia_puzzles::SINGLETON_LAUNCHER_HASH),
        1,
    );
    let policy = Policy::new(
        increment.to_vec(),
        reset.to_vec(),
        &owner.public_key().to_bytes(),
        launcher.coin_id(),
    )?;
    let initial_state = 5;
    let next_state = if selected == 0 { 6 } else { 0 };
    let eve_reveal = policy.full(initial_state)?;
    let launch_solution = list(&[atom(hash(&eve_reveal)?.as_ref())?, integer(1)?, atom(&[])?])?;
    let announcement = sha(&[
        launcher.coin_id().as_ref(),
        hash(&launch_solution)?.as_ref(),
    ]);
    let funding_conditions = list(&[
        list(&[
            integer(51)?,
            atom(launcher.puzzle_hash.as_ref())?,
            integer(1)?,
        ])?,
        list(&[integer(61)?, atom(announcement.as_ref())?])?,
    ])?;
    let funding_solution = list(&[
        atom(&[])?,
        pair(&integer(1)?, &funding_conditions)?,
        atom(&[])?,
    ])?;
    let spends = vec![
        coin_spend(genesis, &wallet, &funding_solution)?,
        coin_spend(
            launcher,
            &chia_puzzles::SINGLETON_LAUNCHER,
            &launch_solution,
        )?,
    ];
    let signature = sign_transaction(&spends, std::slice::from_ref(owner))?;
    ledger.submit("fund_and_launch_singleton", spends, signature, None)?;
    let eve = ledger.child(launcher, &eve_reveal)?;
    let solution = policy.full_solution(
        Proof::Eve(EveProof {
            parent_parent_coin_info: genesis.coin_id(),
            parent_amount: 1,
        }),
        &[selected],
    )?;
    let wrong_sibling_path = [true, true, false, true, false, false, true, true, false];
    let bad_solution = rewrite(&solution, &wrong_sibling_path, &atom(&[0x45; 32])?)?;
    let invalid = coin_spend(eve, &eve_reveal, &bad_solution)?;
    ledger.submit(
        "wrong_membership_sibling",
        vec![invalid],
        Signature::default(),
        Some(ErrorCode::GeneratorRuntimeError),
    )?;
    let spend = coin_spend(eve, &eve_reveal, &solution)?;
    if selected == 1 {
        ledger.submit(
            "missing_reset_signature",
            vec![spend.clone()],
            Signature::default(),
            Some(ErrorCode::BadAggregateSignature),
        )?;
        let wrong_key = SecretKey::from_seed(&[0x15; 32]);
        let invalid_signature =
            sign_single_requirement_with_wrong_key(std::slice::from_ref(&spend), &wrong_key)?;
        ensure!(
            invalid_signature != Signature::default(),
            "actual nonempty wrong signature"
        );
        ledger.submit(
            "wrong_key_reset_signature",
            vec![spend.clone()],
            invalid_signature,
            Some(ErrorCode::BadAggregateSignature),
        )?;
    }
    let signature = sign_transaction(std::slice::from_ref(&spend), std::slice::from_ref(owner))?;
    ledger.submit(
        "selected_action",
        vec![spend.clone()],
        signature.clone(),
        None,
    )?;
    ledger.expect_output(&expected_output(
        &policy,
        next_state,
        launcher.coin_id(),
        owner,
        selected == 1,
    )?)?;
    let selected_child = ledger.child(eve, &policy.full(next_state)?)?;
    ledger.submit(
        "consumed_bundle_replay",
        vec![spend],
        signature,
        Some(ErrorCode::DoubleSpend),
    )?;
    let lineage = Proof::Lineage(LineageProof {
        parent_parent_coin_info: eve.parent_coin_info,
        parent_inner_puzzle_hash: hash(&policy.inner(initial_state)?)?,
        parent_amount: 1,
    });
    let next_solution = policy.full_solution(lineage, &[0])?;
    let continuation = coin_spend(selected_child, &policy.full(next_state)?, &next_solution)?;
    ledger.submit(
        "increment_actual_child",
        vec![continuation],
        Signature::default(),
        None,
    )?;
    ledger.expect_output(&expected_output(
        &policy,
        next_state + 1,
        eve.coin_id(),
        owner,
        false,
    )?)?;
    let continued_child = ledger.child(selected_child, &policy.full(next_state + 1)?)?;
    let accepted = ledger
        .attempts
        .iter()
        .filter(|a| a["outcome"]["status"] == "accepted")
        .count();
    let typed = ledger
        .attempts
        .iter()
        .filter(|a| a["outcome"]["stage"] == "consensus")
        .count();
    let runtime = ledger
        .attempts
        .iter()
        .filter(|a| a["outcome"]["stage"] == "runtime_umbrella")
        .count();
    let primary_runs: usize = ledger
        .attempts
        .iter()
        .map(|a| a["primary_vm"].as_array().unwrap().len())
        .sum();
    let expected_attempts = if selected == 0 { 5 } else { 7 };
    ensure!(
        ledger.attempts.len() == expected_attempts
            && accepted == 3
            && typed == expected_attempts - 4
            && runtime == 1
            && primary_runs == expected_attempts + 1,
        "exact counter lifecycle inventory"
    );
    Ok(LifecycleJournal(
        json!({"selected_action":selected_action,"initial_state":initial_state,"selected_child_state":next_state,"continued_child_state":next_state+1,"owner_public_key":hex::encode(owner.public_key().to_bytes()),"merkle_root":hex::encode(policy.root),"increment_module_hex":hex::encode(increment),"reset_template_hex":hex::encode(reset),"genesis":coin(genesis),"launcher":coin(launcher),"eve":coin(eve),"selected_child":coin(selected_child),"continued_child":coin(continued_child),"attempts":ledger.attempts,"sdk_transaction_attempts":expected_attempts,"accepted":accepted,"typed_rejected":typed,"runtime_umbrellas":runtime,"component_primary_vm_runs":primary_runs}),
    ))
}
