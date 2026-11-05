use super::*;
use crate::live::{LiveNetwork, LiveState, network::*, wallet::*};
use chia_bls::{SecretKey, master_to_wallet_unhardened};
use chia_consensus::validation_error::ErrorCode;
use chia_puzzle_types::{DeriveSynthetic, standard::StandardArgs};
use chia_sdk_test::{Simulator, SimulatorError};
use std::sync::OnceLock;
const SEED: [u8; 32] = [0x67; 32];
fn module() -> Vec<u8> {
    static MODULE: OnceLock<Vec<u8>> = OnceLock::new();
    MODULE
        .get_or_init(|| {
            chialisp_oxide_reference::compile(include_str!(
                "../../../../../experiments/authenticated-continuations/reference.clsp"
            ))
            .unwrap()
        })
        .clone()
}
fn recipe(count: usize) -> ContinuationRecipe {
    ContinuationRecipe::checked(
        crate::fresh::continuation::fixture(module()),
        ContinuationMode::On,
        (0..count)
            .map(|i| [u8::try_from(i + 1).unwrap(); 32])
            .collect(),
    )
    .unwrap()
}
fn identity() -> WalletIdentity {
    let master = SecretKey::from_seed(&SEED);
    let synthetic = master_to_wallet_unhardened(&master, 0).derive_synthetic();
    WalletIdentity::from_master_public_key(
        master.public_key().to_bytes(),
        StandardArgs::curry_tree_hash(synthetic.public_key()).to_bytes(),
    )
    .unwrap()
}
fn net_identity() -> NetworkIdentity {
    NetworkIdentity {
        provider_network_name: "testnet11".into(),
        provider_network_prefix: "txch".into(),
        origin: ORIGIN.into(),
        local_genesis_challenge: TESTNET11_CONSTANTS.genesis_challenge.to_bytes(),
        local_agg_sig_me_domain: TESTNET11_CONSTANTS.agg_sig_me_additional_data.to_bytes(),
        scope: "scripted provider over actual SDK simulator".into(),
    }
}
struct Mock {
    sim: Simulator,
    pushes: usize,
    lose_response: bool,
    queries: usize,
    corrupt: Option<Digest>,
}
impl Mock {
    fn new() -> Self {
        let mut sim = Simulator::new();
        sim.create_block();
        sim.new_coin(identity().wallet_puzzle_hash().into(), 1_000_000);
        Self {
            sim,
            pushes: 0,
            lose_response: false,
            queries: 0,
            corrupt: None,
        }
    }
}
impl LiveNetwork for Mock {
    fn verify_network(&mut self) -> crate::live::network::Result<NetworkIdentity> {
        self.queries += 1;
        Ok(net_identity())
    }
    fn synced_tip(&mut self) -> crate::live::network::Result<LiveTip> {
        self.queries += 1;
        Ok(LiveTip {
            height: self.sim.height(),
            header_hash: self.sim.header_hash().to_bytes(),
        })
    }
    fn coin(&mut self, id: Digest) -> crate::live::network::Result<Option<LiveCoinRecord>> {
        self.queries += 1;
        let mut result = self.sim.coin_state(id.into()).map(|state| LiveCoinRecord {
            parent: state.coin.parent_coin_info.to_bytes(),
            puzzle_hash: state.coin.puzzle_hash.to_bytes(),
            amount: state.coin.amount,
            confirmed_height: state.created_height.unwrap(),
            spent_height: state.spent_height.unwrap_or(0),
            spent: state.spent_height.is_some(),
            coinbase: false,
            timestamp: 0,
        });
        if self.corrupt == Some(id)
            && let Some(coin) = &mut result
        {
            coin.spent = false;
            coin.spent_height = coin.confirmed_height.max(1);
        }
        Ok(result)
    }
    fn observed_spend(
        &mut self,
        id: Digest,
        _: u32,
    ) -> crate::live::network::Result<ObservedSpend> {
        self.queries += 1;
        let spend = self.sim.coin_spend(id.into()).unwrap();
        Ok(ObservedSpend {
            parent: spend.coin.parent_coin_info.to_bytes(),
            puzzle_hash: spend.coin.puzzle_hash.to_bytes(),
            amount: spend.coin.amount,
            puzzle_reveal: spend.puzzle_reveal.as_ref().to_vec(),
            solution: spend.solution.as_ref().to_vec(),
        })
    }
    fn address_coins(&mut self, ph: Digest) -> crate::live::network::Result<CoinSelection> {
        self.queries += 1;
        let ids = self.sim.unspent_coins(ph.into(), false);
        let mut records = vec![];
        for coin in ids {
            records.push(self.coin(coin.coin_id().to_bytes())?.unwrap());
        }
        Ok(CoinSelection {
            records,
            complete: false,
            scope: "bounded candidates only".into(),
        })
    }
    fn push(&mut self, bundle: &SpendBundle) -> crate::live::network::Result<SubmissionOutcome> {
        self.pushes += 1;
        self.sim.new_transaction(bundle.clone()).unwrap();
        if self.lose_response {
            return Err(Error {
                kind: ErrorKind::Timeout,
                message: "response lost after actual acceptance".into(),
            });
        }
        Ok(SubmissionOutcome::MempoolAccepted {
            transaction_id: bundle.name().to_bytes(),
        })
    }
    fn take_receipts(&mut self) -> Vec<HttpReceipt> {
        vec![]
    }
}
impl ContinuationNetwork for Mock {
    fn continuation_tip(&mut self) -> crate::live::network::Result<(LiveTip, u32)> {
        Ok((self.synced_tip()?, self.sim.height().saturating_sub(1)))
    }
}
fn funding(session: &mut ContinuationSession, net: &mut Mock) {
    session
        .prepare_funding_with(net, |plan, _, _| Ok(plan.sign_fixture(SEED)?))
        .unwrap();
}
fn step(session: &mut ContinuationSession, net: &mut Mock) {
    session
        .prepare_step_with(net, |plan, _, _, _| Ok(plan.sign_fixture(SEED)?))
        .unwrap();
}
#[test]
fn complete_queue_counts_curry_openings_and_exact_terminal_effects() {
    for count in 1..=8 {
        let recipe = recipe(count);
        let mut coin = CoinRecord {
            parent: [7; 32],
            puzzle_hash: recipe.funding_puzzle_hash(),
            amount: 1000 * count as u64,
        };
        let mut sim = Simulator::new();
        sim.insert_coin(coin.coin());
        for index in 0..count {
            let prepared = recipe.prepare(index, coin.clone(), 0).unwrap();
            assert_eq!(
                prepared.children.len(),
                if index + 1 == count { 1 } else { 2 }
            );
            let bundle = prepared.spend_bundle().unwrap();
            sim.new_transaction(bundle).unwrap();
            if index + 1 < count {
                coin = prepared.children[1].clone();
            }
        }
    }
}
#[test]
fn actual_sdk_bad_funding_signature_and_double_spend_keep_full_simulator_state() {
    let recipe = recipe(4);
    let id = identity();
    let network = Testnet11Network::from_provider_labels("testnet11", "txch").unwrap();
    let mut sim = Simulator::new();
    let input = sim.new_coin(id.wallet_puzzle_hash().into(), 1_000_000);
    let plan =
        continuation::ContinuationFundingPlan::new(id, input, &recipe, 10000, 0, &network).unwrap();
    let checked = plan.sign_fixture(SEED).unwrap();
    let bad = SpendBundle::new(checked.bundle().coin_spends.clone(), Signature::default());
    let before = sim.clone();
    assert!(matches!(
        sim.new_transaction(bad),
        Err(SimulatorError::Validation(ErrorCode::BadAggregateSignature))
    ));
    assert_eq!(sim, before);
    sim.new_transaction(checked.bundle().clone()).unwrap();
    let before = sim.clone();
    assert!(matches!(
        sim.new_transaction(checked.bundle().clone()),
        Err(SimulatorError::Validation(ErrorCode::DoubleSpend))
    ));
    assert_eq!(sim, before);
}
#[test]
fn shared_session_funds_sponsors_all_steps_and_reloads_exact_queue() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir
        .path()
        .canonicalize()
        .unwrap()
        .join("trial.continuation.live.json");
    let mut session = ContinuationSession::create(
        recipe(4),
        identity(),
        ContinuationStore::new(&path).unwrap(),
        10000,
        100,
    )
    .unwrap();
    let mut net = Mock::new();
    funding(&mut session, &mut net);
    session.submit(&mut net).unwrap();
    assert_eq!(session.monitor(&mut net).unwrap(), LiveState::Confirmed);
    for _ in 0..4 {
        step(&mut session, &mut net);
        session.submit(&mut net).unwrap();
        assert_eq!(session.monitor(&mut net).unwrap(), LiveState::Confirmed);
        session = ContinuationSession::load(
            recipe(4),
            identity(),
            ContinuationStore::new(&path).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(net.pushes, 5);
    assert_eq!(session.outbox().transactions().len(), 5);
    assert!(session.prepare_step(&mut net).is_err());
    assert!(
        ContinuationSession::load(
            recipe(3),
            identity(),
            ContinuationStore::new(&path).unwrap()
        )
        .is_err()
    );
}
#[test]
fn crash_after_push_intent_never_retries_and_stale_revision_precedes_network_and_signing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir
        .path()
        .canonicalize()
        .unwrap()
        .join("trial.continuation.live.json");
    let mut session = ContinuationSession::create(
        recipe(1),
        identity(),
        ContinuationStore::new(&path).unwrap(),
        10000,
        0,
    )
    .unwrap();
    let mut stale = ContinuationSession::load(
        recipe(1),
        identity(),
        ContinuationStore::new(&path).unwrap(),
    )
    .unwrap();
    let mut net = Mock::new();
    funding(&mut session, &mut net);
    let queries = net.queries;
    let mut called = false;
    assert!(
        stale
            .prepare_funding_with(&mut net, |_, _, _| {
                called = true;
                panic!("stale signer")
            })
            .is_err()
    );
    assert!(!called);
    assert_eq!(net.queries, queries);
    // Worker startup must revalidate and display a legitimate external append,
    // rather than returning the original in-memory empty outbox snapshot.
    let view = stale.reload_view_current().unwrap();
    assert_eq!(view["prepared_transactions"], 1);
    assert_eq!(view["revision"], hex::encode(session.revision()));
    assert_eq!(stale.outbox(), session.outbox());
    assert_eq!(net.queries, queries);
    net.lose_response = true;
    assert!(session.submit(&mut net).is_err());
    let mut loaded = ContinuationSession::load(
        recipe(1),
        identity(),
        ContinuationStore::new(&path).unwrap(),
    )
    .unwrap();
    assert!(loaded.submit(&mut net).is_err());
    assert_eq!(net.pushes, 1);
    assert_eq!(loaded.monitor(&mut net).unwrap(), LiveState::Confirmed);
    step(&mut loaded, &mut net);
    net.lose_response = false;
    loaded.submit(&mut net).unwrap();
    assert_eq!(loaded.monitor(&mut net).unwrap(), LiveState::Confirmed);
}
#[test]
fn future_coin_snapshot_is_inconclusive_and_exact_child_mutation_conflicts() {
    let recipe = recipe(1);
    let input = CoinRecord {
        parent: [9; 32],
        puzzle_hash: recipe.funding_puzzle_hash(),
        amount: 1000,
    };
    let p = recipe.prepare(0, input, 0).unwrap();
    let tx = super::session::CheckedTransaction {
        bundle: p.spend_bundle().unwrap(),
        children: p.children.clone(),
    };
    let coin = &tx.bundle.coin_spends[0].coin;
    let mut evidence = crate::live::MonitoringEvidence {
        network: net_identity(),
        before: LiveTip {
            height: 10,
            header_hash: [0; 32],
        },
        after: LiveTip {
            height: 10,
            header_hash: [0; 32],
        },
        inputs: vec![Some(LiveCoinRecord {
            parent: coin.parent_coin_info.to_bytes(),
            puzzle_hash: coin.puzzle_hash.to_bytes(),
            amount: 1000,
            confirmed_height: 11,
            spent_height: 12,
            spent: true,
            coinbase: false,
            timestamp: 0,
        })],
        spends: vec![None],
        children: vec![None],
        receipts: vec![],
    };
    assert_eq!(
        super::session::observe(&tx, &evidence).unwrap(),
        LiveState::Inconclusive
    );
    evidence.inputs[0].as_mut().unwrap().confirmed_height = 1;
    evidence.inputs[0].as_mut().unwrap().spent_height = 2;
    let spend = &tx.bundle.coin_spends[0];
    evidence.spends[0] = Some(ObservedSpend {
        parent: spend.coin.parent_coin_info.to_bytes(),
        puzzle_hash: spend.coin.puzzle_hash.to_bytes(),
        amount: 1000,
        puzzle_reveal: spend.puzzle_reveal.as_ref().to_vec(),
        solution: spend.solution.as_ref().to_vec(),
    });
    evidence.children[0] = Some(LiveCoinRecord {
        parent: p.children[0].parent,
        puzzle_hash: [0; 32],
        amount: 1000,
        confirmed_height: 2,
        spent_height: 0,
        spent: false,
        coinbase: false,
        timestamp: 0,
    });
    assert_eq!(
        super::session::observe(&tx, &evidence).unwrap(),
        LiveState::Conflict
    );
    // Height-zero creation remains valid. Contradictory retained metadata must
    // conflict before the newer-provider-snapshot inconclusive classification.
    evidence.children[0].as_mut().unwrap().puzzle_hash = p.children[0].puzzle_hash;
    evidence.inputs[0].as_mut().unwrap().confirmed_height = 0;
    assert_eq!(
        super::session::observe(&tx, &evidence).unwrap(),
        LiveState::Confirmed
    );
    for (confirmed, spent, height) in [(2, true, 0), (2, true, 1), (11, false, 12), (11, true, 0)] {
        let child = evidence.children[0].as_mut().unwrap();
        child.confirmed_height = confirmed;
        child.spent = spent;
        child.spent_height = height;
        assert_eq!(
            super::session::observe(&tx, &evidence).unwrap(),
            LiveState::Conflict
        );
    }
    let child = evidence.children[0].as_mut().unwrap();
    child.confirmed_height = 11;
    child.spent = true;
    child.spent_height = 12;
    assert_eq!(
        super::session::observe(&tx, &evidence).unwrap(),
        LiveState::Inconclusive
    );
    let input = evidence.inputs[0].as_mut().unwrap();
    input.confirmed_height = 11;
    input.spent = true;
    input.spent_height = 0;
    assert_eq!(
        super::session::observe(&tx, &evidence).unwrap(),
        LiveState::Conflict
    );
}

#[test]
fn bounded_store_rejects_stale_cas_rewritten_binding_and_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let path = root.join("store.continuation.live.json");
    let session = ContinuationSession::create(
        recipe(1),
        identity(),
        ContinuationStore::new(&path).unwrap(),
        10000,
        0,
    )
    .unwrap();
    let store = ContinuationStore::new(&path).unwrap();
    let mut rewritten = session.outbox().clone();
    rewritten.binding.plan.recipients[0] = [0x99; 32];
    assert!(store.commit(Some(session.revision()), &rewritten).is_err());
    assert!(store.commit(Some([0; 32]), session.outbox()).is_err());
    assert_eq!(store.load().unwrap().byte_digest, session.revision());
    #[cfg(unix)]
    {
        let link = root.join("link.continuation.live.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(ContinuationStore::new(link).is_err());
    }
}

#[tokio::test]
async fn loopback_continuation_page_and_cached_view_use_the_same_sealed_session() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir
        .path()
        .canonicalize()
        .unwrap()
        .join("gui.continuation.live.json");
    let session = ContinuationSession::create(
        recipe(2),
        identity(),
        ContinuationStore::new(&path).unwrap(),
        10000,
        100,
    )
    .unwrap();
    let revision = hex::encode(session.revision());
    let server = crate::live::http::LiveLoopbackServer::bind_continuation(session)
        .await
        .unwrap();
    let url = server.url().to_owned();
    let (stop, done) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(server.run_until(async {
        let _ = done.await;
    }));
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let html = client.get(&url).send().await.unwrap().text().await.unwrap();
    assert!(html.contains("Committed payout queue"));
    let capability = html
        .split("name=\"oxide-session\" content=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let state: serde_json::Value = client
        .get(format!("{url}/api/state"))
        .header("x-oxide-session", capability)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .map(|text| serde_json::from_str(&text).unwrap())
        .unwrap();
    assert_eq!(state["view"]["revision"], revision);
    assert_eq!(state["view"]["plan"]["steps"].as_array().unwrap().len(), 2);
    assert_eq!(
        state["view"]["available_actions"],
        serde_json::json!(["reload", "prepare_funding"])
    );
    let script = client
        .get(format!("{url}/app.js"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(script.contains("view.available_actions"));
    assert!(!script.contains("fixed payment amount"));
    stop.send(()).unwrap();
    task.await.unwrap().unwrap();
}

#[test]
fn contradictory_child_metadata_conflicts_in_monitor_and_rejects_retained_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir
        .path()
        .canonicalize()
        .unwrap()
        .join("records.continuation.live.json");
    let mut session = ContinuationSession::create(
        recipe(1),
        identity(),
        ContinuationStore::new(&path).unwrap(),
        10000,
        0,
    )
    .unwrap();
    let mut net = Mock::new();
    funding(&mut session, &mut net);
    session.submit(&mut net).unwrap();
    assert_eq!(session.monitor(&mut net).unwrap(), LiveState::Confirmed);
    let original = std::fs::read(&path).unwrap();
    let mut tampered = session.outbox().clone();
    let ContinuationEvent::Observed { evidence, .. } = tampered.events.last_mut().unwrap() else {
        panic!("expected retained observation")
    };
    let child = evidence.children[0].as_mut().unwrap();
    let child_id = child.id();
    child.spent = true;
    child.spent_height = 0;
    std::fs::write(&path, serde_json::to_vec(&tampered).unwrap()).unwrap();
    assert!(
        ContinuationSession::load(
            recipe(1),
            identity(),
            ContinuationStore::new(&path).unwrap()
        )
        .is_err()
    );
    std::fs::write(&path, original).unwrap();
    net.corrupt = Some(child_id);
    assert_eq!(session.monitor(&mut net).unwrap(), LiveState::Conflict);
    assert_eq!(session.outbox().state(0), Some(LiveState::Conflict));
}
