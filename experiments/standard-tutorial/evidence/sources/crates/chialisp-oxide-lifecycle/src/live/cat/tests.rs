use super::*;
use chia_bls::{SecretKey, master_to_wallet_unhardened};
use chia_puzzle_types::{DeriveSynthetic, Memos, cat::CatSolution};
use chia_sdk_driver::{Cat, CatInfo, CatSpend};
use chia_sdk_test::{Simulator, SimulatorError};
use chia_sdk_types::{
    Conditions,
    conditions::{CreateCoin, RunCatTail},
};
use clvm_traits::{FromClvm, ToClvm};
use clvmr_sdk::{
    NodePtr,
    serde::{node_from_bytes, node_to_bytes},
};
use std::sync::{
    OnceLock,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn cost_exhaustion_is_inconclusive_without_relabeling_runtime_umbrellas() {
    assert_eq!(
        vm_preflight_error(ErrorCode::CostExceeded),
        CatError::VmInconclusive(ErrorCode::CostExceeded)
    );
    assert_eq!(
        vm_preflight_error(ErrorCode::GeneratorRuntimeError),
        CatError::Consensus(ErrorCode::GeneratorRuntimeError)
    );
    assert_eq!(
        vm_preflight_error(ErrorCode::BadAggregateSignature),
        CatError::Consensus(ErrorCode::BadAggregateSignature)
    );
}

#[derive(Clone, Copy, Debug)]
enum FixtureMode {
    RustOff,
    RustOn,
    CanonicalReference,
}

const OWNER_SEED: [u8; 32] = [0x39; 32];
const RECIPIENT_SEED: [u8; 32] = [0x58; 32];
const WRONG_SEED: [u8; 32] = [0x67; 32];
static SDK_ATTEMPTS: AtomicUsize = AtomicUsize::new(0);
fn network() -> Testnet11Network {
    Testnet11Network::from_provider_labels("testnet11", "txch").unwrap()
}
fn identity(seed: [u8; 32]) -> (WalletIdentity, SecretKey) {
    let master = SecretKey::from_seed(&seed);
    let key = master_to_wallet_unhardened(&master, 0).derive_synthetic();
    let hash =
        chia_puzzle_types::standard::StandardArgs::curry_tree_hash(key.public_key()).to_bytes();
    (
        WalletIdentity::from_master_public_key(master.public_key().to_bytes(), hash).unwrap(),
        key,
    )
}
fn authority(mode: FixtureMode) -> CatTailAuthority {
    static REPORT: OnceLock<serde_json::Value> = OnceLock::new();
    let report = REPORT.get_or_init(|| {
        serde_json::from_str(include_str!("../../../../../corpus/validation.json")).unwrap()
    });
    let row = report["puzzles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["slug"] == "cat-genesis-by-coin-id")
        .unwrap();
    let artifacts = row["artifacts"].as_array().unwrap();
    let matches = artifacts
        .iter()
        .filter(|a| match mode {
            FixtureMode::RustOff => a["backend"] == "rust_mir" && a["optimize_lir"] == false,
            FixtureMode::RustOn => a["backend"] == "rust_mir" && a["optimize_lir"] == true,
            FixtureMode::CanonicalReference => a["backend"] == "chialisp",
        })
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1);
    let row = matches[0];
    let raw = hex::decode(row["clvm_hex"].as_str().unwrap()).unwrap();
    assert_eq!(row["bytes"].as_u64().unwrap(), raw.len() as u64);
    let authority = match mode {
        FixtureMode::RustOff => CatTailAuthority::fixture_recorded(&raw, CatMode::RustOff),
        FixtureMode::RustOn => CatTailAuthority::fixture_recorded(&raw, CatMode::RustOn),
        FixtureMode::CanonicalReference => CatTailAuthority::fixture_reference(&raw),
    }
    .unwrap();
    assert_eq!(hex::encode(authority.module_hash()), row["tree_hash"]);
    authority
}
fn modes() -> [FixtureMode; 3] {
    [
        FixtureMode::RustOff,
        FixtureMode::RustOn,
        FixtureMode::CanonicalReference,
    ]
}
fn genesis(owner: &WalletIdentity, tag: u8) -> Coin {
    Coin::new(
        [tag; 32].into(),
        owner.wallet_puzzle_hash().into(),
        1_000_000_000_000,
    )
}
fn mint(authority: &CatTailAuthority, tag: u8) -> CatMintPlan {
    let (owner, _) = identity(OWNER_SEED);
    CatMintPlan::new(
        authority,
        owner,
        genesis(&owner, tag),
        owner.wallet_puzzle_hash(),
        CatFees::new(DEFAULT_CAT_FEE, DEFAULT_CAT_FEE).unwrap(),
        0,
        &network(),
    )
    .unwrap()
}
fn fixture_signature(plan: &CatSpendPlan, key: &SecretKey) -> Signature {
    let mut signature = Signature::default();
    for required in plan.required_bls() {
        assert_eq!(required.public_key, key.public_key());
        signature += &chia_bls::sign(key, required.message());
    }
    signature
}
fn sign(plan: &CatSpendPlan, key: &SecretKey) -> VerifiedCatBundle {
    plan.verify_returned_signature(&fixture_signature(plan, key))
        .unwrap()
}
fn accepted(simulator: &mut Simulator, bundle: SpendBundle, name: &str) {
    SDK_ATTEMPTS.fetch_add(1, Ordering::SeqCst);
    simulator.new_transaction(bundle).unwrap();
    println!("CAT_SDK_ACCEPTED {name}");
}
fn rejected(simulator: &mut Simulator, bundle: SpendBundle, code: ErrorCode, name: &str) {
    let before = simulator.clone();
    SDK_ATTEMPTS.fetch_add(1, Ordering::SeqCst);
    match simulator.new_transaction(bundle) {
        Err(SimulatorError::Validation(actual)) => assert_eq!(actual, code),
        _ => panic!("expected exact SDK consensus code"),
    }
    assert_eq!(
        simulator, &before,
        "typed rejected submission changed complete SDK state"
    );
    println!("CAT_SDK_REJECTED {name} {code:?} FULL_SIMULATOR_EQUAL");
}
fn fund(simulator: &mut Simulator, plan: &CatMintPlan) {
    let input = plan.funding_plan().input_coin();
    simulator.insert_coin(input);
    let signed = plan.funding_plan().sign_fixture(OWNER_SEED).unwrap();
    accepted(simulator, signed.bundle().clone(), "fund_exact_eve");
    assert!(
        simulator
            .children(input.coin_id())
            .iter()
            .any(|c| c.coin == plan.eve_coin())
    );
}
fn cost(mode: FixtureMode, kind: &str, costs: CatCosts) {
    assert_eq!(
        costs.total,
        costs.execution
            + costs.conditions
            + costs.charged_generator_bytes as u64 * costs.cost_per_byte
    );
    println!(
        "CAT_COST mode={mode:?} kind={kind} total={} bytes={} vm={} conditions={} bls={}",
        costs.total,
        costs.charged_generator_bytes,
        costs.execution,
        costs.conditions,
        costs.bls_verified
    );
}
fn primary_code(spend: &CoinSpend) -> chialisp_oxide_vm::FailureCode {
    let mut allocator = clvmr::Allocator::new_limited(MAX_CODEC_HEAP);
    let puzzle =
        clvmr::serde::node_from_bytes(&mut allocator, spend.puzzle_reveal.as_slice()).unwrap();
    let solution =
        clvmr::serde::node_from_bytes(&mut allocator, spend.solution.as_slice()).unwrap();
    let error = clvmr::run_program(
        &mut allocator,
        &clvmr::ChiaDialect::new(clvmr::chia_dialect::MEMPOOL_MODE),
        puzzle,
        solution,
        super::super::wallet::MAX_POLICY_VM_COST,
    )
    .expect_err("primary VM must reject");
    let failure = chialisp_oxide_vm::classify_failure(&error);
    assert!(
        failure.is_program_rejection(),
        "resource/host failures are inconclusive"
    );
    failure.code()
}

fn exact_single_recipient_hint(spend: &CoinSpend, expected: [u8; 32]) {
    let mut ctx = codec::context();
    let root = node_from_bytes(&mut ctx, spend.solution.as_slice()).unwrap();
    let cat = CatSolution::<NodePtr>::from_clvm(&*ctx, root).unwrap();
    let standard = chia_puzzle_types::standard::StandardSolution::<NodePtr, NodePtr>::from_clvm(
        &ctx,
        cat.inner_puzzle_solution,
    )
    .unwrap();
    let clvmr_sdk::SExp::Pair(op, body) = ctx.sexp(standard.delegated_puzzle) else {
        panic!("expected delegated quote")
    };
    assert_eq!(ctx.atom(op).as_ref(), &[1]);
    let conditions = Conditions::from_clvm(&*ctx, body).unwrap();
    let creates = conditions
        .into_vec()
        .into_iter()
        .filter_map(|c| c.into_create_coin())
        .collect::<Vec<_>>();
    assert_eq!(creates.len(), 1);
    let Memos::Some(memos) = creates[0].memos else {
        panic!("required first recipient hint")
    };
    let decoded = Vec::<chia_protocol::Bytes32>::from_clvm(&*ctx, memos).unwrap();
    assert_eq!(
        decoded,
        vec![expected.into()],
        "exact one32byte-atom memo, not generic u8 list"
    );
}

#[test]
fn recorded_three_lane_issuance_transfer_and_return_preserve_exact_asset_hint_lineage_and_xch_change()
 {
    for mode in modes() {
        let a = authority(mode);
        let plan = mint(&a, 0x62);
        let (owner, key) = identity(OWNER_SEED);
        let (recipient, recipient_key) = identity(RECIPIENT_SEED);
        assert_eq!(
            plan.eve_coin().parent_coin_info,
            plan.funding_plan().input_coin().coin_id()
        );
        assert_eq!(plan.expected_post_eve_cat().parent_coin(), plan.eve_coin());
        assert_eq!(
            plan.expected_post_eve_cat()
                .lineage()
                .parent_parent_coin_info,
            plan.eve_coin().parent_coin_info
        );
        let mut sim = Simulator::new();
        fund(&mut sim, &plan);
        let issued = sign(plan.issuance_plan(), &key);
        cost(mode, "issue", issued.costs());
        exact_single_recipient_hint(&issued.bundle().coin_spends[0], owner.wallet_puzzle_hash());
        accepted(&mut sim, issued.bundle().clone(), "issue_post_eve_cat");
        assert_eq!(
            sim.children(plan.eve_coin().coin_id())[0].coin,
            issued.normal_cat().coin()
        );
        let transfer = CatTransferPlan::new(
            &a,
            issued.normal_cat(),
            owner,
            recipient.wallet_puzzle_hash(),
            issued.fee_change(),
            DEFAULT_CAT_FEE,
            0,
            &network(),
        )
        .unwrap();
        let transferred = sign(transfer.spend_plan(), &key);
        cost(mode, "transfer", transferred.costs());
        exact_single_recipient_hint(
            &transferred.bundle().coin_spends[0],
            recipient.wallet_puzzle_hash(),
        );
        accepted(
            &mut sim,
            transferred.bundle().clone(),
            "transfer_whole_1000",
        );
        assert_eq!(transferred.asset_id(), issued.asset_id());
        assert_eq!(transferred.normal_cat().coin().amount, CAT_AMOUNT);
        let recipient_fee = Coin::new(
            [0x72; 32].into(),
            recipient.wallet_puzzle_hash().into(),
            1_000_000,
        );
        sim.insert_coin(recipient_fee);
        let back = CatTransferPlan::new(
            &a,
            transferred.normal_cat(),
            recipient,
            owner.wallet_puzzle_hash(),
            recipient_fee,
            DEFAULT_CAT_FEE,
            0,
            &network(),
        )
        .unwrap();
        let returned = sign(back.spend_plan(), &recipient_key);
        cost(mode, "return", returned.costs());
        exact_single_recipient_hint(
            &returned.bundle().coin_spends[0],
            owner.wallet_puzzle_hash(),
        );
        accepted(&mut sim, returned.bundle().clone(), "return_whole_1000");
        assert_eq!(returned.asset_id(), issued.asset_id());
        assert_eq!(
            returned.normal_cat().inner_puzzle_hash(),
            owner.wallet_puzzle_hash()
        );
        assert_eq!(
            returned.fee_change().puzzle_hash,
            recipient.wallet_puzzle_hash().into()
        );
    }
}
#[test]
fn empty_wrong_key_wrong_domain_partial_and_consumed_bundles_have_exact_codes_and_full_state_equality()
 {
    for mode in modes() {
        let a = authority(mode);
        let plan = mint(&a, 0x63);
        let (_, key) = identity(OWNER_SEED);
        let (_, wrong) = identity(WRONG_SEED);
        let mut sim = Simulator::new();
        fund(&mut sim, &plan);
        let p = plan.issuance_plan();
        rejected(
            &mut sim,
            SpendBundle::new(p.spends.clone(), Signature::default()),
            ErrorCode::BadAggregateSignature,
            "empty_bls",
        );
        let mut wrong_signature = Signature::default();
        let mut wrong_domain = Signature::default();
        for r in p.required_bls() {
            wrong_signature += &chia_bls::sign(&wrong, r.message());
            let mut changed = r.clone();
            changed.domain_string =
                Some(chia_sdk_types::MAINNET_CONSTANTS.agg_sig_me_additional_data);
            wrong_domain += &chia_bls::sign(&key, changed.message());
        }
        rejected(
            &mut sim,
            SpendBundle::new(p.spends.clone(), wrong_signature),
            ErrorCode::BadAggregateSignature,
            "nonempty_wrong_key_bls",
        );
        rejected(
            &mut sim,
            SpendBundle::new(p.spends.clone(), wrong_domain),
            ErrorCode::BadAggregateSignature,
            "wrong_network_domain_bls",
        );
        let signed = sign(p, &key);
        let fee_requirement = &p.requirements[1];
        let fee_only_signature = chia_bls::sign(&key, fee_requirement.message());
        rejected(
            &mut sim,
            SpendBundle::new(vec![p.spends[1].clone()], fee_only_signature),
            ErrorCode::AssertConcurrentSpendFailed,
            "fee_input_without_cat",
        );
        // Change only the canonical standard fee reveal to another public key:
        // execution succeeds, then consensus's reveal/coin hash check is exact.
        let mut wrong_reveal = p.spends.clone();
        let mut ctx = codec::context();
        use chia_sdk_driver::Layer;
        let node = chia_sdk_driver::StandardLayer::new(wrong.public_key())
            .construct_puzzle(&mut ctx)
            .unwrap();
        wrong_reveal[1].puzzle_reveal = ctx.serialize(&node).unwrap();
        rejected(
            &mut sim,
            SpendBundle::new(wrong_reveal, signed.bundle().aggregated_signature.clone()),
            ErrorCode::WrongPuzzleHash,
            "fee_reveal_wrong_coin_hash",
        );
        accepted(
            &mut sim,
            signed.bundle().clone(),
            "valid_after_signature_controls",
        );
        rejected(
            &mut sim,
            signed.bundle().clone(),
            ErrorCode::DoubleSpend,
            "consumed_eve_replay",
        );
    }
}
#[test]
fn public_persisted_bytes_cannot_change_any_spend_or_signature_and_hashes_do_not_admit_them() {
    let a = authority(FixtureMode::RustOn);
    let plan = mint(&a, 0x64);
    let (_, key) = identity(OWNER_SEED);
    let p = plan.issuance_plan();
    let signed = sign(p, &key);
    for value in [
        vec![],
        vec![0; MAX_CAT_BUNDLE_BYTES + 1],
        signed.bundle_bytes()[..signed.bundle_bytes().len() - 1].to_vec(),
        [signed.bundle_bytes(), &[0]].concat(),
        {
            let mut b = signed.bundle_bytes().to_vec();
            b[12] ^= 1;
            b
        },
    ] {
        assert!(matches!(
            p.verify_signed_bundle(&value),
            Err(CatError::ChangedBundle)
        ));
    }
    let mut signature = signed.bundle_bytes().to_vec();
    let start = signature.len() - 96;
    signature[start..].fill(0xff);
    assert!(matches!(
        p.verify_signed_bundle(&signature),
        Err(CatError::Signature)
    ));
    assert!(matches!(
        p.verify_signed_bundle(p.unsigned_bundle_bytes()),
        Err(CatError::Consensus(ErrorCode::BadAggregateSignature))
    ));
    let mut changed = p.input_coins();
    changed[1].amount -= 1;
    assert!(matches!(
        p.require_requeried_inputs(&changed, &network()),
        Err(CatError::Coin)
    ));
}
#[test]
fn fixed_plans_refuse_wrong_recipient_input_amount_fee_asset_or_lineage_before_any_signer() {
    let a = authority(FixtureMode::RustOn);
    let (owner, _) = identity(OWNER_SEED);
    let g = genesis(&owner, 0x65);
    assert!(matches!(
        CatFees::new(MAX_CAT_FEE + 1, 0),
        Err(CatError::Fee)
    ));
    assert!(matches!(
        CatFees::new(0, MAX_CAT_FEE + 1),
        Err(CatError::Fee)
    ));
    assert!(matches!(
        CatMintPlan::new(
            &a,
            owner,
            g,
            [0; 32],
            CatFees::new(0, 0).unwrap(),
            0,
            &network()
        ),
        Err(CatError::Identity)
    ));
    assert!(matches!(
        CatMintPlan::new(
            &a,
            owner,
            Coin::new(g.parent_coin_info, g.puzzle_hash, 999),
            owner.wallet_puzzle_hash(),
            CatFees::new(0, 0).unwrap(),
            0,
            &network()
        ),
        Err(CatError::Amount)
    ));
    let plan = mint(&a, 0x65);
    let mut wrong = plan.expected_post_eve_cat().clone();
    wrong.module_hash[0] ^= 1;
    assert!(matches!(
        CatTransferPlan::new(
            &a,
            &wrong,
            owner,
            owner.wallet_puzzle_hash(),
            plan.funding_plan().change_coin(),
            0,
            0,
            &network()
        ),
        Err(CatError::Asset)
    ));
    let mut wrong = plan.expected_post_eve_cat().clone();
    wrong
        .cat
        .lineage_proof
        .as_mut()
        .unwrap()
        .parent_parent_coin_info = [9; 32].into();
    assert!(matches!(
        CatTransferPlan::new(
            &a,
            &wrong,
            owner,
            owner.wallet_puzzle_hash(),
            plan.funding_plan().change_coin(),
            0,
            0,
            &network()
        ),
        Err(CatError::Lineage)
    ));
    assert!(Testnet11Network::from_provider_labels("mainnet", "xch").is_err());
}

// Construct independent adversarial wire transactions. These controls deliberately
// bypass the sealed plan constructor, fund each control's own Eve reveal, and
// never count a local signer/builder refusal as a consensus observation.
fn adversarial_eve(authority: &CatTailAuthority, kind: &str) -> (Simulator, Vec<CoinSpend>) {
    let (owner, _) = identity(OWNER_SEED);
    let g = genesis(
        &owner,
        match kind {
            "genesis" => 0x73,
            "asset" => 0x74,
            _ => 0x75,
        },
    );
    let genesis_id = if kind == "genesis" {
        [0x81; 32]
    } else {
        g.coin_id().to_bytes()
    };
    let (tail, tail_hash) = codec::tail(authority, genesis_id).unwrap();
    let asset = if kind == "asset" {
        [0x82; 32]
    } else {
        tail_hash
    };
    let info = CatInfo::new(asset.into(), None, owner.wallet_puzzle_hash().into());
    let funding = super::super::wallet::FundingPlan::new(
        owner,
        g,
        info.puzzle_hash().to_bytes(),
        DEFAULT_CAT_FEE,
        0,
        &network(),
    )
    .unwrap();
    let mut sim = Simulator::new();
    sim.insert_coin(g);
    let signed = funding.sign_fixture(OWNER_SEED).unwrap();
    accepted(&mut sim, signed.bundle().clone(), "fund_mutant_own_eve");
    let eve = Cat::new(funding.payment_coin(), None, info);
    let mut ctx = codec::context();
    let selected = if kind == "malformed" {
        authority.module_bytes()
    } else {
        &tail
    };
    let tail_node = node_from_bytes(&mut ctx, selected).unwrap();
    let hint = ctx.hint(owner.wallet_puzzle_hash().into()).unwrap();
    let inner = codec::inner_spend(
        &mut ctx,
        &owner,
        Conditions::new()
            .with(CreateCoin::new(
                owner.wallet_puzzle_hash().into(),
                CAT_AMOUNT,
                hint,
            ))
            .with(RunCatTail::new(tail_node, NodePtr::NIL)),
    )
    .unwrap();
    Cat::spend_all(&mut ctx, &[CatSpend::new(eve, inner)]).unwrap();
    codec::sponsor(
        &mut ctx,
        &owner,
        funding.change_coin(),
        DEFAULT_CAT_FEE,
        eve.coin.coin_id(),
    )
    .unwrap();
    (sim, ctx.take())
}
#[test]
fn own_funded_wrong_genesis_wrong_asset_and_malformed_tail_have_primary_raise_and_sdk_runtime_code()
{
    for mode in modes() {
        for kind in ["genesis", "asset", "malformed"] {
            let a = authority(mode);
            let (mut sim, spends) = adversarial_eve(&a, kind);
            assert_eq!(
                primary_code(&spends[0]),
                chialisp_oxide_vm::FailureCode::Raise
            );
            rejected(
                &mut sim,
                SpendBundle::new(spends, Signature::default()),
                ErrorCode::GeneratorRuntimeError,
                kind,
            );
        }
    }
}
fn mutate_solution(spend: &CoinSpend, edit: impl FnOnce(&mut CatSolution<NodePtr>)) -> CoinSpend {
    let mut ctx = codec::context();
    let ptr = node_from_bytes(&mut ctx, spend.solution.as_slice()).unwrap();
    let mut solution = CatSolution::<NodePtr>::from_clvm(&*ctx, ptr).unwrap();
    edit(&mut solution);
    let ptr = solution.to_clvm(&mut *ctx).unwrap();
    let mut result = spend.clone();
    result.solution = node_to_bytes(&ctx, ptr).unwrap().into();
    result
}
#[test]
fn actual_lineage_and_ring_mutations_and_imbalance_are_rejected_without_state_changes() {
    for mode in modes() {
        let a = authority(mode);
        let plan = mint(&a, 0x76);
        let (owner, key) = identity(OWNER_SEED);
        let (recipient, _) = identity(RECIPIENT_SEED);
        let mut sim = Simulator::new();
        fund(&mut sim, &plan);
        let issued = sign(plan.issuance_plan(), &key);
        accepted(&mut sim, issued.bundle().clone(), "setup_normal_cat");
        let transfer = CatTransferPlan::new(
            &a,
            issued.normal_cat(),
            owner,
            recipient.wallet_puzzle_hash(),
            issued.fee_change(),
            DEFAULT_CAT_FEE,
            0,
            &network(),
        )
        .unwrap();
        let p = transfer.spend_plan();
        let signed = sign(p, &key);
        let mut lineage = p.spends.clone();
        lineage[0] = mutate_solution(&lineage[0], |s| {
            s.lineage_proof.as_mut().unwrap().parent_parent_coin_info = [0x91; 32].into()
        });
        assert_eq!(
            primary_code(&lineage[0]),
            chialisp_oxide_vm::FailureCode::Raise
        );
        rejected(
            &mut sim,
            SpendBundle::new(lineage, signed.bundle().aggregated_signature.clone()),
            ErrorCode::GeneratorRuntimeError,
            "wrong_lineage",
        );
        let mut ring = p.spends.clone();
        ring[0] = mutate_solution(&ring[0], |s| {
            s.next_coin_proof.parent_coin_info = [0x92; 32].into()
        });
        rejected(
            &mut sim,
            SpendBundle::new(ring, signed.bundle().aggregated_signature.clone()),
            ErrorCode::AssertCoinAnnouncementFailed,
            "wrong_ring_successor",
        );
        let mut ctx = codec::context();
        let hint = ctx.hint(recipient.wallet_puzzle_hash().into()).unwrap();
        let inner = codec::inner_spend(
            &mut ctx,
            &owner,
            Conditions::new().with(CreateCoin::new(
                recipient.wallet_puzzle_hash().into(),
                CAT_AMOUNT - 1,
                hint,
            )),
        )
        .unwrap();
        Cat::spend_all(&mut ctx, &[CatSpend::new(issued.normal_cat().cat, inner)]).unwrap();
        codec::sponsor(
            &mut ctx,
            &owner,
            issued.fee_change(),
            DEFAULT_CAT_FEE,
            issued.normal_cat().coin().coin_id(),
        )
        .unwrap();
        let debt = ctx.take();
        // The canonical source enforces ring balance through global coin
        // announcements, not a direct VM raise. Its raw VM returns conditions.
        // The existing independent corpus control specifies this exact code.
        let mut vm = clvmr::Allocator::new_limited(MAX_CODEC_HEAP);
        let puz = clvmr::serde::node_from_bytes(&mut vm, debt[0].puzzle_reveal.as_slice()).unwrap();
        let sol = clvmr::serde::node_from_bytes(&mut vm, debt[0].solution.as_slice()).unwrap();
        clvmr::run_program(
            &mut vm,
            &clvmr::ChiaDialect::new(clvmr::chia_dialect::MEMPOOL_MODE),
            puz,
            sol,
            super::super::wallet::MAX_POLICY_VM_COST,
        )
        .unwrap();
        let debt_signature =
            chia_sdk_test::sign_transaction(&debt, std::slice::from_ref(&key)).unwrap();
        rejected(
            &mut sim,
            SpendBundle::new(debt, debt_signature),
            ErrorCode::AssertCoinAnnouncementFailed,
            "nonzero_cat_debt",
        );
        accepted(
            &mut sim,
            signed.bundle().clone(),
            "valid_transfer_after_controls",
        );
        rejected(
            &mut sim,
            signed.bundle().clone(),
            ErrorCode::DoubleSpend,
            "normal_cat_consumed_replay",
        );
    }
}
#[test]
fn actual_native_paired_cargo_tail_37_declared_vectors_are_separate_from_recorded_compiled_artifacts()
 {
    use bumpalo::Bump;
    use chialisp_oxide::Tree;
    fn borrowed<'a>(
        a: &clvmr::Allocator,
        n: clvmr::NodePtr,
        arena: &'a Bump,
        depth: usize,
        budget: &mut usize,
    ) -> &'a Tree<'a> {
        assert!(
            depth <= 64 && *budget > 0,
            "host codec preflight outside policy adapter"
        );
        *budget -= 1;
        let tree = match a.sexp(n) {
            clvmr::SExp::Atom => Tree::Atom(arena.alloc_slice_copy(a.atom(n).as_ref())),
            clvmr::SExp::Pair(f, r) => Tree::Pair(
                borrowed(a, f, arena, depth + 1, budget),
                borrowed(a, r, arena, depth + 1, budget),
            ),
        };
        arena.alloc(tree)
    }
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../experiments/cat-genesis-tail/vectors.json"
    ))
    .unwrap();
    let rows = vectors["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 37);
    for row in rows {
        let raw = hex::decode(row["environment_hex"].as_str().unwrap()).unwrap();
        assert!(raw.len() <= 32768);
        let mut allocator = clvmr::Allocator::new_limited(MAX_CODEC_HEAP);
        let node = clvmr::serde::node_from_bytes(&mut allocator, &raw).unwrap();
        let arena = Bump::new();
        let tree = borrowed(&allocator, node, &arena, 0, &mut 1024);
        let observed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chialisp_oxide_puzzles::cat_genesis_by_coin_id::puzzle(tree)
        }));
        if row["expected"]["status"] == "accepted" {
            assert_eq!(observed.unwrap(), Tree::Atom(&[]));
        } else {
            assert!(observed.is_err());
        }
    }
}

#[test]
fn signer_recheck_rejects_internal_cost_message_and_projection_corruption() {
    // Constructed in-memory controls, not source mutations or ledger attempts.
    let authority = authority(FixtureMode::RustOn);
    let mint = mint(&authority, 0x95);
    let base = mint.issuance_plan();
    let copy = || {
        CatSpendPlan::new(
            base.owner,
            base.spends.clone(),
            base.expected.clone(),
            base.fee,
            base.costs.height,
            base.costs.max_cost,
            base.source_identity,
            base.asset,
            base.child.clone(),
        )
        .unwrap()
    };
    let mut changed = copy();
    changed.costs.max_cost += 1;
    assert!(matches!(
        changed.recheck_for_signing(&changed.input_coins(), 0, &network()),
        Err(CatError::Budget)
    ));
    let mut changed = copy();
    changed.requirements[0].domain_string =
        Some(chia_sdk_types::MAINNET_CONSTANTS.agg_sig_me_additional_data);
    assert!(matches!(
        changed.recheck_for_signing(&changed.input_coins(), 0, &network()),
        Err(CatError::Conditions)
    ));
    let mut changed = copy();
    changed.costs.total += 1;
    assert!(matches!(
        changed.recheck_for_signing(&changed.input_coins(), 0, &network()),
        Err(CatError::Conditions)
    ));
    let mut changed = copy();
    changed.unsigned[0] ^= 1;
    assert!(matches!(
        changed.recheck_for_signing(&changed.input_coins(), 0, &network()),
        Err(CatError::Conditions)
    ));
    let mut changed = copy();
    changed.expected[0][0].hint = None;
    assert!(matches!(
        changed.recheck_for_signing(&changed.input_coins(), 0, &network()),
        Err(CatError::Conditions)
    ));
}
