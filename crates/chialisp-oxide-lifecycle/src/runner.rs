//! Shared trusted simulator runner for the historical retained fixture and a
//! private recipe produced by executed fresh checks. Both use the same closed
//! commands, exact SDK validation and durable journal; neither submits network
//! transactions. A saved report cannot construct a validated recipe.
use crate::*;
use anyhow::{Context, Result, ensure};
use chia_consensus::{
    flags::MEMPOOL_MODE,
    spendbundle_validation::{get_flags_for_height_and_constants, validate_clvm_and_signature},
};
use chia_protocol::{Coin, CoinSpend, Program, SpendBundle};
use chia_sdk_types::TESTNET11_CONSTANTS;
use chia_traits::Streamable;
use chialisp_oxide_simulator::{ConsensusRejection, Simulator, require_consensus_rejection};
use serde_json::{Value, json};

const MAX_COST: u64 = 10_000_000;
const REVISION: &str = "18d2f3bebc16c0ed460203d6751502719f1a5d91";
const TREE_HASH: &str = "9a630f1e6d93705847614fc4b23b5f5cbf330edf802280684a050a572f47b6b9";
const BLOB_HASH: &str = "4571a31579378b03c1aeef5db4a394325474c0e1b185b187732d8d24bb87bac1";
const SOURCE_HASH: &str = "e3d32e261b8403c9bf971f7d6c621552ff9ddf0964413c0e01f838f28540a84f";

#[derive(Clone, Copy)]
pub enum Command {
    Advance,
    Refresh,
    /// Restore the actual seeded SDK snapshot from before submission. This
    /// models a local simulator reorg; it is not an observed network reorg.
    ReorgUnspent,
}

pub struct SimulatorRunner {
    journal: Journal,
    simulator: Simulator,
    unspent: Option<Simulator>,
    checks: CheckRecord,
    recipe: ValidatedRecipe,
}

/// A process-local trusted recipe. No deserializer or public field permits a
/// saved report to manufacture execution credit. Fresh construction executes
/// the complete adapter before returning this value.
#[derive(Clone)]
pub struct ValidatedRecipe {
    plan: Plan,
    artifact: Artifact,
    checks: CheckRecord,
    scope: Value,
}

impl ValidatedRecipe {
    fn retained() -> Result<Self> {
        let (plan, artifact) = recipe()?;
        let checks = preflight(&plan, &artifact)?;
        Ok(Self {
            plan,
            artifact,
            checks,
            scope: json!({
                "recipe":"retained-fixed-payment-v1", "retained_commit":REVISION,
                "artifact_origin":"retained_optimized_rust_artifact",
                "fresh_execution":"unavailable", "mode":"rust_on"
            }),
        })
    }

    pub(crate) fn fresh(
        plan: Plan,
        artifact: Artifact,
        native_receipt: Vec<u8>,
        scope: Value,
    ) -> Result<Self> {
        ensure!(
            !native_receipt.is_empty() && native_receipt.len() <= 64 * 1024,
            "fresh source-check receipt byte bound"
        );
        let checks = sdk_preflight(&plan, &artifact, native_receipt)?;
        Ok(Self {
            plan,
            artifact,
            checks,
            scope,
        })
    }

    pub fn start(&self) -> Result<SimulatorRunner> {
        self.require_current()?;
        Ok(SimulatorRunner {
            journal: Journal::new(self.plan.clone())?,
            simulator: Simulator::new(),
            unspent: None,
            checks: self.checks.clone(),
            recipe: self.clone(),
        })
    }

    fn require_current(&self) -> Result<()> {
        if self.scope["fresh_execution"] == "executed_this_process" {
            crate::fresh::require_native_inputs()?;
        }
        Ok(())
    }

    pub fn resume(&self, retained: Journal) -> Result<SimulatorRunner> {
        let mut runner = self.start()?;
        runner.replay(retained)?;
        Ok(runner)
    }
}

fn recipe() -> Result<(Plan, Artifact)> {
    let artifact = Artifact {
        reveal: hex::decode(include_str!("fixed-payment.clvm.hex").trim())?,
        solution: vec![0x80],
    };
    ensure!(
        hex::encode(digest(&artifact.reveal)) == BLOB_HASH
            && hex::encode(artifact.puzzle_hash()?) == TREE_HASH,
        "retained artifact identity changed"
    );
    ensure!(
        hex::encode(digest(include_bytes!(
            "../../../puzzles/fixed-payment/puzzle.rs"
        ))) == SOURCE_HASH,
        "pinned native source changed; this recipe needs explicit review"
    );
    let plan = Plan {
        network: Network {
            testnet: "testnet11".into(),
            genesis_challenge: TESTNET11_CONSTANTS.genesis_challenge.to_bytes(),
        },
        amount: 1000,
        outputs: vec![Output {
            puzzle_hash: [0x42; 32],
            amount: 1000,
        }],
    };
    Ok((plan, artifact))
}

fn bundle(coin: Coin, artifact: &Artifact) -> SpendBundle {
    SpendBundle::new(
        vec![CoinSpend::new(
            coin,
            Program::from(artifact.reveal.as_slice()),
            Program::from(artifact.solution.as_slice()),
        )],
        Default::default(),
    )
}

fn validate(bundle: &SpendBundle, height: u32, plan: &Plan) -> Result<Value> {
    let flags = get_flags_for_height_and_constants(height, &TESTNET11_CONSTANTS) | MEMPOOL_MODE;
    let (conditions, _) =
        validate_clvm_and_signature(bundle, MAX_COST, &TESTNET11_CONSTANTS, flags).map_err(
            |error| anyhow::anyhow!("SDK CLVM/signature validation failed: {:?}", error.1),
        )?;
    ensure!(
        conditions.spends.len() == 1 && bundle.coin_spends.len() == 1,
        "fixed recipe requires one input"
    );
    let spend = &conditions.spends[0];
    ensure!(
        spend.coin_id == bundle.coin_spends[0].coin.coin_id(),
        "SDK validated a different input"
    );
    let mut observed = spend
        .create_coin
        .iter()
        .map(|(hash, amount, _)| ((*hash).to_bytes(), *amount))
        .collect::<Vec<_>>();
    let mut expected = plan
        .outputs
        .iter()
        .map(|output| (output.puzzle_hash, output.amount))
        .collect::<Vec<_>>();
    observed.sort();
    expected.sort();
    ensure!(
        observed == expected
            && conditions.removal_amount == 1000
            && conditions.addition_amount == 1000,
        "SDK complete outputs/amounts differ from recipe"
    );
    Ok(
        json!({"status":"executed_sdk_clvm_and_aggregate_validation", "consensus":"chia-consensus 0.36.1", "network":"TESTNET11", "max_cost":MAX_COST, "flags":flags,
        "cost":conditions.cost, "coin_id":hex::encode(spend.coin_id), "outputs":plan.outputs.iter().map(|output| json!({"puzzle_hash":hex::encode(output.puzzle_hash),"amount":output.amount})).collect::<Vec<_>>()}),
    )
}

fn preflight(plan: &Plan, artifact: &Artifact) -> Result<CheckRecord> {
    // Call the actual Cargo puzzle library, never a copied policy body.
    let (amount, output) = chialisp_oxide_puzzles::fixed_payment::puzzle();
    ensure!(
        amount.0 == 73
            && amount.1.0 == 1000
            && output.0 == 51
            && output.1.0 == [0x42; 32]
            && output.2.0 == 1000,
        "current native source output differs from the pinned recipe"
    );
    sdk_preflight(
        plan,
        artifact,
        serde_json::to_vec(
            &json!({"status":"executed_current_cargo_native_source", "source_sha256":SOURCE_HASH,
        "outputs":[{"opcode":73,"amount":1000},{"opcode":51,"puzzle_hash":"42".repeat(32),"amount":1000}]}),
        )?,
    )
}

fn sdk_preflight(plan: &Plan, artifact: &Artifact, native_receipt: Vec<u8>) -> Result<CheckRecord> {
    let coin = CoinRecord {
        parent: [0x19; 32],
        puzzle_hash: artifact.puzzle_hash()?,
        amount: 1000,
    };
    let mut vm = validate(&bundle(coin.coin(), artifact), 0, plan)?;
    // A genuine invalid bundle reaches SDK consensus, not signer preflight.
    let mut simulator = Simulator::new();
    let underfunded = simulator.new_coin(artifact.puzzle_hash()?.into(), 999);
    let before = simulator.clone();
    require_consensus_rejection(
        simulator.new_transaction(bundle(underfunded, artifact)),
        ConsensusRejection::AssertMyAmountFailed,
    )?;
    ensure!(
        simulator == before,
        "underfunded SDK rejection changed complete simulator state"
    );
    vm["underfunded_submission"] =
        json!({"code":"AssertMyAmountFailed","full_state_unchanged":true});
    Ok(CheckRecord {
        artifact_digest: artifact_digest(artifact)?,
        observed_outputs: plan.outputs.clone(),
        native_receipt,
        clvm_receipt: serde_json::to_vec(&vm)?,
    })
}

impl SimulatorRunner {
    pub fn new() -> Result<Self> {
        ValidatedRecipe::retained()?.start()
    }

    /// Retained historical fixture only. Fresh journals require a freshly
    /// executed matching ValidatedRecipe for this process.
    pub fn resume(retained: Journal) -> Result<Self> {
        ValidatedRecipe::retained()?.resume(retained)
    }

    pub fn validated_recipe(&self) -> ValidatedRecipe {
        self.recipe.clone()
    }

    fn replay(&mut self, retained: Journal) -> Result<()> {
        ensure!(
            self.journal.plan() == retained.plan(),
            "unrelated simulator recipe plan"
        );
        for expected in retained.entries() {
            let command = match &expected.event {
                Event::Observed { observation }
                    if matches!(observation.input, InputState::Unspent { .. })
                        && self
                            .journal
                            .signing_request()
                            .ok()
                            .and_then(|request| {
                                self.simulator.coin_state(request.input.id().into())
                            })
                            .is_some_and(|state| state.spent_height.is_some()) =>
                {
                    Command::ReorgUnspent
                }
                Event::Observed { .. } => Command::Refresh,
                _ => Command::Advance,
            };
            self.execute(command)?;
            ensure!(
                self.journal.entries().last() == Some(expected),
                "retained event does not match actual deterministic SDK/native replay"
            );
        }
        Ok(())
    }

    pub fn journal(&self) -> &Journal {
        &self.journal
    }
    pub fn into_journal(self) -> Journal {
        self.journal
    }

    fn observation(&self, coin: &CoinRecord) -> Result<Observation> {
        let input = match self.simulator.coin_state(coin.id().into()) {
            None => InputState::Missing,
            Some(state) => {
                let created_height = state
                    .created_height
                    .context("SDK input creation height missing")?;
                match state.spent_height {
                    Some(spent_height) => InputState::Spent {
                        created_height,
                        spent_height,
                    },
                    None => InputState::Unspent { created_height },
                }
            }
        };
        let mut children = self
            .simulator
            .children(coin.id().into())
            .into_iter()
            .map(|state| {
                Ok(ChildObservation {
                    coin: CoinRecord {
                        parent: state.coin.parent_coin_info.to_bytes(),
                        puzzle_hash: state.coin.puzzle_hash.to_bytes(),
                        amount: state.coin.amount,
                    },
                    created_height: state
                        .created_height
                        .context("SDK child creation height missing")?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        children.sort_by(|a, b| a.coin.cmp(&b.coin));
        Ok(Observation {
            network: self.journal.plan().network.clone(),
            tip: Tip {
                height: self.simulator.height(),
                header_hash: self.simulator.header_hash().to_bytes(),
            },
            input_id: coin.id(),
            input,
            children,
        })
    }

    pub fn execute(&mut self, command: Command) -> Result<()> {
        self.recipe.require_current()?;
        // Local SDK state is cloned alongside the journal transition. Failed
        // commands leave both unchanged; there is no network side effect.
        let prior = self.simulator.clone();
        let checkpoint = self.unspent.clone();
        let result = self
            .event(command)
            .and_then(|event| self.journal.append(self.journal.revision(), event));
        if result.is_err() {
            self.simulator = prior;
            self.unspent = checkpoint;
        }
        result
    }

    fn event(&mut self, command: Command) -> Result<Event> {
        match command {
            Command::Refresh => {
                let request = self.journal.signing_request()?;
                return Ok(Event::Observed {
                    observation: self.observation(&request.input)?,
                });
            }
            Command::ReorgUnspent => {
                ensure!(
                    matches!(
                        self.journal.stage(),
                        Stage::Confirmed | Stage::AwaitingConfirmation
                    ),
                    "simulator reorg requires submitted spend"
                );
                let request = self.journal.signing_request()?;
                self.simulator = self
                    .unspent
                    .clone()
                    .context("unspent SDK checkpoint missing")?;
                return Ok(Event::Observed {
                    observation: self.observation(&request.input)?,
                });
            }
            Command::Advance => (),
        }
        Ok(match self.journal.stage() {
            Stage::AwaitingBuild => Event::Built {
                artifact: self.recipe.artifact.clone(),
            },
            Stage::AwaitingCheck => Event::Checked {
                record: self.checks.clone(),
            },
            Stage::AwaitingFunding => {
                let artifact = &self.recipe.artifact;
                let coin = self
                    .simulator
                    .new_coin(artifact.puzzle_hash()?.into(), 1000);
                let coin = CoinRecord {
                    parent: coin.parent_coin_info.to_bytes(),
                    puzzle_hash: coin.puzzle_hash.to_bytes(),
                    amount: coin.amount,
                };
                self.unspent = Some(self.simulator.clone());
                Event::Funded {
                    observation: self.observation(&coin)?,
                    coin,
                }
            }
            Stage::AwaitingSignature => {
                let request = self.journal.signing_request()?;
                // This permissionless recipe needs no signing key. Actual
                // aggregate validation still runs before submission.
                Event::SignatureReturned {
                    receipt: SignReceipt {
                        request_id: request.id()?,
                        signed_bundle: request.unsigned_bundle,
                    },
                }
            }
            Stage::AwaitingSignatureValidation => {
                let request = self.journal.signing_request()?;
                let validated = validate(
                    &SpendBundle::from_bytes(&request.unsigned_bundle)?,
                    self.simulator.height(),
                    self.journal.plan(),
                )?;
                Event::SignatureValidated {
                    validation: SignatureValidation {
                        request_id: request.id()?,
                        signed_bundle_digest: digest(&request.unsigned_bundle),
                        receipt: serde_json::to_vec(&validated)?,
                    },
                }
            }
            Stage::ReadyToSubmit => {
                let request = self.journal.signing_request()?;
                let bytes = self.journal.submission_bytes()?;
                let pin = digest(bytes);
                self.simulator
                    .new_transaction(SpendBundle::from_bytes(bytes)?)?;
                Event::Submitted {
                    request_id: request.id()?,
                    signed_bundle_digest: pin,
                    public_receipt: serde_json::to_vec(
                        &json!({"status":"accepted_sdk_simulator","coin_id":hex::encode(request.input.id())}),
                    )?,
                }
            }
            Stage::AwaitingConfirmation => {
                let request = self.journal.signing_request()?;
                Event::Observed {
                    observation: self.observation(&request.input)?,
                }
            }
            Stage::Confirmed | Stage::Conflict => {
                anyhow::bail!("no automatic advancement from current stage")
            }
        })
    }

    pub fn view(&self) -> Result<Value> {
        let checks: Value = serde_json::from_slice(&self.checks.clvm_receipt)?;
        let children = self.journal.last_observation().map(|observation| observation.children.iter().map(|child|
            json!({"coin":child.coin,"coin_id":hex::encode(child.coin.id()),"created_height":child.created_height})
        ).collect::<Vec<_>>());
        let input_coin =
            self.journal.signing_request().ok().map(
                |request| json!({"coin":request.input,"coin_id":hex::encode(request.input.id())}),
            );
        Ok(
            json!({"status":"wip_prototype_not_ready_for_use", "backend":"seeded_sdk_simulator_only", "network":"TESTNET11 simulator rules",
            "recipe":self.recipe.scope["recipe"], "retained_commit":self.recipe.scope["retained_commit"], "module_tree_hash":self.recipe.scope.get("module_tree_hash").cloned().unwrap_or(json!(TREE_HASH)), "instance_tree_hash":hex::encode(self.recipe.artifact.puzzle_hash()?),
            "artifact_origin":self.recipe.scope["artifact_origin"], "recipe_scope":self.recipe.scope, "destination_hex":hex::encode(self.journal.plan().outputs[0].puzzle_hash), "stage":self.journal.stage(), "revision":self.journal.revision(), "input_coin":input_coin,
            "checks":{"current_native_source":"executed", "retained_clvm_and_aggregate":"executed", "fresh_cargo_build":self.recipe.scope["fresh_execution"], "fresh_subset_extraction":self.recipe.scope["fresh_execution"], "independent_reference":self.recipe.scope["fresh_execution"], "live_network":"unavailable"},
            "metrics":{
                "reveal_bytes":self.recipe.artifact.reveal.len(),
                "solution_bytes":self.recipe.artifact.solution.len(),
                "plan_fee":0,
                "preflight_sdk_validation":{
                    "api":"chia_consensus::spendbundle_validation::validate_clvm_and_signature",
                    "consensus":"chia-consensus 0.36.1","sdk_vm":"clvmr 0.16.2",
                    "network":"TESTNET11","height":0,"input_coin_id":checks["coin_id"],
                    "cost":checks["cost"],"flags":checks["flags"],"max_cost":checks["max_cost"],
                    "scope":"Executed deterministic preflight input parent19x32; conditions.cost includes SDK charged generator bytes, VM and conditions. Not primary VM cost, current funded-input/network measurement, full-block cost or fee quote."
                }
            },
            "funding_request":{
                "protocol":"chialisp-oxide-lifecycle-public-funding-request/v1",
                "network":self.journal.plan().network, "selected_mode":self.recipe.scope["mode"],
                "source_check_receipt_sha256":hex::encode(digest(&self.checks.native_receipt)),
                "reveal_tree_hash":hex::encode(self.recipe.artifact.puzzle_hash()?),
                "reveal_blob_sha256":hex::encode(digest(&self.recipe.artifact.reveal)),
                "solution_blob_sha256":hex::encode(digest(&self.recipe.artifact.solution)),
                "recipient_puzzle_hash":hex::encode(self.journal.plan().outputs[0].puzzle_hash),
                "input_amount":1000,"complete_outputs":self.journal.plan().outputs,"fee":0,
                "status":"public_plan_only_no_live_funding_observed",
                "scope":"simulator workflow; external wallet signs funding separately; permissionless spend has no owner signature condition; public recipient is not ownership evidence",
                "live_funding_adapter":"unavailable"
            },
            "typed_controls":[checks.get("underfunded_submission").context("underfunded SDK control missing")?], "children":children}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn complete(runner: &mut SimulatorRunner) {
        while runner.journal.stage() != Stage::Confirmed {
            runner.execute(Command::Advance).unwrap();
        }
    }

    #[test]
    fn trusted_runner_executes_and_resumes_at_every_stage() {
        let mut runner = SimulatorRunner::new().unwrap();
        while runner.journal.stage() != Stage::Confirmed {
            runner.execute(Command::Advance).unwrap();
            let bytes = runner.journal.to_json().unwrap();
            runner = SimulatorRunner::resume(Journal::from_json(&bytes).unwrap()).unwrap();
            assert_eq!(runner.journal.to_json().unwrap(), bytes);
        }
        let children = &runner.journal.last_observation().unwrap().children;
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].coin.amount, 1000);
        assert_eq!(children[0].coin.puzzle_hash, [0x42; 32]);
    }

    #[test]
    fn actual_sdk_duplicate_rejection_preserves_full_state_and_journal() {
        let mut runner = SimulatorRunner::new().unwrap();
        complete(&mut runner);
        let before = runner.simulator.clone();
        let journal = runner.journal.to_json().unwrap();
        let request = runner.journal.signing_request().unwrap();
        require_consensus_rejection(
            runner
                .simulator
                .new_transaction(SpendBundle::from_bytes(&request.unsigned_bundle).unwrap()),
            ConsensusRejection::DoubleSpend,
        )
        .unwrap();
        assert_eq!(runner.simulator, before);
        assert_eq!(runner.journal.to_json().unwrap(), journal);
    }

    #[test]
    fn actual_sdk_checkpoint_reorg_can_resume_and_spend_again() {
        let mut runner = SimulatorRunner::new().unwrap();
        complete(&mut runner);
        runner.execute(Command::ReorgUnspent).unwrap();
        assert_eq!(runner.journal.stage(), Stage::ReadyToSubmit);
        runner = SimulatorRunner::resume(
            Journal::from_json(&runner.journal.to_json().unwrap()).unwrap(),
        )
        .unwrap();
        complete(&mut runner);
        assert_eq!(runner.journal.last_observation().unwrap().children.len(), 1);
    }

    #[test]
    fn forged_external_check_receipt_does_not_survive_trusted_replay() {
        let mut runner = SimulatorRunner::new().unwrap();
        runner.execute(Command::Advance).unwrap();
        runner.execute(Command::Advance).unwrap();
        let mut value: Value = serde_json::from_slice(&runner.journal.to_json().unwrap()).unwrap();
        value["entries"][1]["event"]["record"]["native_receipt"] = json!([1]);
        let forged = Journal::from_json(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(SimulatorRunner::resume(forged).is_err());
    }
}

#[cfg(test)]
mod selected_recipe_tests {
    use super::*;
    use crate::fresh::{PaymentMode, reference_recipe_control};

    #[test]
    fn selected_recipient_recipe_resumes_spends_and_reorgs_without_wallet() {
        let recipe = reference_recipe_control([0x23; 32], PaymentMode::On).unwrap();
        let mut runner = recipe.start().unwrap();
        while runner.journal().stage() != Stage::Confirmed {
            runner.execute(Command::Advance).unwrap();
            runner = recipe
                .resume(Journal::from_json(&runner.journal().to_json().unwrap()).unwrap())
                .unwrap();
        }
        let view = runner.view().unwrap();
        assert_eq!(
            view["children"][0]["coin"]["puzzle_hash"],
            json!(vec![0x23; 32])
        );
        assert_eq!(view["funding_request"]["fee"], 0);
        assert_eq!(view["funding_request"]["input_amount"], 1000);
        assert_eq!(
            view["funding_request"]["recipient_puzzle_hash"],
            "23".repeat(32)
        );
        assert_eq!(
            view["checks"]["fresh_cargo_build"],
            "unavailable_test_control"
        );
        assert_eq!(
            view["funding_request"]["status"],
            "public_plan_only_no_live_funding_observed"
        );
        runner.execute(Command::ReorgUnspent).unwrap();
        let mut runner = recipe.resume(runner.into_journal()).unwrap();
        assert_eq!(runner.journal().stage(), Stage::ReadyToSubmit);
        runner.execute(Command::Advance).unwrap();
        runner.execute(Command::Refresh).unwrap();
        assert_eq!(runner.journal().stage(), Stage::Confirmed);
    }

    #[test]
    fn recipient_mode_and_retained_fixture_mismatch_cannot_resume_checked_journal() {
        let recipe = reference_recipe_control([0x23; 32], PaymentMode::On).unwrap();
        let mut runner = recipe.start().unwrap();
        runner.execute(Command::Advance).unwrap();
        runner.execute(Command::Advance).unwrap();
        let journal = || Journal::from_json(&runner.journal().to_json().unwrap()).unwrap();
        assert!(
            reference_recipe_control([0x24; 32], PaymentMode::On)
                .unwrap()
                .resume(journal())
                .is_err()
        );
        assert!(
            reference_recipe_control([0x23; 32], PaymentMode::Off)
                .unwrap()
                .resume(journal())
                .is_err()
        );
        assert!(SimulatorRunner::resume(journal()).is_err());
        let mut forged: Value =
            serde_json::from_slice(&runner.journal().to_json().unwrap()).unwrap();
        forged["entries"][1]["event"]["record"]["native_receipt"] = json!([1]);
        let journal = Journal::from_json(&serde_json::to_vec(&forged).unwrap()).unwrap();
        assert!(recipe.resume(journal).is_err());
    }
}

#[cfg(test)]
mod receipt_compatibility_tests {
    use super::*;
    #[test]
    fn historical_hex_receipt_and_preflight_metrics_remain_explicit() {
        let runner = SimulatorRunner::new().unwrap();
        let receipt: Value = serde_json::from_slice(&runner.checks.clvm_receipt).unwrap();
        assert_eq!(
            receipt["outputs"],
            json!([{"puzzle_hash":"42".repeat(32),"amount":1000}])
        );
        let view = runner.view().unwrap();
        let metrics = &view["metrics"];
        assert_eq!(metrics["reveal_bytes"], runner.recipe.artifact.reveal.len());
        assert_eq!(metrics["solution_bytes"], 1);
        assert_eq!(metrics["plan_fee"], 0);
        assert_eq!(metrics["preflight_sdk_validation"]["cost"], receipt["cost"]);
        assert_eq!(
            metrics["preflight_sdk_validation"]["flags"],
            receipt["flags"]
        );
        assert_eq!(
            metrics["preflight_sdk_validation"]["input_coin_id"],
            receipt["coin_id"]
        );
        assert_eq!(
            metrics["preflight_sdk_validation"]["sdk_vm"],
            "clvmr 0.16.2"
        );
        assert!(receipt["cost"].as_u64().unwrap() > 0);
    }
}
