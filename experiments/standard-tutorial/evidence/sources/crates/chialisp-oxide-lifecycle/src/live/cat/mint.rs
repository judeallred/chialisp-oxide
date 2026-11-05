use super::super::wallet::FundingPlan;
use super::*;
use chia_sdk_driver::{Cat, CatInfo, CatSpend};
use chia_sdk_types::{
    Conditions,
    conditions::{CreateCoin, RunCatTail},
};
use clvmr_sdk::{NodePtr, serde::node_from_bytes};

pub struct CatMintPlan {
    funding: FundingPlan,
    issuance: CatSpendPlan,
    eve: Coin,
    tail: Vec<u8>,
}
impl CatMintPlan {
    /// Only the trusted live parent may combine actual-process source authority
    /// with live coin observations. Production authority requires the opaque
    /// actual-source and SDK gate; fixture/recorded construction is cfg(test) only.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::live) fn new(
        authority: &CatTailAuthority,
        owner: WalletIdentity,
        genesis: Coin,
        recipient_inner: [u8; 32],
        fees: CatFees,
        height: u32,
        network: &Testnet11Network,
    ) -> Result<Self> {
        check_network(network)?;
        if recipient_inner != owner.wallet_puzzle_hash() {
            return Err(CatError::Identity);
        }
        if genesis.puzzle_hash.to_bytes() != owner.wallet_puzzle_hash() {
            return Err(CatError::Coin);
        }
        let available = genesis
            .amount
            .checked_sub(CAT_AMOUNT)
            .and_then(|v| v.checked_sub(fees.funding()))
            .ok_or(CatError::Amount)?;
        if available < fees.issuance() {
            return Err(CatError::Amount);
        }
        let (tail, asset) = codec::tail(authority, genesis.coin_id().to_bytes())?;
        let mut ctx = codec::context();
        let inner = codec::owner_layer(&owner)?;
        use clvm_utils::ToTreeHash;
        if inner.tree_hash().to_bytes() != owner.wallet_puzzle_hash() {
            return Err(CatError::Identity);
        }
        let info = CatInfo::new(asset.into(), None, owner.wallet_puzzle_hash().into());
        let funding = FundingPlan::new(
            owner,
            genesis,
            info.puzzle_hash().to_bytes(),
            fees.funding(),
            height,
            network,
        )
        .map_err(|_| CatError::Coin)?;
        let eve = funding.payment_coin();
        let eve_cat = Cat::new(eve, None, info);
        let tail_node = node_from_bytes(&mut ctx, &tail).map_err(|_| CatError::Module)?;
        let memos = ctx
            .hint(recipient_inner.into())
            .map_err(|_| CatError::Codec)?;
        let conditions = Conditions::new()
            .with(CreateCoin::new(recipient_inner.into(), CAT_AMOUNT, memos))
            .with(RunCatTail::new(tail_node, NodePtr::NIL));
        let spend = codec::inner_spend(&mut ctx, &owner, conditions)?;
        let children = Cat::spend_all(&mut ctx, &[CatSpend::new(eve_cat, spend)])
            .map_err(|_| CatError::Codec)?;
        if children.len() != 1
            || children[0].coin.amount != CAT_AMOUNT
            || children[0].info.p2_puzzle_hash.to_bytes() != recipient_inner
            || children[0].info.asset_id.to_bytes() != asset
            || children[0].lineage_proof.is_none()
        {
            return Err(CatError::Conditions);
        }
        let change = codec::sponsor(
            &mut ctx,
            &owner,
            funding.change_coin(),
            fees.issuance(),
            eve.coin_id(),
        )?;
        let child = NormalCatCoin {
            cat: children[0],
            genesis: genesis.coin_id().to_bytes(),
            module_hash: authority.module_hash(),
            mode: authority.mode(),
            parent: eve,
            parent_inner: owner.wallet_puzzle_hash(),
        };
        child.check(authority, &owner)?;
        let expected = vec![
            vec![ExpectedOutput {
                coin: child.coin(),
                hint: Some(recipient_inner),
            }],
            vec![ExpectedOutput {
                coin: change,
                hint: None,
            }],
        ];
        let issuance = CatSpendPlan::new(
            owner,
            ctx.take(),
            expected,
            fees.issuance(),
            height,
            authority.max_cost(),
            authority.source_identity(),
            asset,
            child,
        )?;
        Ok(Self {
            funding,
            issuance,
            eve,
            tail,
        })
    }
    pub fn funding_plan(&self) -> &FundingPlan {
        &self.funding
    }
    pub fn issuance_plan(&self) -> &CatSpendPlan {
        &self.issuance
    }
    pub fn eve_coin(&self) -> Coin {
        self.eve
    }
    pub fn asset_id(&self) -> [u8; 32] {
        self.issuance.asset_id()
    }
    pub fn tail_reveal(&self) -> &[u8] {
        &self.tail
    }
    pub fn expected_post_eve_cat(&self) -> &NormalCatCoin {
        self.issuance.expected_cat()
    }
}
