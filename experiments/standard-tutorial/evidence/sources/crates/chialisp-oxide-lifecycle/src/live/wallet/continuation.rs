//! Purpose-specific signing plans. The old payment/CAT ceilings are unchanged.
use super::*;
use crate::live::continuation::{ContinuationRecipe, MAX_CONTINUATION_COST};

pub struct ContinuationFundingPlan(TransactionPlan);
impl ContinuationFundingPlan {
    pub fn new(
        identity: WalletIdentity,
        input: Coin,
        recipe: &ContinuationRecipe,
        fee: u64,
        height: u32,
        network: &Testnet11Network,
    ) -> Result<Self> {
        network.check()?;
        recipe
            .require_current()
            .map_err(|_| WalletError::ProgramBuild)?;
        let principal = recipe.funding_amount();
        let destination = recipe.funding_puzzle_hash();
        let change_amount = checked_change(&identity, input, principal, fee)?;
        if destination == identity.puzzle_hash && change_amount == principal {
            return Err(WalletError::DuplicateOutput);
        }
        let payment = Coin::new(input.coin_id(), destination.into(), principal);
        let change = Coin::new(input.coin_id(), identity.puzzle_hash.into(), change_amount);
        let delegated = Conditions::new()
            .with(CreateCoin::new(
                destination.into(),
                principal,
                Memos::<NodePtr>::None,
            ))
            .with(CreateCoin::new(
                identity.puzzle_hash.into(),
                change_amount,
                Memos::<NodePtr>::None,
            ))
            .with(ReserveFee::new(fee));
        let (spend, requirement) = standard_spend(&identity, input, delegated)?;
        Ok(Self(TransactionPlan::new(
            identity,
            vec![spend],
            requirement,
            vec![vec![payment, change]],
            payment,
            change,
            fee,
            height,
        )?))
    }
    pub fn verify_signed_bundle(&self, bytes: &[u8]) -> Result<VerifiedTransaction> {
        self.0.verify(bytes)
    }
    pub fn sign_hidden_tty(
        &self,
        input: Coin,
        network: &Testnet11Network,
    ) -> Result<VerifiedTransaction> {
        self.0.require_inputs(&[input], network)?;
        self.0.sign(&tty::read_master_secret()?)
    }
    #[cfg(test)]
    pub(crate) fn sign_fixture(&self, seed: [u8; 32]) -> Result<VerifiedTransaction> {
        self.0.sign(&MasterSecret(SecretKey::from_seed(&seed)))
    }
}

pub struct ContinuationSponsorPlan(TransactionPlan);
impl ContinuationSponsorPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        identity: WalletIdentity,
        input: Coin,
        recipe: &ContinuationRecipe,
        index: usize,
        sponsor: Coin,
        fee: u64,
        height: u32,
        network: &Testnet11Network,
    ) -> Result<Self> {
        network.check()?;
        if input.coin_id() == sponsor.coin_id() || fee == 0 {
            return Err(WalletError::InputMismatch);
        }
        let prepared = recipe
            .prepare(
                index,
                crate::CoinRecord {
                    parent: input.parent_coin_info.to_bytes(),
                    puzzle_hash: input.puzzle_hash.to_bytes(),
                    amount: input.amount,
                },
                height,
            )
            .map_err(|_| WalletError::ProgramBuild)?;
        let game = prepared
            .spend_bundle()
            .map_err(|_| WalletError::ProgramBuild)?;
        let change_amount = checked_change(&identity, sponsor, 0, fee)?;
        let change = Coin::new(
            sponsor.coin_id(),
            identity.puzzle_hash.into(),
            change_amount,
        );
        let delegated = Conditions::new()
            .with(CreateCoin::new(
                identity.puzzle_hash.into(),
                change_amount,
                Memos::<NodePtr>::None,
            ))
            .with(ReserveFee::new(fee))
            .with(AssertConcurrentSpend::new(input.coin_id()));
        let (wallet_spend, requirement) = standard_spend(&identity, sponsor, delegated)?;
        let outputs = prepared
            .children
            .iter()
            .map(|coin| coin.coin())
            .collect::<Vec<_>>();
        let payment = *outputs.first().ok_or(WalletError::ConditionMismatch)?;
        let mut plan = TransactionPlan::new(
            identity,
            vec![game.coin_spends[0].clone(), wallet_spend],
            requirement,
            vec![outputs, vec![change]],
            payment,
            change,
            fee,
            height,
        )?;
        // Closed continuation-only measured ceiling; never caller-selected.
        plan.max_cost = MAX_CONTINUATION_COST;
        Ok(Self(plan))
    }
    pub fn verify_signed_bundle(&self, bytes: &[u8]) -> Result<VerifiedTransaction> {
        self.0.verify(bytes)
    }
    pub fn sign_hidden_tty(
        &self,
        input: Coin,
        sponsor: Coin,
        network: &Testnet11Network,
    ) -> Result<VerifiedTransaction> {
        self.0.require_inputs(&[input, sponsor], network)?;
        self.0.sign(&tty::read_master_secret()?)
    }
    #[cfg(test)]
    pub(crate) fn sign_fixture(&self, seed: [u8; 32]) -> Result<VerifiedTransaction> {
        self.0.sign(&MasterSecret(SecretKey::from_seed(&seed)))
    }
}
