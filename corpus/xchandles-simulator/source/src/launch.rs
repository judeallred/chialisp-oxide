//! Deterministic disposable host launch recipe using public SDK primitives.
//! Adapted from Apache-2.0 chia-sdk-driver 0.36.0 launch_drivers.rs.
//! No dependency/RNG patches; this is not an on-chain Rust implementation.
use anyhow::Result;
use chia_bls::SecretKey;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    EveProof, LineageProof, Proof,
    singleton::{SingletonArgs, SingletonSolution},
};
use chia_sdk_driver::*;
use chia_sdk_types::{TESTNET11_CONSTANTS, puzzles::*};
use clvm_traits::clvm_list;
use clvm_utils::ToTreeHash;
use clvmr::NodePtr;

/// Public fixture seed, never a wallet or production secret.
const SECURITY_FIXTURE_SEED: [u8; 32] = [0x77; 32];

pub fn launch(
    ctx: &mut SpendContext,
    offer: &Offer,
    base_price: u64,
    period: u64,
    constants: XchandlesConstants,
    asset: Bytes32,
) -> Result<(
    SecretKey,
    XchandlesRegistry,
    [Slot<XchandlesHandleSlotValue>; 2],
)> {
    let key = SecretKey::from_seed(&SECURITY_FIXTURE_SEED);
    let security =
        create_security_coin_with_pk(ctx, offer.offered_coins().xch[0], key.public_key())?;
    for spend in &offer.spend_bundle().coin_spends {
        ctx.insert(spend.clone());
    }
    let launcher = Launcher::new(security.coin_id(), 1);
    let launcher_coin = launcher.coin();
    let launcher_id = launcher_coin.coin_id();
    let state = XchandlesRegistryState::from(asset.tree_hash().into(), base_price, period);
    let constants = constants.with_launcher_id(launcher_id);
    let info = XchandlesRegistryInfo::new(state, constants);
    let inner_hash: Bytes32 = info.inner_puzzle_hash().into();
    let left = XchandlesHandleSlotValue::initial_left_end();
    let right = XchandlesHandleSlotValue::initial_right_end();
    let nonce = XchandlesSlotNonce::HANDLE.to_u64();
    let eve_inner = eve_singleton_inner_puzzle(
        ctx,
        launcher_id,
        nonce,
        left,
        right,
        NodePtr::NIL,
        inner_hash,
    )?;
    let eve_hash: Bytes32 = ctx.tree_hash(eve_inner).into();
    let (conditions, eve) = launcher.spend(
        ctx,
        eve_hash,
        clvm_list!(asset, base_price, period, state, constants),
    )?;
    let puzzle = ctx.curry(SingletonArgs::new(launcher_id, eve_inner))?;
    let solution = ctx.alloc(&SingletonSolution {
        lineage_proof: Proof::Eve(EveProof {
            parent_parent_coin_info: launcher_coin.parent_coin_info,
            parent_amount: 1,
        }),
        amount: 1,
        inner_solution: NodePtr::NIL,
    })?;
    ctx.spend(eve, Spend::new(puzzle, solution))?;
    let proof = LineageProof {
        parent_parent_coin_info: eve.parent_coin_info,
        parent_inner_puzzle_hash: eve_hash,
        parent_amount: 1,
    };
    let coin = Coin::new(
        eve.coin_id(),
        SingletonArgs::curry_tree_hash(launcher_id, inner_hash.into()).into(),
        1,
    );
    let slots = [
        Slot::new(proof, SlotInfo::from_value(launcher_id, nonce, left)),
        Slot::new(proof, SlotInfo::from_value(launcher_id, nonce, right)),
    ];
    spend_security_coin(
        ctx,
        security,
        conditions.assert_concurrent_spend(eve.coin_id()),
        &key,
        &TESTNET11_CONSTANTS,
    )?;
    Ok((
        key,
        XchandlesRegistry::new(coin, Proof::Lineage(proof), info),
        slots,
    ))
}
