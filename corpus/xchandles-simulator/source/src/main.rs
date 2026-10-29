mod controls;
mod journal;
mod launch;
mod metrics;
// WIP canonical SDK host pilot. No on-chain Rust frontend or network access.
// API recipe adapted from chia-sdk-driver 0.36.0 launch_drivers.rs, Apache-2.0.
use anyhow::{Context, Result, ensure};
use chia_bls::SecretKey;
use chia_consensus::validation_error::ErrorCode;
use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::{EveProof, Proof, nft::NftMetadata, singleton::SingletonStruct};
use chia_sdk_driver::*;
use chia_sdk_test::{Simulator, SimulatorError, sign_transaction};
use chia_sdk_types::{Conditions, Mod, puzzles::*};
use clvm_utils::ToTreeHash;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const BASE_PRICE: u64 = 2_000;
const PERIOD: u64 = 366 * 24 * 60 * 60;
const PAYMENT: u64 = 10_000_000;
const REGISTER_HASH: &str = "150ae295efe60fa049b87cff5c05c0031fa768959ba466b425ef5a58b4a95ee8";

fn coin(c: Coin) -> Value {
    json!({"coin_id":hex::encode(c.coin_id()),"parent_coin_id":hex::encode(c.parent_coin_info),"puzzle_hash":hex::encode(c.puzzle_hash),"amount":c.amount})
}
fn slot_record(slot: &Slot<XchandlesHandleSlotValue>, launcher: Bytes32) -> Result<Value> {
    let creator = Coin::new(
        slot.proof.parent_parent_coin_info,
        chia_puzzle_types::singleton::SingletonArgs::curry_tree_hash(
            launcher,
            slot.proof.parent_inner_puzzle_hash.into(),
        )
        .into(),
        slot.proof.parent_amount,
    );
    ensure!(
        creator.coin_id() == slot.coin.parent_coin_info,
        "exact creator proof vs slot coin parent"
    );
    Ok(
        json!({"coin":coin(slot.coin),"creator_registry_coin":coin(creator),"creator_parent_coin_id":hex::encode(slot.proof.parent_parent_coin_info),"creator_inner_puzzle_hash":hex::encode(slot.proof.parent_inner_puzzle_hash),"handle_hash":hex::encode(slot.info.value.handle_hash),"left_hash":hex::encode(slot.info.value.neighbors.left_value),"right_hash":hex::encode(slot.info.value.neighbors.right_value),"slot_value_tree_hash":hex::encode(slot.info.value_hash),"slot_nonce":slot.info.nonce,"registry_launcher_id":hex::encode(slot.info.launcher_id),"expiration":slot.info.value.expiration,"counter":slot.info.value.counter,"owner_launcher_id":hex::encode(slot.info.value.owner_launcher_id),"resolved_launcher_id":hex::encode(slot.info.value.resolved_launcher_id)}),
    )
}
fn submit(
    sim: &mut Simulator,
    ctx: &mut SpendContext,
    keys: &[SecretKey],
    name: &str,
    rows: &mut Vec<Value>,
) -> Result<()> {
    let spends = ctx.take();
    let signature = sign_transaction(&spends, keys)?;
    let mut components = Vec::new();
    for spend in &spends {
        let mut a = clvmr::Allocator::new_limited(64 * 1024 * 1024);
        let p = clvmr::serde::node_from_bytes(&mut a, spend.puzzle_reveal.as_ref())?;
        let s = clvmr::serde::node_from_bytes(&mut a, spend.solution.as_ref())?;
        let reduction = clvmr::run_program(
            &mut a,
            &clvmr::ChiaDialect::new(clvmr::MEMPOOL_MODE),
            p,
            s,
            500_000_000,
        )?;
        components.push(json!({"coin":coin(spend.coin),"cost":reduction.0,"output_hex":hex::encode(clvmr::serde::node_to_bytes(&a,reduction.1)?)}));
    }
    let input_ids = spends
        .iter()
        .map(|s| hex::encode(s.coin.coin_id()))
        .collect::<Vec<_>>();
    let wire = spends.iter().map(|s| json!({"coin":coin(s.coin),"reveal_hex":hex::encode(s.puzzle_reveal.as_ref()),"solution_hex":hex::encode(s.solution.as_ref())})).collect::<Vec<_>>();
    let bundle = SpendBundle::new(spends, signature.clone());
    let measurement = metrics::measure(&bundle, sim.height())?;
    if name == "register_handle" {
        rows.push(controls::wrong_neighbors(ctx, sim, &bundle)?);
        let before = sim.clone();
        let wrong = chia_bls::sign(
            &SecretKey::from_seed(&[0x91; 32]),
            b"public invalid aggregate fixture",
        );
        let result =
            sim.new_transaction(SpendBundle::new(bundle.coin_spends.clone(), wrong.clone()));
        ensure!(
            matches!(
                result,
                Err(SimulatorError::Validation(ErrorCode::BadAggregateSignature))
            ),
            "actual nonempty invalid aggregate must receive exact consensus code"
        );
        ensure!(
            *sim == before,
            "full simulator equality after invalid signature"
        );
        rows.push(json!({"name":"invalid_signature","status":"consensus_rejected","code":"BadAggregateSignature","full_simulator_equality":true,"input_coin_ids":input_ids,"signature_hex":hex::encode(wrong.to_bytes()),"component_vm":[]}));
    }
    eprintln!(
        "submitting {name}: {} coin spends",
        bundle.coin_spends.len()
    );
    sim.new_transaction(bundle.clone())?;
    rows.push(json!({"name":name,"status":"accepted","input_coin_ids":input_ids,"spends":wire,"signature_hex":hex::encode(signature.to_bytes()),"component_vm":components,"spendbundle_metrics":measurement}));
    if name == "register_handle" {
        let before = sim.clone();
        let result = sim.new_transaction(bundle);
        ensure!(
            matches!(
                result,
                Err(SimulatorError::Validation(ErrorCode::DoubleSpend))
            ),
            "actual consumed bundle replay must be DoubleSpend"
        );
        ensure!(*sim == before, "full simulator equality after replay");
        rows.push(json!({"name":"consumed_registry_replay","status":"consensus_rejected","code":"DoubleSpend","full_simulator_equality":true,"component_vm":[]}));
    }
    Ok(())
}

fn pilot(stop_after: usize, resume: Option<&Value>) -> Result<Value> {
    ensure!(
        XCHANDLES_REGISTER_PUZZLE.len() == 1478
            && hex::encode(XCHANDLES_REGISTER_PUZZLE_HASH.to_bytes()) == REGISTER_HASH,
        "pinned canonical register bytes/hash"
    );
    let published = hex::decode(
        include_str!("../fixtures/register.rue.hex")
            .split_whitespace()
            .collect::<String>(),
    )?;
    ensure!(
        published.as_slice() == XCHANDLES_REGISTER_PUZZLE,
        "exact retained published module artifact bytes"
    );
    let mut ctx = SpendContext::new();
    let mut sim = Simulator::new();
    let mut rows = Vec::new();
    let user = sim.bls(0);
    let user_p2 = StandardLayer::new(user.pk);
    let minter = sim.bls(PAYMENT);
    let minter_p2 = StandardLayer::new(minter.pk);
    let (issue, cats) = Cat::single_issuance(
        &mut ctx,
        minter.coin.coin_id(),
        None,
        PAYMENT,
        Conditions::new().create_coin(minter.puzzle_hash, PAYMENT, chia_puzzle_types::Memos::None),
    )?;
    let mut payment = cats[0];
    minter_p2.spend(&mut ctx, minter.coin, issue)?;
    submit(
        &mut sim,
        &mut ctx,
        std::slice::from_ref(&minter.sk),
        "issue_payment_cat",
        &mut rows,
    )?;

    // Real NFT owner/resolved singleton, funded by a signed standard input.
    let nft_fund = sim.bls(1);
    let metadata = ctx.alloc_hashed(&NftMetadata::default())?;
    let (nft_conditions, mut nft) = Launcher::new(nft_fund.coin.coin_id(), 1)
        .mint_nft(&mut ctx, &NftMint::new(metadata, user.puzzle_hash, 0, None))?;
    StandardLayer::new(nft_fund.pk).spend(&mut ctx, nft_fund.coin, nft_conditions)?;

    // Disposable local price singleton, inner puzzle 1, used only as a launcher ID.
    let price_fund = sim.bls(1);
    let launcher = Launcher::new(price_fund.coin.coin_id(), 1);
    let price_launcher = launcher.coin();
    let price_inner = ctx.alloc(&1)?;
    let price_hash = ctx.tree_hash(price_inner);
    let (price_conditions, price_coin) = launcher.spend(&mut ctx, price_hash.into(), ())?;
    StandardLayer::new(price_fund.pk).spend(&mut ctx, price_fund.coin, price_conditions)?;
    let _price_proof = Proof::Eve(EveProof {
        parent_parent_coin_info: price_launcher.parent_coin_info,
        parent_amount: 1,
    });

    let funding = sim.bls(1);
    let spend = StandardLayer::new(funding.pk).spend_with_conditions(
        &mut ctx,
        Conditions::new().create_coin(
            chia_puzzles::SETTLEMENT_PAYMENT_HASH.into(),
            1,
            chia_puzzle_types::Memos::None,
        ),
    )?;
    let offered = CoinSpend::new(
        funding.coin,
        ctx.serialize(&spend.puzzle)?,
        ctx.serialize(&spend.solution)?,
    );
    let signature = sign_transaction(
        std::slice::from_ref(&offered),
        std::slice::from_ref(&funding.sk),
    )?;
    let offer = Offer::from_spend_bundle(&mut ctx, &SpendBundle::new(vec![offered], signature))?;
    let constants = XchandlesConstants {
        launcher_id: [1; 32].into(),
        precommit_payout_puzzle_hash: [8; 32].into(),
        relative_block_height: 1,
        price_singleton_launcher_id: price_launcher.coin_id(),
    };
    let (security, mut registry, sentinels) = launch::launch(
        &mut ctx,
        &offer,
        BASE_PRICE,
        PERIOD,
        constants,
        payment.info.asset_id,
    )?;
    submit(
        &mut sim,
        &mut ctx,
        &[nft_fund.sk, price_fund.sk, funding.sk, security],
        "launch_registry_price_and_nft",
        &mut rows,
    )?;
    ensure!(
        sim.coin_state(price_coin.coin_id()).is_some()
            && sim.coin_state(nft.coin.coin_id()).is_some(),
        "actual launched price/NFT children"
    );
    let mut slots = sentinels.to_vec();
    let initial_state = hex::encode(registry.info.state.tree_hash().to_bytes());
    let mut steps = Vec::new();
    let mut remaining = PAYMENT;
    let batches: &[&[&str]] = &[
        &["pilotalpha"],
        &["pilotbeta", "pilotgamma"],
        &["pilotdelta", "pilotepsilon", "pilotzeta"],
    ];
    for batch in batches {
        let mut prepared = Vec::new();
        for handle in *batch {
            let handle = handle.to_string();
            let hash: Bytes32 = handle.tree_hash().into();
            let pricing = XchandlesFactorPricingPuzzleArgs {
                base_price: BASE_PRICE,
                registration_period: PERIOD,
            };
            let pricing_solution = XchandlesPricingSolution {
                buy_time: sim.next_timestamp() + 100,
                current_expiration: 0,
                handle: handle.clone(),
                num_periods: 1,
            };
            let amount = XchandlesFactorPricingPuzzleArgs::get_price(BASE_PRICE, &handle, 1);
            let value = XchandlesPrecommitValue::for_normal_registration(
                payment.info.asset_id.tree_hash(),
                pricing.curry_tree_hash(),
                &pricing_solution.tree_hash(),
                handle.clone(),
                Bytes32::default(),
                nft.info.launcher_id,
                nft.info.launcher_id,
            );
            let refund = ctx.alloc(&1)?;
            let refund_hash = ctx.tree_hash(refund).into();
            let precommit = PrecommitCoin::new(
                &mut ctx,
                payment.coin.coin_id(),
                payment.child_lineage_proof(),
                payment.info.asset_id,
                SingletonStruct::new(registry.info.constants.launcher_id)
                    .tree_hash()
                    .into(),
                constants.relative_block_height,
                constants.precommit_payout_puzzle_hash,
                refund_hash,
                value,
                amount,
            )?;
            let spend = minter_p2.spend_with_conditions(
                &mut ctx,
                Conditions::new()
                    .create_coin(
                        precommit.inner_puzzle_hash,
                        amount,
                        chia_puzzle_types::Memos::None,
                    )
                    .create_coin(
                        minter.puzzle_hash,
                        remaining - amount,
                        chia_puzzle_types::Memos::None,
                    ),
            )?;
            Cat::spend_all(
                &mut ctx,
                &[CatSpend {
                    cat: payment,
                    spend,
                    hidden: false,
                }],
            )?;
            remaining -= amount;
            payment = payment.child(minter.puzzle_hash, remaining);
            submit(
                &mut sim,
                &mut ctx,
                std::slice::from_ref(&minter.sk),
                "create_precommit",
                &mut rows,
            )?;
            prepared.push((handle, hash, pricing, pricing_solution, precommit));
        }
        slots.sort_by_key(|s| s.info.value.handle_hash);
        let previous = registry.coin;
        let mut secure_all = Conditions::new();
        let mut owner_all = Conditions::new();
        let mut neighbors = Vec::new();
        for (handle, hash, pricing, pricing_solution, precommit) in prepared {
            let left = slots
                .iter()
                .rev()
                .find(|s| s.info.value.handle_hash < hash)
                .unwrap()
                .clone();
            let right = slots
                .iter()
                .find(|s| s.info.value.handle_hash > hash)
                .unwrap()
                .clone();
            let (actual_left, actual_right) =
                registry.actual_neigbors(hash, left.clone(), right.clone());
            neighbors.push(json!({"handle":handle,"left":slot_record(&actual_left,registry.info.constants.launcher_id)?,"right":slot_record(&actual_right,registry.info.constants.launcher_id)?}));
            let pricing_reveal = ctx.curry(pricing)?;
            let (secure, owner, resolved) = registry
                .new_action::<XchandlesRegisterAction>()
                .spend_with_pricing(
                    &mut ctx,
                    &mut registry,
                    left,
                    right,
                    &precommit,
                    pricing_reveal,
                    pricing_solution,
                    nft.info.inner_puzzle_hash().into(),
                    nft.info.inner_puzzle_hash().into(),
                )?;
            ensure!(resolved.is_none(), "same owner/resolved NFT approval");
            secure_all = secure_all.extend(secure);
            owner_all = owner_all.extend(owner);
        }
        let guard = sim.bls(1);
        StandardLayer::new(guard.pk).spend(&mut ctx, guard.coin, secure_all)?;
        nft = nft.transfer(&mut ctx, &user_p2, user.puzzle_hash, owner_all)?;
        let new_slots = registry
            .pending_spend
            .created_handle_slots
            .iter()
            .map(|v| registry.created_handle_slot_value_to_slot(*v))
            .collect::<Vec<_>>();
        registry = registry.finish_spend(&mut ctx)?.0;
        sim.pass_time(100);
        submit(
            &mut sim,
            &mut ctx,
            &[user.sk.clone(), guard.sk],
            "register_handle",
            &mut rows,
        )?;
        ensure!(
            sim.coin_state(previous.coin_id())
                .unwrap()
                .spent_height
                .is_some(),
            "previous registry consumed"
        );
        ensure!(
            sim.coin_state(registry.coin.coin_id())
                .unwrap()
                .spent_height
                .is_none(),
            "exact current registry coin"
        );
        ensure!(
            hex::encode(registry.info.state.tree_hash().to_bytes()) == initial_state,
            "canonical state remains constant"
        );
        let mut expected_children = new_slots
            .iter()
            .map(|s| s.coin.coin_id())
            .collect::<Vec<_>>();
        expected_children.push(registry.coin.coin_id());
        expected_children.sort();
        let mut actual_children = sim
            .children(previous.coin_id())
            .iter()
            .map(|s| s.coin.coin_id())
            .collect::<Vec<_>>();
        actual_children.sort();
        ensure!(
            expected_children == actual_children,
            "complete actual registry child set including ephemeral slots"
        );
        slots.retain(|s| {
            sim.coin_state(s.coin.coin_id())
                .unwrap()
                .spent_height
                .is_none()
        });
        slots.extend(
            new_slots
                .iter()
                .filter(|s| {
                    sim.coin_state(s.coin.coin_id())
                        .unwrap()
                        .spent_height
                        .is_none()
                })
                .cloned(),
        );
        slots.sort_by_key(|s| s.info.value.handle_hash);
        for pair in slots.windows(2) {
            ensure!(
                pair[0].info.value.neighbors.right_value == pair[1].info.value.handle_hash
                    && pair[1].info.value.neighbors.left_value == pair[0].info.value.handle_hash,
                "complete surviving linked-neighbor order"
            );
        }
        let mut children = sim.children(previous.coin_id());
        children.sort_by_key(|c| c.coin.coin_id());
        steps.push(json!({"handles":batch,"batch_size":batch.len(),"previous_registry":coin(previous),"current_registry":coin(registry.coin),"registry_state_tree_hash":initial_state,"registry_state":{"cat_maker_puzzle_hash":hex::encode(registry.info.state.cat_maker_puzzle_hash),"pricing_puzzle_hash":hex::encode(registry.info.state.pricing_puzzle_hash),"expired_handle_pricing_puzzle_hash":hex::encode(registry.info.state.expired_handle_pricing_puzzle_hash)},"neighbors":neighbors,"complete_registry_children":children.iter().map(|c|json!({"coin":coin(c.coin),"created_height":c.created_height,"spent_height":c.spent_height})).collect::<Vec<_>>(),"surviving_slots":slots.iter().map(|s|slot_record(s,registry.info.constants.launcher_id)).collect::<Result<Vec<_>>>()?,"owner_resolved_nft":{"coin":coin(nft.coin),"launcher_id":hex::encode(nft.info.launcher_id),"inner_puzzle_hash":hex::encode(nft.info.inner_puzzle_hash().to_bytes())}}));
        if let Some(prefix) = resume
            && prefix["registration_steps"].as_array().unwrap().len() == steps.len()
        {
            ensure!(
                *prefix == report(&rows, &steps, true),
                "resumed full checkpoint prefix differs from actual fresh SDK recipe"
            );
        }
        if steps.len() == stop_after && stop_after < 3 {
            return Ok(report(&rows, &steps, true));
        }
    }
    Ok(report(&rows, &steps, false))
}
fn report(rows: &[Value], steps: &[Value], paused: bool) -> Value {
    let component_runs = rows
        .iter()
        .map(|r| r["component_vm"].as_array().unwrap().len())
        .sum::<usize>();
    json!({"protocol":"chialisp-oxide-xchandles-canonical-host-pilot/v1","status":if paused {"paused_after_validated_step"}else{"executed_local_simulator"},"recipe_fingerprint":journal::fingerprint(),"batch_sizes":[1,2,3],"canonical_modules":metrics::canonical_modules().expect("fixed module identities"),"sdk":"chia-sdk-driver/test 0.36.0","consensus":"chia-consensus 0.36.1","component_vm":"clvmr 0.16.2","network":"TESTNET11","register_module_bytes":1478,"register_module_tree_hash":REGISTER_HASH,"register_module_blob_sha256":hex::encode(Sha256::digest(XCHANDLES_REGISTER_PUZZLE)),"registration_steps":steps,"sdk_transaction_attempts":rows.len(),"component_vm_runs":component_runs,"transactions":rows,"scope":"Canonical host-only fixed local recipe, complete plain-generator condition/cost measurements separate from actual SDK BLS/ledger admission. Resume reexecutes and compares complete public prefix. No Rust frontend port, live/mainnet replay, autonomous remaining-work commitment, source authentication or compiler preservation."})
}
fn main() -> Result<()> {
    journal::current_sources()?;
    let mut args = std::env::args_os().skip(1);
    let mut stop = 3usize;
    let mut resume = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--stop-after") => {
                stop = args
                    .next()
                    .and_then(|s| s.into_string().ok())
                    .context("stop count")?
                    .parse()?;
                ensure!((1..=3).contains(&stop), "fixed one-to-three step bound");
            }
            Some("--resume") => {
                resume = Some(journal::read(std::path::Path::new(
                    &args.next().context("checkpoint path")?,
                ))?);
            }
            _ => anyhow::bail!(
                "usage: host-pilot [--stop-after 1|2|3] [--resume PUBLIC_CHECKPOINT.json]"
            ),
        }
    }
    if let Some(prefix) = &resume {
        ensure!(
            stop > prefix["registration_steps"].as_array().unwrap().len(),
            "resume must advance beyond retained checkpoint"
        );
    }
    let output = pilot(stop, resume.as_ref())?;
    journal::current_sources()?;
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
