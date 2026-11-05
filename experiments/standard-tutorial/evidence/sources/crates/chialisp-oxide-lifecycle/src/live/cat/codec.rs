use super::*;
use chia_puzzle_types::cat::GenesisByCoinIdTailArgs;
use chia_sdk_driver::{SpendContext, SpendWithConditions, StandardLayer};
use chia_sdk_types::{
    Conditions,
    conditions::{AssertConcurrentSpend, CreateCoin, ReserveFee},
};
use clvm_utils::{CurriedProgram, tree_hash};
use clvmr_sdk::serde::{node_from_bytes, node_to_bytes};

pub(super) fn context() -> SpendContext {
    let mut ctx = SpendContext::new();
    *ctx = Allocator::new_limited(MAX_CODEC_HEAP);
    ctx
}
pub(super) fn owner_layer(owner: &WalletIdentity) -> Result<StandardLayer> {
    Ok(StandardLayer::new(
        PublicKey::from_bytes(&owner.synthetic_public_key()).map_err(|_| CatError::Identity)?,
    ))
}
pub(super) fn tail(authority: &CatTailAuthority, genesis: [u8; 32]) -> Result<(Vec<u8>, [u8; 32])> {
    if authority.module_bytes().is_empty() || authority.module_bytes().len() > MAX_CAT_MODULE_BYTES
    {
        return Err(CatError::Module);
    }
    let mut ctx = context();
    let module =
        node_from_bytes(&mut ctx, authority.module_bytes()).map_err(|_| CatError::Module)?;
    if tree_hash(&ctx, module).to_bytes() != authority.module_hash() {
        return Err(CatError::Module);
    }
    // ctx.curry(GenesisByCoinIdTailArgs) would silently select the canonical
    // 45-byte TAIL. Explicitly curry the selected actual Rust program instead.
    let curried = ctx
        .alloc(&CurriedProgram {
            program: module,
            args: GenesisByCoinIdTailArgs::new(genesis.into()),
        })
        .map_err(|_| CatError::Codec)?;
    let hash = ctx.tree_hash(curried).to_bytes();
    Ok((
        node_to_bytes(&ctx, curried).map_err(|_| CatError::Codec)?,
        hash,
    ))
}
pub(super) fn sponsor(
    ctx: &mut SpendContext,
    owner: &WalletIdentity,
    coin: Coin,
    fee: u64,
    cat_id: chia_protocol::Bytes32,
) -> Result<Coin> {
    let change = fee_change(owner, coin, fee)?;
    let delegated = Conditions::new()
        .with(CreateCoin::new(
            owner.wallet_puzzle_hash().into(),
            change.amount,
            chia_puzzle_types::Memos::None,
        ))
        .with(ReserveFee::new(fee))
        .with(AssertConcurrentSpend::new(cat_id));
    owner_layer(owner)?
        .spend(ctx, coin, delegated)
        .map_err(|_| CatError::Codec)?;
    Ok(change)
}
pub(super) fn inner_spend(
    ctx: &mut SpendContext,
    owner: &WalletIdentity,
    conditions: Conditions,
) -> Result<chia_sdk_driver::Spend> {
    owner_layer(owner)?
        .spend_with_conditions(ctx, conditions)
        .map_err(|_| CatError::Codec)
}
