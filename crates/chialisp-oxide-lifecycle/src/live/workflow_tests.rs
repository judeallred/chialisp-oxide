use super::*;
use chia_bls::{SecretKey, master_to_wallet_unhardened};
use chia_puzzle_types::{DeriveSynthetic, standard::StandardArgs};
use std::collections::BTreeMap;

// Public disposable unit-test seed, unrelated to any operator wallet.
const FIXTURE_MASTER_SEED: [u8; 32] = [0x57; 32];

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
fn fixture(payment_fee: u64) -> (LiveRecipe, WalletIdentity) {
    let master = SecretKey::from_seed(&FIXTURE_MASTER_SEED);
    let synthetic = master_to_wallet_unhardened(&master, 0).derive_synthetic();
    let hash = StandardArgs::curry_tree_hash(synthetic.public_key()).to_bytes();
    let wallet =
        WalletIdentity::from_master_public_key(master.public_key().to_bytes(), hash).unwrap();
    // Independently compiled reference control only. This cannot enter
    // LiveRecipe::from_fresh or assert current Rust compiler execution.
    let reveal = chialisp_oxide_reference::compile(&format!(
        "(mod () (list (list 73 1000) (list 51 0x{} 1000)))",
        hex::encode(hash)
    ))
    .unwrap();
    let artifact = Artifact {
        reveal,
        solution: vec![0x80],
    };
    let plan = Plan {
        network: crate::provider::testnet11_network(),
        amount: 1000,
        outputs: vec![crate::Output {
            puzzle_hash: hash,
            amount: 1000,
        }],
    };
    let binding = LiveBinding {
        network: plan.network.clone(),
        wallet_puzzle_hash: hash,
        master_public_key_hex: hex::encode(master.public_key().to_bytes()),
        artifact_digest: artifact_digest(&artifact).unwrap(),
        native_check_digest: digest(b"reference test control, no native execution credit"),
        clvm_check_digest: digest(b"reference test control"),
        recipient: hash,
        fee: DEFAULT_FEE,
        payment_fee,
    };
    (
        LiveRecipe {
            plan,
            artifact,
            binding,
        },
        wallet,
    )
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
struct ScriptedNetwork {
    coins: BTreeMap<Digest, LiveCoinRecord>,
    spends: BTreeMap<Digest, ObservedSpend>,
    pushed: Vec<Vec<u8>>,
    timeout_next: bool,
}
impl ScriptedNetwork {
    fn new() -> Self {
        Self {
            coins: BTreeMap::new(),
            spends: BTreeMap::new(),
            pushed: vec![],
            timeout_next: false,
        }
    }
    fn unspent(&mut self, transfer: &PreparedTransfer) {
        for input in &transfer.inputs {
            self.coins.insert(input.id(), live_record(input, false, 1));
        }
    }
    fn confirmed(&mut self, transfer: &PreparedTransfer) {
        let bundle = decode_bundle(&decode_hex(&transfer.bundle_hex, MAX_BUNDLE).unwrap()).unwrap();
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
        Ok(declaration())
    }
    fn synced_tip(&mut self) -> network::Result<LiveTip> {
        Ok(LiveTip {
            height: 30,
            header_hash: [0x18; 32],
        })
    }
    fn coin(&mut self, id: Digest) -> network::Result<Option<LiveCoinRecord>> {
        Ok(self.coins.get(&id).cloned())
    }
    fn observed_spend(&mut self, id: Digest, _: u32) -> network::Result<ObservedSpend> {
        self.spends.get(&id).cloned().ok_or(network::Error {
            kind: network::ErrorKind::Inconclusive,
            message: "missing scripted spend".into(),
        })
    }
    fn address_coins(&mut self, hash: Digest) -> network::Result<CoinSelection> {
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
fn session(payment_fee: u64) -> (tempfile::TempDir, LiveSession, ScriptedNetwork) {
    let (recipe, wallet) = fixture(payment_fee);
    let directory = tempfile::tempdir().unwrap();
    let store = LiveStore::new(
        directory
            .path()
            .canonicalize()
            .unwrap()
            .join("fixture.live.json"),
    )
    .unwrap();
    let input = CoinRecord {
        parent: [0x61; 32],
        puzzle_hash: wallet.wallet_puzzle_hash(),
        amount: 1_000_000,
    };
    let plan = FundingPlan::new(
        wallet,
        input.coin(),
        recipe.funding_puzzle_hash().unwrap(),
        DEFAULT_FEE,
        0,
        &require_network(&declaration()).unwrap(),
    )
    .unwrap();
    let signed = plan.sign_fixture(FIXTURE_MASTER_SEED).unwrap();
    let funding = prepared(TransferKind::Funding, vec![input], &signed, 0);
    let mut session = LiveSession::create(recipe, store).unwrap();
    session
        .commit(LiveEvent::Prepared {
            transfer: funding.clone(),
        })
        .unwrap();
    let mut network = ScriptedNetwork::new();
    network.unspent(&funding);
    (directory, session, network)
}
#[test]
fn timeout_retry_uses_same_durable_bytes_and_payment_uses_actual_funding_child() {
    let (directory, mut session, mut network) = session(0);
    network.timeout_next = true;
    session.submit(&mut network, TransferKind::Funding).unwrap();
    assert!(matches!(
        session.outbox.events.last(),
        Some(LiveEvent::PushResult {
            outcome: PushOutcome::Inconclusive {
                kind: network::ErrorKind::Timeout
            },
            ..
        })
    ));
    let (recipe, _) = fixture(0);
    let store = LiveStore::new(
        directory
            .path()
            .canonicalize()
            .unwrap()
            .join("fixture.live.json"),
    )
    .unwrap();
    let mut session = LiveSession::load(recipe, store).unwrap();
    session.submit(&mut network, TransferKind::Funding).unwrap();
    assert_eq!(network.pushed.len(), 2);
    assert_eq!(network.pushed[0], network.pushed[1]);
    let funding = session
        .outbox
        .transfer(TransferKind::Funding)
        .unwrap()
        .clone();
    network.confirmed(&funding);
    session.prepare_payment(&mut network).unwrap();
    let payment = session
        .outbox
        .transfer(TransferKind::Payment)
        .unwrap()
        .clone();
    assert_eq!(payment.inputs, vec![funding.children[0].clone()]);
    assert_eq!(payment.fee, 0);
    session.submit(&mut network, TransferKind::Payment).unwrap();
    network.confirmed(&payment);
    assert_eq!(
        session
            .monitor(&mut network, TransferKind::Payment)
            .unwrap(),
        LiveState::Confirmed
    );
    let count = network.pushed.len();
    session.submit(&mut network, TransferKind::Payment).unwrap();
    assert_eq!(
        network.pushed.len(),
        count,
        "observed exact effects stop resubmission"
    );
}
#[test]
fn signed_sponsor_is_separate_and_observation_requires_both_inputs_and_all_children() {
    let (_directory, mut session, mut network) = session(DEFAULT_FEE);
    let funding = session
        .outbox
        .transfer(TransferKind::Funding)
        .unwrap()
        .clone();
    network.confirmed(&funding);
    assert_eq!(
        session
            .monitor(&mut network, TransferKind::Funding)
            .unwrap(),
        LiveState::Confirmed
    );
    let identity = session.identity().unwrap();
    let plan = PaymentSponsorPlan::new(
        identity,
        funding.children[0].coin(),
        &session.recipe.artifact.reveal,
        funding.children[1].coin(),
        DEFAULT_FEE,
        0,
        &require_network(&declaration()).unwrap(),
    )
    .unwrap();
    let signed = plan.sign_fixture(FIXTURE_MASTER_SEED).unwrap();
    let payment = prepared(
        TransferKind::Payment,
        vec![funding.children[0].clone(), funding.children[1].clone()],
        &signed,
        0,
    );
    session
        .commit(LiveEvent::Prepared {
            transfer: payment.clone(),
        })
        .unwrap();
    session.submit(&mut network, TransferKind::Payment).unwrap();
    network.confirmed(&payment);
    network.coins.remove(&payment.children[1].id());
    assert_eq!(
        session
            .monitor(&mut network, TransferKind::Payment)
            .unwrap(),
        LiveState::Inconclusive
    );
    network.coins.insert(
        payment.children[1].id(),
        live_record(&payment.children[1], false, 20),
    );
    assert_eq!(
        session
            .monitor(&mut network, TransferKind::Payment)
            .unwrap(),
        LiveState::Confirmed
    );
    network
        .spends
        .get_mut(&payment.inputs[1].id())
        .unwrap()
        .solution
        .push(0x80);
    assert_eq!(
        session
            .monitor(&mut network, TransferKind::Payment)
            .unwrap(),
        LiveState::Conflict
    );
}
#[test]
fn coherent_retained_tampering_never_reaches_push() {
    for field in ["children", "cost", "fee", "bundle"] {
        let (_directory, mut session, mut network) = session(0);
        let LiveEvent::Prepared { transfer } = &mut session.outbox.events[0] else {
            unreachable!()
        };
        match field {
            "children" => transfer.children[0].puzzle_hash = [0x99; 32],
            "cost" => transfer.cost += 1,
            "fee" => transfer.fee += 1,
            "bundle" => {
                transfer.bundle_hex.replace_range(0..8, "ffffffff");
                transfer.bundle_sha256 = digest(&hex::decode(&transfer.bundle_hex).unwrap());
            }
            _ => unreachable!(),
        }
        assert!(
            session.submit(&mut network, TransferKind::Funding).is_err(),
            "accepted {field}"
        );
        assert!(network.pushed.is_empty());
    }
}
#[test]
fn crash_before_response_reconciles_and_partial_or_moving_tips_do_not_confirm() {
    let (_directory, mut session, mut network) = session(0);
    let funding = session
        .outbox
        .transfer(TransferKind::Funding)
        .unwrap()
        .clone();
    session
        .commit(LiveEvent::PushStarted {
            kind: TransferKind::Funding,
            bundle_id: funding.bundle_id,
        })
        .unwrap();
    network.confirmed(&funding);
    session.submit(&mut network, TransferKind::Funding).unwrap();
    assert!(network.pushed.is_empty());
    let LiveEvent::Observed { evidence, .. } = session.outbox.events.last().unwrap() else {
        unreachable!()
    };
    let mut evidence = evidence.clone();
    evidence.after.header_hash = [0x99; 32];
    assert_eq!(
        observe_state(&funding, &evidence).unwrap(),
        LiveState::Inconclusive
    );
    evidence.after = evidence.before.clone();
    evidence.children[0] = None;
    assert_eq!(
        observe_state(&funding, &evidence).unwrap(),
        LiveState::Inconclusive
    );
    evidence.children[0] = Some(live_record(&funding.children[0], false, 19));
    assert_eq!(
        observe_state(&funding, &evidence).unwrap(),
        LiveState::Conflict
    );
}
#[test]
fn only_a_current_executed_fresh_recipe_can_construct_production_live_capability() {
    let retained = crate::runner::SimulatorRunner::new()
        .unwrap()
        .validated_recipe();
    let (_, wallet) = fixture(0);
    assert!(
        LiveRecipe::from_fresh(
            &retained,
            wallet.master_public_key(),
            wallet.wallet_puzzle_hash(),
            DEFAULT_FEE,
            0
        )
        .is_err()
    );
}
#[test]
fn serialized_bundle_preflight_rejects_declared_counts_atoms_depth_and_trailing_bytes() {
    let mut count = vec![0xff; 4];
    count.resize(100, 0);
    assert!(decode_bundle(&count).is_err());
    for program in [
        vec![0xfe],
        vec![0xfc, 0xff, 0xff, 0xff, 0xff, 0xff],
        vec![0xff; 513],
    ] {
        let mut bytes = vec![0, 0, 0, 1];
        bytes.extend_from_slice(&[0; 72]);
        bytes.extend_from_slice(&program);
        bytes.extend_from_slice(&[0x80; 100]);
        assert!(decode_bundle(&bytes).is_err());
    }
    let (_directory, session, _) = session(0);
    let transfer = session.outbox.transfer(TransferKind::Funding).unwrap();
    let mut bytes = hex::decode(&transfer.bundle_hex).unwrap();
    assert!(decode_bundle(&bytes).is_ok());
    bytes.push(0);
    assert!(decode_bundle(&bytes).is_err());
}

#[test]
fn outbox_event_exhaustion_blocks_push_before_external_side_effects() {
    let (_directory, mut session, mut network) = session(0);
    while session.outbox.events().len() < MAX_EVENTS - 3 {
        assert_eq!(
            session
                .monitor(&mut network, TransferKind::Funding)
                .unwrap(),
            LiveState::Unspent
        );
    }
    let before = session.outbox.clone();
    assert!(session.submit(&mut network, TransferKind::Funding).is_err());
    assert_eq!(session.outbox, before);
    assert!(network.pushed.is_empty());
}

#[test]
fn future_confirmation_height_is_rejected_before_the_funding_signer() {
    let (recipe, wallet) = fixture(0);
    let directory = tempfile::tempdir().unwrap();
    let store = LiveStore::new(
        directory
            .path()
            .canonicalize()
            .unwrap()
            .join("future.live.json"),
    )
    .unwrap();
    let mut session = LiveSession::create(recipe, store).unwrap();
    let input = CoinRecord {
        parent: [0x62; 32],
        puzzle_hash: wallet.wallet_puzzle_hash(),
        amount: 1_000_000,
    };
    let mut network = ScriptedNetwork::new();
    network
        .coins
        .insert(input.id(), live_record(&input, false, 31));
    let before = session.outbox.clone();
    let error = session.prepare_funding(&mut network).unwrap_err();
    assert!(error.to_string().contains("future confirmation height"));
    assert_eq!(session.outbox, before);
    assert!(network.pushed.is_empty());
}
