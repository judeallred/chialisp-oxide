use super::*;
use chia_sdk_driver::{Cat, CatSpend};
use chia_sdk_types::{Conditions, conditions::CreateCoin};

pub struct CatTransferPlan {
    transaction: CatSpendPlan,
}
impl CatTransferPlan {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::live) fn new(
        authority: &CatTailAuthority,
        owned: &NormalCatCoin,
        owner: WalletIdentity,
        recipient_inner: [u8; 32],
        fee_input: Coin,
        fee: u64,
        height: u32,
        network: &Testnet11Network,
    ) -> Result<Self> {
        check_network(network)?;
        owned.check(authority, &owner)?;
        let mut ctx = codec::context();
        let memos = ctx
            .hint(recipient_inner.into())
            .map_err(|_| CatError::Codec)?;
        let delegated =
            Conditions::new().with(CreateCoin::new(recipient_inner.into(), CAT_AMOUNT, memos));
        let spend = codec::inner_spend(&mut ctx, &owner, delegated)?;
        let children = Cat::spend_all(&mut ctx, &[CatSpend::new(owned.cat, spend)])
            .map_err(|_| CatError::Codec)?;
        if children.len() != 1
            || children[0].coin.amount != CAT_AMOUNT
            || children[0].info.p2_puzzle_hash.to_bytes() != recipient_inner
            || children[0].info.asset_id != owned.cat.info.asset_id
            || children[0].lineage_proof.is_none()
        {
            return Err(CatError::Conditions);
        }
        let change = codec::sponsor(&mut ctx, &owner, fee_input, fee, owned.coin().coin_id())?;
        let child = NormalCatCoin {
            cat: children[0],
            genesis: owned.genesis,
            module_hash: owned.module_hash,
            mode: owned.mode,
            parent: owned.coin(),
            parent_inner: owner.wallet_puzzle_hash(),
        };
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
        let transaction = CatSpendPlan::new(
            owner,
            ctx.take(),
            expected,
            fee,
            height,
            authority.max_cost(),
            authority.source_identity(),
            owned.asset_id(),
            child,
        )?;
        Ok(Self { transaction })
    }
    pub fn spend_plan(&self) -> &CatSpendPlan {
        &self.transaction
    }
    pub fn expected_cat(&self) -> &NormalCatCoin {
        self.transaction.expected_cat()
    }
}
