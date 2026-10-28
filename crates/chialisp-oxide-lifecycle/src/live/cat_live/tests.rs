use super::*;
use chia_bls::{SecretKey, Signature, master_to_wallet_unhardened};
use chia_puzzle_types::{DeriveSynthetic, standard::StandardArgs};
use network::{CoinSelection, HttpReceipt, LiveTip, ObservedSpend, SubmissionOutcome};
use std::collections::BTreeMap;
// Explicit public fixture seed, unrelated to the operator wallet.
const SEED: [u8; 32] = [0x57; 32];
fn declaration() -> NetworkIdentity {
    NetworkIdentity {
        provider_network_name: "testnet11".into(),
        provider_network_prefix: "txch".into(),
        origin: network::ORIGIN.into(),
        local_genesis_challenge: TESTNET11_CONSTANTS.genesis_challenge.to_bytes(),
        local_agg_sig_me_domain: TESTNET11_CONSTANTS.agg_sig_me_additional_data.to_bytes(),
        scope: "scripted provider test control, not live chain evidence".into(),
    }
}
fn live_record(coin: &CoinRecord, spent: bool, height: u32) -> LiveCoinRecord {
    LiveCoinRecord {
        parent: coin.parent,
        puzzle_hash: coin.puzzle_hash,
        amount: coin.amount,
        confirmed_height: if spent { 1 } else { height },
        spent_height: if spent { height } else { 0 },
        spent,
        coinbase: false,
        timestamp: 1,
    }
}
struct TipHook {
    at_call: u8,
    run: Box<dyn FnOnce()>,
}
struct ScriptedNetwork {
    simulator: chialisp_oxide_simulator::Simulator,
    coins: BTreeMap<Digest, LiveCoinRecord>,
    spends: BTreeMap<Digest, ObservedSpend>,
    pushed: Vec<Vec<u8>>,
    timeout_next: bool,
    outbox_path: Option<std::path::PathBuf>,
    moving_tip: bool,
    tip_calls: u8,
    rpc_calls: usize,
    tip_hook: Option<TipHook>,
}
impl ScriptedNetwork {
    fn new() -> Self {
        Self {
            simulator: chialisp_oxide_simulator::Simulator::new(),
            coins: BTreeMap::new(),
            spends: BTreeMap::new(),
            pushed: vec![],
            timeout_next: false,
            outbox_path: None,
            moving_tip: false,
            tip_calls: 0,
            rpc_calls: 0,
            tip_hook: None,
        }
    }
    fn confirmed(&mut self, transfer: &CatPrepared) {
        let bundle = decode_bundle(&decode_hex(&transfer.bundle_hex, MAX_BUNDLE).unwrap()).unwrap();
        self.simulator.new_transaction(bundle.clone()).unwrap();
        for (input, spend) in transfer.inputs.iter().zip(bundle.coin_spends) {
            self.coins.insert(input.id(), live_record(input, true, 20));
            self.spends.insert(
                input.id(),
                ObservedSpend {
                    parent: input.parent,
                    puzzle_hash: input.puzzle_hash,
                    amount: input.amount,
                    puzzle_reveal: spend.puzzle_reveal.to_vec(),
                    solution: spend.solution.to_vec(),
                },
            );
        }
        for child in &transfer.children {
            self.coins.insert(child.id(), live_record(child, false, 20));
        }
    }
}
impl LiveNetwork for ScriptedNetwork {
    fn verify_network(&mut self) -> network::Result<NetworkIdentity> {
        self.rpc_calls += 1;
        Ok(declaration())
    }
    fn synced_tip(&mut self) -> network::Result<LiveTip> {
        self.rpc_calls += 1;
        self.tip_calls = self.tip_calls.saturating_add(1);
        if self
            .tip_hook
            .as_ref()
            .is_some_and(|hook| hook.at_call == self.tip_calls)
        {
            (self.tip_hook.take().unwrap().run)();
        }
        Ok(LiveTip {
            height: 30,
            header_hash: if self.moving_tip {
                [self.tip_calls; 32]
            } else {
                [0x18; 32]
            },
        })
    }
    fn coin(&mut self, id: Digest) -> network::Result<Option<LiveCoinRecord>> {
        self.rpc_calls += 1;
        Ok(self.coins.get(&id).cloned())
    }
    fn observed_spend(&mut self, id: Digest, _: u32) -> network::Result<ObservedSpend> {
        self.rpc_calls += 1;
        self.spends.get(&id).cloned().ok_or(network::Error {
            kind: network::ErrorKind::Inconclusive,
            message: "missing scripted spend".into(),
        })
    }
    fn address_coins(&mut self, hash: Digest) -> network::Result<CoinSelection> {
        self.rpc_calls += 1;
        Ok(CoinSelection {
            records: self
                .coins
                .values()
                .filter(|coin| coin.puzzle_hash == hash)
                .cloned()
                .collect(),
            complete: false,
            scope: "bounded scripted candidates".into(),
        })
    }
    fn push(&mut self, bundle: &SpendBundle) -> network::Result<SubmissionOutcome> {
        self.rpc_calls += 1;
        // Read the actual CAS/fsynced file before recording any simulated
        // external effect, including a deliberately lost provider response.
        let stored = CatStore::new(self.outbox_path.as_ref().expect("fixture outbox path"))
            .unwrap()
            .load()
            .unwrap();
        assert!(
            matches!(stored.outbox.events().last(), Some(CatEvent::PushStarted {bundle_id,..})
            if *bundle_id == bundle.name().to_bytes())
        );
        self.pushed.push(bundle.to_bytes().unwrap());
        if self.timeout_next {
            self.timeout_next = false;
            return Err(network::Error {
                kind: network::ErrorKind::Timeout,
                message: "scripted lost response".into(),
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
fn fixture_recipe() -> CatRecipe {
    let master = SecretKey::from_seed(&SEED);
    let synthetic = master_to_wallet_unhardened(&master, 0).derive_synthetic();
    let wallet = StandardArgs::curry_tree_hash(synthetic.public_key()).to_bytes();
    let authority =
        CatTailAuthority::fixture_recorded(&chia_puzzles::GENESIS_BY_COIN_ID, CatMode::RustOn)
            .unwrap();
    // Fixture-only constructor is private to this test. No actual-source credit.
    CatRecipe::with_source(
        authority,
        master.public_key().to_bytes(),
        wallet,
        10_000,
        10_000,
        digest(b"public canonical fixture; no source execution authority"),
    )
    .unwrap()
}
fn session() -> (tempfile::TempDir, CatSession, ScriptedNetwork) {
    let recipe = fixture_recipe();
    let directory = tempfile::tempdir().unwrap();
    let store = CatStore::new(
        directory
            .path()
            .canonicalize()
            .unwrap()
            .join("fixture.cat.live.json"),
    )
    .unwrap();
    let genesis = CoinRecord {
        parent: [0x61; 32],
        puzzle_hash: recipe.binding.wallet_puzzle_hash,
        amount: 1_000_000,
    };
    let mut network = ScriptedNetwork::new();
    network.outbox_path = Some(store.path().to_path_buf());
    network.simulator.insert_coin(genesis.coin());
    network
        .coins
        .insert(genesis.id(), live_record(&genesis, false, 1));
    (
        directory,
        CatSession::create(recipe, store).unwrap(),
        network,
    )
}
fn funding(session: &mut CatSession, network: &mut ScriptedNetwork) -> Result<()> {
    session.prepare_funding_with(network, |plan, coin, _token| {
        ensure!(coin == plan.input_coin(), "fixture expected input");
        Ok(plan.sign_fixture(SEED)?)
    })
}
fn issuance(session: &mut CatSession, network: &mut ScriptedNetwork) -> Result<()> {
    session.prepare_issuance_with(network, |plan, coins, height, token| {
        plan.recheck_for_signing(coins, height, token)?;
        let key = master_to_wallet_unhardened(&SecretKey::from_seed(&SEED), 0).derive_synthetic();
        let mut signature = Signature::default();
        for req in plan.required_bls() {
            ensure!(
                req.public_key == key.public_key(),
                "fixture required signing role"
            );
            signature += &chia_bls::sign(&key, req.message());
        }
        Ok(plan.verify_returned_signature(&signature)?)
    })
}
#[test]
fn full_fixture_chain_resumes_and_retries_exact_saved_bytes() {
    let (_dir, mut session, mut network) = session();
    funding(&mut session, &mut network).unwrap();
    let funding = session.outbox.transfer(CatStage::Funding).unwrap().clone();
    let selected = session.outbox.genesis().unwrap().clone();
    assert_eq!(selected.genesis, funding.inputs[0]);
    network.timeout_next = true;
    session.submit(&mut network, CatStage::Funding).unwrap();
    let store = CatStore::new(session.store.path()).unwrap();
    let mut session = CatSession::load(fixture_recipe(), store).unwrap();
    session.submit(&mut network, CatStage::Funding).unwrap();
    assert_eq!(network.pushed[0], network.pushed[1]);
    network.confirmed(&funding);
    assert_eq!(
        session.monitor(&mut network, CatStage::Funding).unwrap(),
        LiveState::Confirmed
    );
    issuance(&mut session, &mut network).unwrap();
    let issued = session.outbox.transfer(CatStage::Issuance).unwrap().clone();
    assert_eq!(issued.inputs, vec![selected.eve, selected.funding_change]);
    assert_eq!(issued.children[0], selected.normal);
    assert!(issued.cost > 25_000_000 && issued.cost <= 100_000_000);
    session.submit(&mut network, CatStage::Issuance).unwrap();
    network.confirmed(&issued);
    assert_eq!(
        session.monitor(&mut network, CatStage::Issuance).unwrap(),
        LiveState::Confirmed
    );
    let pushes = network.pushed.len();
    session.submit(&mut network, CatStage::Issuance).unwrap();
    assert_eq!(network.pushed.len(), pushes);
    // G is already spent: resuming issuance checks saved G intent but queries
    // exact Eve/sponsor, and never requires G to become unspent again.
    let store = CatStore::new(session.store.path()).unwrap();
    let resumed = CatSession::load(fixture_recipe(), store).unwrap();
    resumed.status(&mut network).unwrap();
}
#[test]
fn selection_survives_signer_failure_and_cas_prevents_any_push() {
    let (_dir, mut session, mut network) = session();
    assert!(
        session
            .prepare_funding_with(&mut network, |_, _, _| anyhow::bail!(
                "fixture cancellation"
            ))
            .is_err()
    );
    let selected = session.outbox.genesis().unwrap().clone();
    assert!(session.outbox.transfer(CatStage::Funding).is_none());
    let store = CatStore::new(session.store.path()).unwrap();
    let mut stale = CatSession::load(fixture_recipe(), store).unwrap();
    funding(&mut session, &mut network).unwrap();
    assert_eq!(session.outbox.genesis(), Some(&selected));
    assert!(funding(&mut stale, &mut network).is_err());
    let store = CatStore::new(session.store.path()).unwrap();
    let mut stale = CatSession::load(fixture_recipe(), store).unwrap();
    session.monitor(&mut network, CatStage::Funding).unwrap();
    assert!(stale.submit(&mut network, CatStage::Funding).is_err());
    assert!(network.pushed.is_empty());
}
#[test]
fn tampered_signed_bytes_source_fee_asset_and_spend_effects_are_rejected() {
    let (_dir, mut session, mut network) = session();
    funding(&mut session, &mut network).unwrap();
    let original = session.outbox.clone();
    for field in [0, 1, 2] {
        session.outbox = original.clone();
        match field {
            0 => session.outbox.binding.source_identity[0] ^= 1,
            1 => session.outbox.binding.fee += 1,
            _ => {
                if let CatEvent::GenesisSelected { intent } = &mut session.outbox.events[0] {
                    intent.asset_id[0] ^= 1
                }
            }
        }
        assert!(session.status(&mut network).is_err());
    }
    session.outbox = original.clone();
    if let CatEvent::Prepared { transfer } = &mut session.outbox.events[1] {
        let mut bytes = decode_hex(&transfer.bundle_hex, MAX_BUNDLE).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        transfer.bundle_hex = hex::encode(&bytes);
        transfer.bundle_sha256 = digest(&bytes);
    }
    assert!(session.status(&mut network).is_err());
    session.outbox = original;
    let funding = session.outbox.transfer(CatStage::Funding).unwrap().clone();
    network.confirmed(&funding);
    network
        .spends
        .get_mut(&funding.inputs[0].id())
        .unwrap()
        .solution = vec![0x80];
    assert_eq!(
        session.monitor(&mut network, CatStage::Funding).unwrap(),
        LiveState::Conflict
    );
    assert!(issuance(&mut session, &mut network).is_err());
    assert!(network.pushed.is_empty());
}
#[test]
fn moving_tip_blocks_before_secret_callback_and_observation_is_inconclusive() {
    let (_dir, mut session, mut network) = session();
    network.moving_tip = true;
    let called = std::cell::Cell::new(false);
    assert!(
        session
            .prepare_funding_with(&mut network, |_, _, _| {
                called.set(true);
                anyhow::bail!("must not run")
            })
            .is_err()
    );
    assert!(!called.get());
    network.moving_tip = false;
    funding(&mut session, &mut network).unwrap();
    network.moving_tip = true;
    assert_eq!(
        session.monitor(&mut network, CatStage::Funding).unwrap(),
        LiveState::Inconclusive
    );
    assert!(session.submit(&mut network, CatStage::Funding).is_err());
    assert!(network.pushed.is_empty());
}
#[test]
fn outbox_limits_unknown_fields_and_aliases_are_fail_closed() {
    let (_dir, session, _network) = session();
    let bytes = serde_json::to_vec(&session.outbox).unwrap();
    let raw = String::from_utf8(bytes.clone())
        .unwrap()
        .replace("\"events\":[]", "\"unknown\":0,\"events\":[]");
    std::fs::write(session.store.path(), raw).unwrap();
    assert!(session.store.load().is_err());
    std::fs::write(session.store.path(), vec![b' '; store::MAX_BYTES + 1]).unwrap();
    assert!(session.store.load().is_err());
    std::fs::write(session.store.path(), bytes).unwrap();
    let alias = session.store.path().with_file_name("hard.cat.live.json");
    std::fs::hard_link(session.store.path(), &alias).unwrap();
    assert!(session.store.load().is_err());
    assert!(CatStore::new(session.store.path().with_file_name("ordinary.live.json")).is_err());
}

#[test]
fn valid_external_append_during_final_tip_blocks_both_signer_readers() {
    let (_dir, mut session, mut network) = session();
    let path = session.store.path().to_path_buf();
    network.tip_hook = Some(TipHook {
        at_call: 2,
        run: Box::new(move || {
            let store = CatStore::new(path).unwrap();
            let mut saved = store.load().unwrap();
            let recipe = fixture_recipe();
            let token = Testnet11Network::from_provider_labels("testnet11", "txch").unwrap();
            let mint = recipe
                .mint(&saved.outbox.genesis().unwrap().genesis, 30, &token)
                .unwrap();
            let signed = mint.funding_plan().sign_fixture(SEED).unwrap();
            saved
                .outbox
                .append(CatEvent::Prepared {
                    transfer: prepared_funding(&signed, 30),
                })
                .unwrap();
            store
                .commit(Some(saved.byte_digest), &saved.outbox)
                .unwrap();
        }),
    });
    let called = std::cell::Cell::new(false);
    let error = session
        .prepare_funding_with(&mut network, |_, _, _| {
            called.set(true);
            anyhow::bail!("funding reader must not run")
        })
        .unwrap_err();
    assert!(error.to_string().contains("revision changed"));
    assert!(!called.get());
    assert!(network.pushed.is_empty());
    // Resume the other valid writer's exact funding, then arrange another valid
    // append at issuance's final tip query after its own current funding monitor.
    let store = CatStore::new(session.store.path()).unwrap();
    let mut session = CatSession::load(fixture_recipe(), store).unwrap();
    let funding = session.outbox.transfer(CatStage::Funding).unwrap().clone();
    network.confirmed(&funding);
    network.tip_calls = 0;
    let path = session.store.path().to_path_buf();
    network.tip_hook = Some(TipHook {
        at_call: 4,
        run: Box::new(move || {
            let store = CatStore::new(path).unwrap();
            let mut saved = store.load().unwrap();
            let event = saved.outbox.events().last().unwrap().clone();
            assert!(matches!(
                event,
                CatEvent::Observed {
                    state: LiveState::Confirmed,
                    ..
                }
            ));
            saved.outbox.append(event).unwrap();
            store
                .commit(Some(saved.byte_digest), &saved.outbox)
                .unwrap();
        }),
    });
    let error = session
        .prepare_issuance_with(&mut network, |_, _, _, _| {
            called.set(true);
            anyhow::bail!("issuance reader must not run")
        })
        .unwrap_err();
    assert!(error.to_string().contains("revision changed"));
    assert!(!called.get());
    assert!(network.pushed.is_empty());
}
#[test]
fn stale_action_entries_make_no_provider_requests() {
    let (_dir, mut owner, mut network) = session();
    funding(&mut owner, &mut network).unwrap();
    let store = CatStore::new(owner.store.path()).unwrap();
    let mut stale = CatSession::load(fixture_recipe(), store).unwrap();
    owner.monitor(&mut network, CatStage::Funding).unwrap();
    network.rpc_calls = 0;
    let called = std::cell::Cell::new(false);
    let results = [
        stale.status(&mut network),
        stale.prepare_funding_with(&mut network, |_, _, _| {
            called.set(true);
            anyhow::bail!("stale reader must not run")
        }),
        stale.prepare_issuance_with(&mut network, |_, _, _, _| {
            called.set(true);
            anyhow::bail!("stale reader must not run")
        }),
        stale.submit(&mut network, CatStage::Funding),
        stale.monitor(&mut network, CatStage::Funding).map(|_| ()),
    ];
    for result in results {
        assert!(result.unwrap_err().to_string().contains("revision changed"));
    }
    assert_eq!(network.rpc_calls, 0);
    assert!(!called.get());
    assert!(network.pushed.is_empty());
}
