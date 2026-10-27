use super::*;
use chia_sdk_test::{Simulator, SimulatorError};
use clvm_traits::{clvm_list, clvm_quote};

const FIXTURE_MASTER_SEED: [u8; 32] = [0x39; 32];
const OTHER_FIXTURE_SEED: [u8; 32] = [0x48; 32];
const FIXTURE_PAYMENT_HASH: [u8; 32] = [0x71; 32];

fn network() -> Testnet11Network {
    Testnet11Network::from_provider_labels("testnet11", "txch").unwrap()
}
fn fixture() -> (WalletIdentity, MasterSecret) {
    let master = SecretKey::from_seed(&FIXTURE_MASTER_SEED);
    let synthetic = master_to_wallet_unhardened(&master, 0).derive_synthetic();
    let hash = StandardArgs::curry_tree_hash(synthetic.public_key()).to_bytes();
    (
        WalletIdentity::from_master_public_key(master.public_key().to_bytes(), hash).unwrap(),
        MasterSecret(master),
    )
}
fn fixture_coin(identity: &WalletIdentity, amount: u64) -> Coin {
    Coin::new([0x32; 32].into(), identity.puzzle_hash.into(), amount)
}
fn funding(amount: u64, fee: u64) -> (FundingPlan, MasterSecret) {
    let (identity, secret) = fixture();
    (
        FundingPlan::new(
            identity,
            fixture_coin(&identity, amount),
            FIXTURE_PAYMENT_HASH,
            fee,
            0,
            &network(),
        )
        .unwrap(),
        secret,
    )
}
fn quote(conditions: Conditions) -> Vec<u8> {
    let mut allocator = Allocator::new_limited(VM_HEAP);
    encode(&mut allocator, &clvm_quote!(conditions)).unwrap()
}
fn payment_reveal(identity: &WalletIdentity) -> Vec<u8> {
    quote(
        Conditions::new()
            .with(AssertMyAmount::new(PAYMENT_AMOUNT))
            .with(CreateCoin::new(
                identity.puzzle_hash.into(),
                PAYMENT_AMOUNT,
                Memos::<NodePtr>::None,
            )),
    )
}
fn sponsor_fixture() -> (PaymentSponsorPlan, MasterSecret) {
    let (identity, secret) = fixture();
    let reveal = payment_reveal(&identity);
    let mut allocator = Allocator::new_limited(VM_HEAP);
    let puzzle = node_from_bytes(&mut allocator, &reveal).unwrap();
    let payment = Coin::new(
        [0x52; 32].into(),
        tree_hash(&allocator, puzzle).to_bytes().into(),
        PAYMENT_AMOUNT,
    );
    let sponsor = fixture_coin(&identity, 1_000_000);
    (
        PaymentSponsorPlan::new(
            identity,
            payment,
            &reveal,
            sponsor,
            DEFAULT_FEE,
            0,
            &network(),
        )
        .unwrap(),
        secret,
    )
}
fn require_rejection(simulator: &mut Simulator, bundle: SpendBundle, expected: ErrorCode) {
    let before = simulator.clone();
    match simulator.new_transaction(bundle) {
        Err(SimulatorError::Validation(actual)) => assert_eq!(actual, expected),
        _ => panic!("expected exact SDK consensus rejection"),
    }
    assert_eq!(
        simulator, &before,
        "rejected transaction changed complete simulator state"
    );
}

#[test]
fn public_master_path_matches_known_public_only_wallet_observation() {
    let key=hex::decode("8d9dba2d2b83b25845894af96a8d0e8fb05a277f2b4b12bf1f550b2eae250e351546cac0e13a3bd8663a25cb65bb1109").unwrap();
    let hash =
        hex::decode("dc2eeaf3acf70a3e5924fd38ef2b3c1ffb26d2edc7e02456abb79abd2bd3cbc1").unwrap();
    let identity =
        WalletIdentity::from_master_public_key(key.try_into().unwrap(), hash.try_into().unwrap())
            .unwrap();
    assert_eq!(
        hex::encode(identity.synthetic_public_key()),
        "b197d811f607379651b68e6de58ba193ed05e7c5bf3d47752a931ff4a16d250090af18aebe483d126ec0e704b12da615"
    );
}
#[test]
fn wrong_network_labels_and_private_token_are_rejected_before_tty() {
    for (name, prefix) in [
        ("mainnet", "xch"),
        ("testnet11", "xch"),
        ("testnet10", "txch"),
        ("TESTNET11", "txch"),
    ] {
        assert!(matches!(
            Testnet11Network::from_provider_labels(name, prefix),
            Err(WalletError::NetworkMismatch)
        ));
    }
    let (plan, _) = funding(1_000_000, DEFAULT_FEE);
    let invalid = Testnet11Network { genesis: [0; 32] };
    assert!(matches!(
        plan.sign_hidden_tty(plan.input_coin(), &invalid),
        Err(WalletError::NetworkMismatch)
    ));
}
#[test]
fn invalid_infinity_and_wrong_public_role_hash_are_refused() {
    let (identity, secret) = fixture();
    for key in [[0; 48], PublicKey::default().to_bytes()] {
        assert!(matches!(
            WalletIdentity::from_master_public_key(key, identity.puzzle_hash),
            Err(WalletError::InvalidPublicKey)
        ));
    }
    assert!(matches!(
        WalletIdentity::from_master_public_key(secret.0.public_key().to_bytes(), [0; 32]),
        Err(WalletError::WalletHashMismatch)
    ));
    assert!(matches!(
        WalletIdentity::from_master_public_key(identity.synthetic.to_bytes(), identity.puzzle_hash),
        Err(WalletError::WalletHashMismatch)
    ));
}
#[test]
fn funding_exact_amount_change_and_fee_boundaries() {
    for fee in [0, DEFAULT_FEE, MAX_FEE] {
        let (plan, _secret) = funding(1_000_000_000_000, fee);
        let signed = plan.sign_fixture(FIXTURE_MASTER_SEED).unwrap();
        assert_eq!(signed.payment_coin().amount, PAYMENT_AMOUNT);
        assert_eq!(
            signed.change_coin().amount,
            1_000_000_000_000 - PAYMENT_AMOUNT - fee
        );
        assert_eq!(
            signed.change_coin().puzzle_hash,
            plan.0.identity.puzzle_hash.into()
        );
        assert_eq!(signed.fee(), fee);
        println!(
            "RESEARCH funding fee={} total={} generator_bytes={} byte_rate={} execution={} conditions={}",
            fee,
            signed.cost(),
            signed.charged_generator_bytes(),
            TESTNET11_CONSTANTS.cost_per_byte,
            signed.execution_cost(),
            signed.condition_cost()
        );
        assert!(signed.cost() > 0 && signed.cost() <= MAX_WALLET_BUNDLE_COST);
        assert_eq!(
            plan.verify_signed_bundle(signed.bundle_bytes())
                .unwrap()
                .transaction_id(),
            signed.transaction_id()
        );
    }
}
#[test]
fn underfunding_wrong_input_and_duplicate_outputs_are_refused() {
    let (identity, _) = fixture();
    let build =
        |coin, destination, fee| FundingPlan::new(identity, coin, destination, fee, 0, &network());
    assert!(matches!(
        build(
            fixture_coin(&identity, PAYMENT_AMOUNT + DEFAULT_FEE - 1),
            FIXTURE_PAYMENT_HASH,
            DEFAULT_FEE
        ),
        Err(WalletError::AmountBound)
    ));
    assert!(matches!(
        build(
            fixture_coin(&identity, u64::MAX),
            FIXTURE_PAYMENT_HASH,
            MAX_FEE + 1
        ),
        Err(WalletError::FeeBound)
    ));
    assert!(matches!(
        build(
            Coin::new([1; 32].into(), [0; 32].into(), 1_000_000),
            FIXTURE_PAYMENT_HASH,
            DEFAULT_FEE
        ),
        Err(WalletError::InputMismatch)
    ));
    assert!(matches!(
        build(
            fixture_coin(&identity, 2 * PAYMENT_AMOUNT),
            identity.puzzle_hash,
            0
        ),
        Err(WalletError::DuplicateOutput)
    ));
    // Exact equality does not underflow: an explicit zero-mojo change output
    // remains part of the two-output plan and is checked by actual consensus.
    let plan = build(
        fixture_coin(&identity, PAYMENT_AMOUNT + DEFAULT_FEE),
        FIXTURE_PAYMENT_HASH,
        DEFAULT_FEE,
    )
    .unwrap();
    assert_eq!(plan.change_coin().amount, 0);
    assert_eq!(
        plan.sign_fixture(FIXTURE_MASTER_SEED)
            .unwrap()
            .change_coin()
            .amount,
        0
    );
}
#[test]
fn requeried_input_changes_abort_before_tty() {
    let (plan, _) = funding(1_000_000, DEFAULT_FEE);
    for coin in [
        Coin::new(
            [3; 32].into(),
            plan.input_coin().puzzle_hash,
            plan.input_coin().amount,
        ),
        Coin::new(
            plan.input_coin().parent_coin_info,
            [4; 32].into(),
            plan.input_coin().amount,
        ),
        Coin::new(
            plan.input_coin().parent_coin_info,
            plan.input_coin().puzzle_hash,
            plan.input_coin().amount + 1,
        ),
    ] {
        assert!(matches!(
            plan.sign_hidden_tty(coin, &network()),
            Err(WalletError::InputMismatch)
        ));
    }
}
#[test]
fn wrong_master_secret_cannot_sign_the_plan() {
    let (plan, _) = funding(1_000_000, DEFAULT_FEE);
    let wrong = MasterSecret(SecretKey::from_seed(&OTHER_FIXTURE_SEED));
    assert!(matches!(plan.0.sign(&wrong), Err(WalletError::KeyMismatch)));
}
#[test]
fn persisted_bytes_require_exact_spends_length_signature_and_bls() {
    let (plan, secret) = funding(1_000_000, DEFAULT_FEE);
    let signed = plan.0.sign(&secret).unwrap();
    let bytes = signed.bundle_bytes();
    for mutated in [
        Vec::new(),
        vec![0xff; MAX_BUNDLE + 1],
        bytes[..bytes.len() - 1].to_vec(),
        [bytes, &[0]].concat(),
        {
            let mut x = bytes.to_vec();
            x[0] ^= 1;
            x
        },
        {
            let mut x = bytes.to_vec();
            x[10] ^= 1;
            x
        },
    ] {
        assert!(matches!(
            plan.verify_signed_bundle(&mutated),
            Err(WalletError::ChangedBundle)
        ));
    }
    let mut bad = bytes.to_vec();
    let last = bad.len() - 96;
    bad[last..].fill(0xff);
    assert!(matches!(
        plan.verify_signed_bundle(&bad),
        Err(WalletError::SignatureEncoding)
    ));
    assert!(matches!(
        plan.verify_signed_bundle(plan.unsigned_bundle_bytes()),
        Err(WalletError::Consensus(ErrorCode::BadAggregateSignature))
    ));
    let wrong = SecretKey::from_seed(&OTHER_FIXTURE_SEED);
    let wrong_bundle = SpendBundle::new(
        plan.0.spends.clone(),
        chia_bls::sign(&wrong, plan.0.requirement.message()),
    )
    .to_bytes()
    .unwrap();
    assert!(matches!(
        plan.verify_signed_bundle(&wrong_bundle),
        Err(WalletError::Consensus(ErrorCode::BadAggregateSignature))
    ));
}
#[test]
fn sdk_funding_accepts_exact_outputs_and_rejects_bad_signature_and_replay_with_full_equality() {
    let (plan, secret) = funding(1_000_000, DEFAULT_FEE);
    let mut simulator = Simulator::new();
    simulator.insert_coin(plan.input_coin());
    require_rejection(
        &mut simulator,
        SpendBundle::new(plan.0.spends.clone(), Signature::default()),
        ErrorCode::BadAggregateSignature,
    );
    let wrong = SecretKey::from_seed(&OTHER_FIXTURE_SEED);
    require_rejection(
        &mut simulator,
        SpendBundle::new(
            plan.0.spends.clone(),
            chia_bls::sign(&wrong, plan.0.requirement.message()),
        ),
        ErrorCode::BadAggregateSignature,
    );
    let synthetic = secret.synthetic_for(&plan.0.identity).unwrap();
    let mut wrong_domain = plan.0.requirement.clone();
    wrong_domain.domain_string = Some(chia_sdk_types::MAINNET_CONSTANTS.agg_sig_me_additional_data);
    let wrong_domain_bundle = SpendBundle::new(
        plan.0.spends.clone(),
        chia_bls::sign(&synthetic, wrong_domain.message()),
    );
    assert!(matches!(
        plan.verify_signed_bundle(&wrong_domain_bundle.to_bytes().unwrap()),
        Err(WalletError::Consensus(ErrorCode::BadAggregateSignature))
    ));
    require_rejection(
        &mut simulator,
        wrong_domain_bundle,
        ErrorCode::BadAggregateSignature,
    );
    let signed = plan.0.sign(&secret).unwrap();
    simulator.new_transaction(signed.bundle().clone()).unwrap();
    let mut actual = simulator
        .children(plan.input_coin().coin_id())
        .into_iter()
        .map(|x| x.coin)
        .collect::<Vec<_>>();
    let mut expected = vec![plan.payment_coin(), plan.change_coin()];
    actual.sort_by_key(Coin::coin_id);
    expected.sort_by_key(Coin::coin_id);
    assert_eq!(actual, expected);
    require_rejection(
        &mut simulator,
        signed.bundle().clone(),
        ErrorCode::DoubleSpend,
    );
}
#[test]
fn prior_provisional_total_ceiling_is_explicitly_inconclusive() {
    let (plan, _secret) = funding(1_000_000_000_000, DEFAULT_FEE);
    let signed = plan.sign_fixture(FIXTURE_MASTER_SEED).unwrap();
    let flags = get_flags_for_height_and_constants(0, &TESTNET11_CONSTANTS) | MEMPOOL_MODE;
    let error = validate_clvm_and_signature(
        signed.bundle(),
        MAX_POLICY_VM_COST,
        &TESTNET11_CONSTANTS,
        flags,
    )
    .err()
    .unwrap();
    assert_eq!(error.1, ErrorCode::CostExceeded);
    assert!(signed.cost() > MAX_POLICY_VM_COST && signed.cost() < MAX_WALLET_BUNDLE_COST);
}

#[test]
fn payment_sponsor_preserves_full_payment_and_returns_fee_change() {
    let (plan, _secret) = sponsor_fixture();
    let signed = plan.sign_fixture(FIXTURE_MASTER_SEED).unwrap();
    assert_eq!(signed.bundle().coin_spends.len(), 2);
    println!(
        "RESEARCH sponsor fee={} total={} generator_bytes={} byte_rate={} execution={} conditions={}",
        signed.fee(),
        signed.cost(),
        signed.charged_generator_bytes(),
        TESTNET11_CONSTANTS.cost_per_byte,
        signed.execution_cost(),
        signed.condition_cost()
    );
    assert_eq!(signed.payment_coin().amount, PAYMENT_AMOUNT);
    assert_eq!(signed.change_coin().amount, 1_000_000 - DEFAULT_FEE);
    assert_eq!(
        signed.payment_coin().puzzle_hash,
        signed.change_coin().puzzle_hash
    );
    assert!(signed.cost() > 0 && signed.cost() <= MAX_WALLET_BUNDLE_COST);
    assert_eq!(
        plan.verify_signed_bundle(signed.bundle_bytes())
            .unwrap()
            .transaction_id(),
        signed.transaction_id()
    );
}
#[test]
fn sponsor_refuses_wrong_payment_program_amount_destination_extra_conditions_and_hash() {
    let (identity, _) = fixture();
    let sponsor = fixture_coin(&identity, 1_000_000);
    let expected = Conditions::new()
        .with(AssertMyAmount::new(PAYMENT_AMOUNT))
        .with(CreateCoin::new(
            identity.puzzle_hash.into(),
            PAYMENT_AMOUNT,
            Memos::<NodePtr>::None,
        ));
    for conditions in [
        Conditions::new().with(CreateCoin::new(
            identity.puzzle_hash.into(),
            PAYMENT_AMOUNT,
            Memos::<NodePtr>::None,
        )),
        Conditions::new()
            .with(AssertMyAmount::new(PAYMENT_AMOUNT))
            .with(CreateCoin::new(
                [4; 32].into(),
                PAYMENT_AMOUNT,
                Memos::<NodePtr>::None,
            )),
        expected.clone().with(ReserveFee::new(0)),
        Conditions::new(),
    ] {
        let reveal = quote(conditions);
        let mut allocator = Allocator::new_limited(VM_HEAP);
        let puzzle = node_from_bytes(&mut allocator, &reveal).unwrap();
        let coin = Coin::new(
            [0x51; 32].into(),
            tree_hash(&allocator, puzzle).to_bytes().into(),
            PAYMENT_AMOUNT,
        );
        assert!(matches!(
            PaymentSponsorPlan::new(identity, coin, &reveal, sponsor, DEFAULT_FEE, 0, &network()),
            Err(WalletError::ConditionMismatch)
        ));
    }
    let reveal = payment_reveal(&identity);
    assert!(matches!(
        PaymentSponsorPlan::new(
            identity,
            Coin::new([2; 32].into(), [9; 32].into(), PAYMENT_AMOUNT),
            &reveal,
            sponsor,
            DEFAULT_FEE,
            0,
            &network()
        ),
        Err(WalletError::InputMismatch)
    ));
    assert!(matches!(
        PaymentSponsorPlan::new(
            identity,
            Coin::new([2; 32].into(), [9; 32].into(), PAYMENT_AMOUNT + 1),
            &reveal,
            sponsor,
            DEFAULT_FEE,
            0,
            &network()
        ),
        Err(WalletError::InputMismatch)
    ));
    assert!(matches!(
        PaymentSponsorPlan::new(
            identity,
            Coin::new([2; 32].into(), [9; 32].into(), PAYMENT_AMOUNT),
            &vec![0; MAX_PAYMENT_REVEAL + 1],
            sponsor,
            DEFAULT_FEE,
            0,
            &network()
        ),
        Err(WalletError::ProgramBuild)
    ));
}
#[test]
fn sponsor_refuses_wrong_fee_input_fee_bound_or_changed_requeried_coins_before_tty() {
    let (plan, _) = sponsor_fixture();
    let identity = plan.0.identity;
    let reveal = plan.0.spends[0].puzzle_reveal.to_vec();
    assert!(matches!(
        PaymentSponsorPlan::new(
            identity,
            plan.input_coin(),
            &reveal,
            Coin::new([2; 32].into(), [9; 32].into(), 1_000_000),
            DEFAULT_FEE,
            0,
            &network()
        ),
        Err(WalletError::InputMismatch)
    ));
    assert!(matches!(
        PaymentSponsorPlan::new(
            identity,
            plan.input_coin(),
            &reveal,
            plan.sponsor_coin(),
            MAX_FEE + 1,
            0,
            &network()
        ),
        Err(WalletError::FeeBound)
    ));
    assert!(matches!(
        PaymentSponsorPlan::new(
            identity,
            plan.input_coin(),
            &reveal,
            fixture_coin(&identity, DEFAULT_FEE - 1),
            DEFAULT_FEE,
            0,
            &network()
        ),
        Err(WalletError::AmountBound)
    ));
    assert!(matches!(
        plan.sign_hidden_tty(plan.sponsor_coin(), plan.input_coin(), &network()),
        Err(WalletError::InputMismatch)
    ));
}
#[test]
fn sponsor_partial_bundle_cannot_spend_fee_coin_and_sdk_negatives_preserve_full_state() {
    let (plan, secret) = sponsor_fixture();
    let signed = plan.0.sign(&secret).unwrap();
    let partial = SpendBundle::new(
        vec![signed.bundle().coin_spends[1].clone()],
        signed.bundle().aggregated_signature.clone(),
    );
    assert!(matches!(
        plan.verify_signed_bundle(&partial.to_bytes().unwrap()),
        Err(WalletError::ChangedBundle)
    ));
    let mut simulator = Simulator::new();
    simulator.insert_coin(plan.input_coin());
    simulator.insert_coin(plan.sponsor_coin());
    require_rejection(
        &mut simulator,
        partial,
        ErrorCode::AssertConcurrentSpendFailed,
    );
    require_rejection(
        &mut simulator,
        SpendBundle::new(plan.0.spends.clone(), Signature::default()),
        ErrorCode::BadAggregateSignature,
    );
    simulator.new_transaction(signed.bundle().clone()).unwrap();
    assert_eq!(
        simulator.children(plan.input_coin().coin_id())[0].coin,
        plan.payment_coin()
    );
    assert_eq!(
        simulator.children(plan.sponsor_coin().coin_id())[0].coin,
        plan.change_coin()
    );
    require_rejection(
        &mut simulator,
        signed.bundle().clone(),
        ErrorCode::DoubleSpend,
    );
}

#[test]
fn constructed_conditional_program_shows_why_fresh_source_authority_is_required() {
    // This deliberately bypasses the private live parent's source authority.
    // It is a constructed raw-program counterexample, not the fresh Rust pay
    // recipe or an actual compiler mutation. Nil-only execution is insufficient
    // to authorize an arbitrary payment program for sponsorship.
    let (identity, secret) = fixture();
    let honest = Conditions::new()
        .with(AssertMyAmount::new(PAYMENT_AMOUNT))
        .with(CreateCoin::new(
            identity.puzzle_hash.into(),
            PAYMENT_AMOUNT,
            Memos::<NodePtr>::None,
        ));
    let redirected_hash = [0x67; 32];
    let redirected = Conditions::new()
        .with(AssertMyAmount::new(PAYMENT_AMOUNT))
        .with(CreateCoin::new(
            redirected_hash.into(),
            PAYMENT_AMOUNT,
            Memos::<NodePtr>::None,
        ));
    let mut allocator = Allocator::new_limited(VM_HEAP);
    // Independent CLVM wire-format control: if environment then redirected
    // else honest. The source-authorized recipe must reject nonnil instead.
    let reveal = encode(
        &mut allocator,
        &clvm_list!(3u8, 1u8, clvm_quote!(redirected), clvm_quote!(honest)),
    )
    .unwrap();
    let puzzle = node_from_bytes(&mut allocator, &reveal).unwrap();
    let payment = Coin::new(
        [0x65; 32].into(),
        tree_hash(&allocator, puzzle).to_bytes().into(),
        PAYMENT_AMOUNT,
    );
    let sponsor = fixture_coin(&identity, 1_000_000);
    let plan = PaymentSponsorPlan::new(
        identity,
        payment,
        &reveal,
        sponsor,
        DEFAULT_FEE,
        0,
        &network(),
    )
    .unwrap();
    let original = plan.0.sign(&secret).unwrap();
    let mut changed = original.bundle().clone();
    changed.coin_spends[0].solution = Program::from(vec![1]);
    assert!(matches!(
        plan.verify_signed_bundle(&changed.to_bytes().unwrap()),
        Err(WalletError::ChangedBundle)
    ));
    // A third party need not call our exact-byte checker. The unchanged wallet
    // signature binds its delegated puzzle and concurrent coin ID, not the
    // other coin's environment. Actual consensus accepts the constructed twin.
    let mut simulator = Simulator::new();
    simulator.insert_coin(payment);
    simulator.insert_coin(sponsor);
    simulator.new_transaction(changed).unwrap();
    assert_eq!(
        simulator.children(payment.coin_id())[0]
            .coin
            .puzzle_hash
            .to_bytes(),
        redirected_hash
    );
}
