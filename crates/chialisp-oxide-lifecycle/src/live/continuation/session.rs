pub(super) use super::observation::observe;
use super::*;
use crate::live::{
    LiveNetwork, LiveState, MonitoringEvidence,
    network::{self, LiveTip},
    wallet::{
        Testnet11Network, VerifiedTransaction, WalletIdentity,
        continuation::{ContinuationFundingPlan, ContinuationSponsorPlan},
    },
};

pub trait ContinuationNetwork: LiveNetwork {
    fn continuation_tip(&mut self) -> network::Result<(LiveTip, u32)>;
}
impl ContinuationNetwork for network::Testnet11Client {
    fn continuation_tip(&mut self) -> network::Result<(LiveTip, u32)> {
        self.continuation_tip()
    }
}
pub(super) struct CheckedTransaction {
    pub(super) bundle: SpendBundle,
    pub(super) children: Vec<CoinRecord>,
}

/// Shared CLI/GUI state machine. A persisted push intent permanently closes
/// submission: restart/transport failure requires observation, never blind retry.
pub struct ContinuationSession {
    recipe: ContinuationRecipe,
    identity: WalletIdentity,
    store: ContinuationStore,
    outbox: ContinuationOutbox,
    revision: Digest,
}
impl ContinuationSession {
    pub fn create(
        recipe: ContinuationRecipe,
        identity: WalletIdentity,
        store: ContinuationStore,
        funding_fee: u64,
        step_fee: u64,
    ) -> Result<Self> {
        recipe.require_current()?;
        let outbox = ContinuationOutbox::new(ContinuationBinding {
            plan: recipe.plan().clone(),
            wallet_hash: identity.wallet_puzzle_hash(),
            master_public_key: identity.master_public_key().to_vec(),
            funding_fee,
            step_fee,
        })?;
        let revision = store.create(&outbox)?;
        Ok(Self {
            recipe,
            identity,
            store,
            outbox,
            revision,
        })
    }
    pub fn load(
        recipe: ContinuationRecipe,
        identity: WalletIdentity,
        store: ContinuationStore,
    ) -> Result<Self> {
        let stored = store.load()?;
        let value = Self {
            recipe,
            identity,
            store,
            outbox: stored.outbox,
            revision: stored.byte_digest,
        };
        value.require_binding()?;
        Ok(value)
    }
    pub fn revision(&self) -> Digest {
        self.revision
    }
    pub fn outbox(&self) -> &ContinuationOutbox {
        &self.outbox
    }
    pub fn require_revision(&self, revision: &Digest) -> Result<()> {
        self.recipe.require_current()?;
        ensure!(
            *revision == self.revision && self.store.load()?.byte_digest == self.revision,
            ContinuationError::RevisionConflict
        );
        Ok(())
    }
    fn require_binding(&self) -> Result<()> {
        self.recipe.require_current()?;
        self.outbox.validate_shape()?;
        let b = &self.outbox.binding;
        ensure!(
            b.plan == *self.recipe.plan()
                && b.wallet_hash == self.identity.wallet_puzzle_hash()
                && b.master_public_key == self.identity.master_public_key(),
            "fresh recipe/wallet/full queue binding changed"
        );
        let transactions = self.outbox.transactions();
        let checked = transactions
            .iter()
            .map(|tx| self.checked(tx))
            .collect::<Result<Vec<_>>>()?;
        for (ordinal, tx) in transactions.iter().enumerate() {
            if ordinal > 0 {
                let prior = &checked[ordinal - 1];
                let expected = if ordinal == 1 {
                    &prior.children[0]
                } else {
                    &prior.children[1]
                };
                ensure!(
                    tx.input == *expected,
                    "continuation chain parent linkage changed"
                );
            }
        }
        // Recompute every retained observation from the actual freshly checked
        // transaction. Retained JSON is history, never execution authority.
        for event in &self.outbox.events {
            if let ContinuationEvent::Observed {
                ordinal,
                state,
                evidence,
            } = event
            {
                ensure!(
                    observe(&checked[*ordinal], evidence)? == *state,
                    "retained observation mismatch"
                );
            }
        }
        Ok(())
    }
    pub fn reload_current(&mut self) -> Result<()> {
        let stored = self.store.load()?;
        self.recipe.require_current()?;
        if stored.byte_digest == self.revision {
            return Ok(());
        }
        ensure!(
            stored.outbox.follows(&self.outbox),
            "journal history was rewritten"
        );
        let old = std::mem::replace(&mut self.outbox, stored.outbox);
        if let Err(error) = self.require_binding() {
            self.outbox = old;
            return Err(error);
        }
        self.revision = stored.byte_digest;
        Ok(())
    }
    fn append(&mut self, event: ContinuationEvent) -> Result<()> {
        self.require_revision(&self.revision)?;
        let mut next = self.outbox.clone();
        next.events.push(event);
        match self.store.commit(Some(self.revision), &next) {
            Ok(revision) => {
                self.outbox = next;
                self.revision = revision;
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }
    fn checked(&self, tx: &ContinuationTransaction) -> Result<CheckedTransaction> {
        let net = Testnet11Network::from_provider_labels("testnet11", "txch")?;
        let height = tx.previous_transaction_height;
        let (bundle, children) = match tx.kind {
            ContinuationTransfer::Funding => {
                ensure!(tx.sponsor.is_none(), "funding sponsor forbidden");
                let plan = ContinuationFundingPlan::new(
                    self.identity,
                    tx.input.coin(),
                    &self.recipe,
                    self.outbox.binding.funding_fee,
                    height,
                    &net,
                )?;
                let checked = plan.verify_signed_bundle(&tx.signed_bundle)?;
                (
                    checked.bundle().clone(),
                    vec![
                        record(checked.payment_coin()),
                        record(checked.change_coin()),
                    ],
                )
            }
            ContinuationTransfer::Step(index) => {
                let prepared = self.recipe.prepare(index, tx.input.clone(), height)?;
                if let Some(sponsor) = &tx.sponsor {
                    ensure!(self.outbox.binding.step_fee > 0, "unexpected sponsor");
                    let plan = ContinuationSponsorPlan::new(
                        self.identity,
                        tx.input.coin(),
                        &self.recipe,
                        index,
                        sponsor.coin(),
                        self.outbox.binding.step_fee,
                        height,
                        &net,
                    )?;
                    let checked = plan.verify_signed_bundle(&tx.signed_bundle)?;
                    let mut children = prepared.children;
                    children.push(record(checked.change_coin()));
                    (checked.bundle().clone(), children)
                } else {
                    ensure!(
                        self.outbox.binding.step_fee == 0 && tx.signed_bundle == prepared.bundle,
                        "exact permissionless step bytes"
                    );
                    (prepared.spend_bundle()?, prepared.children)
                }
            }
        };
        Ok(CheckedTransaction { bundle, children })
    }
    fn context(
        &self,
        network: &mut impl ContinuationNetwork,
    ) -> Result<(Testnet11Network, LiveTip, u32)> {
        self.require_revision(&self.revision)?;
        let identity = network.verify_network()?;
        let token = crate::live::workflow::require_network(&identity)?;
        let (tip, height) = network.continuation_tip()?;
        Ok((token, tip, height))
    }
    fn available(
        &self,
        network: &mut impl ContinuationNetwork,
        coin: &CoinRecord,
        tip: &LiveTip,
    ) -> Result<()> {
        let observed = network.coin(coin.id())?.context("input missing")?;
        ensure!(
            observed.confirmed_height <= tip.height,
            ContinuationError::Inconclusive
        );
        ensure!(
            record(observed.coin()) == *coin && !observed.spent && observed.spent_height == 0,
            ContinuationError::Conflict
        );
        Ok(())
    }
    fn candidate(
        &self,
        network: &mut impl ContinuationNetwork,
        principal: u64,
        fee: u64,
        tip: &LiveTip,
    ) -> Result<CoinRecord> {
        let selection = network.address_coins(self.identity.wallet_puzzle_hash())?;
        ensure!(
            selection.records.len() <= network::MAX_COINS,
            "bounded wallet candidates"
        );
        // Candidate scope need not be a complete wallet inventory. Every chosen
        // exact coin is separately queried immediately before a signer callback.
        selection
            .records
            .iter()
            .filter(|coin| {
                !coin.spent
                    && coin.spent_height == 0
                    && coin.puzzle_hash == self.identity.wallet_puzzle_hash()
                    && coin.amount > principal.saturating_add(fee)
                    && coin.confirmed_height <= tip.height
            })
            .min_by_key(|coin| (coin.amount, coin.id()))
            .map(|coin| record(coin.coin()))
            .context("no adequate single wallet candidate")
    }
    pub fn prepare_funding(&mut self, network: &mut impl ContinuationNetwork) -> Result<()> {
        self.prepare_funding_with(network, |plan, input, net| {
            Ok(plan.sign_hidden_tty(input, net)?)
        })
    }
    pub fn prepare_funding_with(
        &mut self,
        network: &mut impl ContinuationNetwork,
        sign: impl FnOnce(
            &ContinuationFundingPlan,
            chia_protocol::Coin,
            &Testnet11Network,
        ) -> Result<VerifiedTransaction>,
    ) -> Result<()> {
        ensure!(
            self.outbox.transactions().is_empty(),
            "funding already prepared"
        );
        let (net, tip, height) = self.context(network)?;
        let input = self.candidate(
            network,
            self.recipe.funding_amount(),
            self.outbox.binding.funding_fee,
            &tip,
        )?;
        let plan = ContinuationFundingPlan::new(
            self.identity,
            input.coin(),
            &self.recipe,
            self.outbox.binding.funding_fee,
            height,
            &net,
        )?;
        self.available(network, &input, &tip)?;
        ensure!(
            network.continuation_tip()? == (tip, height),
            ContinuationError::Inconclusive
        );
        self.require_revision(&self.revision)?; // before ANY signing callback
        let signed = sign(&plan, input.coin(), &net)?;
        let tx = ContinuationTransaction {
            kind: ContinuationTransfer::Funding,
            input,
            sponsor: None,
            previous_transaction_height: height,
            signed_bundle: signed.bundle_bytes().to_vec(),
        };
        self.checked(&tx)?;
        self.append(ContinuationEvent::Prepared { transaction: tx })
    }
    pub fn prepare_step(&mut self, network: &mut impl ContinuationNetwork) -> Result<()> {
        self.prepare_step_with(network, |plan, input, sponsor, net| {
            Ok(plan.sign_hidden_tty(input, sponsor, net)?)
        })
    }
    pub fn prepare_step_with(
        &mut self,
        network: &mut impl ContinuationNetwork,
        sign: impl FnOnce(
            &ContinuationSponsorPlan,
            chia_protocol::Coin,
            chia_protocol::Coin,
            &Testnet11Network,
        ) -> Result<VerifiedTransaction>,
    ) -> Result<()> {
        let transactions = self.outbox.transactions();
        let ordinal = transactions
            .len()
            .checked_sub(1)
            .context("funding not prepared")?;
        ensure!(
            self.outbox.state(ordinal) == Some(LiveState::Confirmed),
            "observe prior transaction confirmed first"
        );
        let index = ordinal;
        ensure!(index < self.recipe.plan().steps.len(), "queue complete");
        let previous = self.checked(transactions[ordinal])?;
        let input = if ordinal == 0 {
            previous.children[0].clone()
        } else {
            previous.children[1].clone()
        };
        // Re-observe the previous spend under current provider tips, including
        // its exact complete output set, before using the retained child.
        ensure!(
            self.monitor(network)? == LiveState::Confirmed,
            ContinuationError::Inconclusive
        );
        let (net, tip, height) = self.context(network)?;
        self.available(network, &input, &tip)?;
        let prepared = self.recipe.prepare(index, input.clone(), height)?;
        let (sponsor, bytes) = if self.outbox.binding.step_fee == 0 {
            (None, prepared.bundle)
        } else {
            let sponsor = self.candidate(network, 0, self.outbox.binding.step_fee, &tip)?;
            let plan = ContinuationSponsorPlan::new(
                self.identity,
                input.coin(),
                &self.recipe,
                index,
                sponsor.coin(),
                self.outbox.binding.step_fee,
                height,
                &net,
            )?;
            self.available(network, &sponsor, &tip)?;
            self.available(network, &input, &tip)?;
            ensure!(
                network.continuation_tip()? == (tip.clone(), height),
                ContinuationError::Inconclusive
            );
            self.require_revision(&self.revision)?;
            let signed = sign(&plan, input.coin(), sponsor.coin(), &net)?;
            (Some(sponsor), signed.bundle_bytes().to_vec())
        };
        self.require_revision(&self.revision)?;
        let tx = ContinuationTransaction {
            kind: ContinuationTransfer::Step(index),
            input,
            sponsor,
            previous_transaction_height: height,
            signed_bundle: bytes,
        };
        self.checked(&tx)?;
        self.append(ContinuationEvent::Prepared { transaction: tx })
    }
    pub fn submit(&mut self, network: &mut impl ContinuationNetwork) -> Result<()> {
        self.require_revision(&self.revision)?;
        let txs = self.outbox.transactions();
        let ordinal = txs.len().checked_sub(1).context("nothing prepared")?;
        ensure!(
            !self.outbox.pushed(ordinal),
            ContinuationError::AlreadySubmitted
        );
        let checked = self.checked(txs[ordinal])?;
        let (_, tip, _) = self.context(network)?;
        for spend in &checked.bundle.coin_spends {
            self.available(network, &record(spend.coin), &tip)?;
        }
        ensure!(
            network.synced_tip()? == tip,
            ContinuationError::Inconclusive
        );
        self.append(ContinuationEvent::PushStarted { ordinal })?;
        // On ANY error after this durable intent, monitor; never push again.
        let outcome = network.push(&checked.bundle)?;
        let id = match &outcome {
            network::SubmissionOutcome::MempoolAccepted { transaction_id }
            | network::SubmissionOutcome::Pending { transaction_id }
            | network::SubmissionOutcome::Rejected { transaction_id, .. } => *transaction_id,
        };
        ensure!(
            id == checked.bundle.name().to_bytes(),
            "provider push ID differs"
        );
        self.append(ContinuationEvent::PushResult { ordinal, outcome })
    }
    pub fn monitor(&mut self, network: &mut impl ContinuationNetwork) -> Result<LiveState> {
        self.require_revision(&self.revision)?;
        let txs = self.outbox.transactions();
        let ordinal = txs.len().checked_sub(1).context("nothing prepared")?;
        let checked = self.checked(txs[ordinal])?;
        let identity = network.verify_network()?;
        crate::live::workflow::require_network(&identity)?;
        let before = network.synced_tip()?;
        let mut inputs = vec![];
        let mut spends = vec![];
        for spend in &checked.bundle.coin_spends {
            let input = network.coin(spend.coin.coin_id().to_bytes())?;
            let actual = if let Some(coin) = &input {
                if coin.spent {
                    Some(network.observed_spend(coin.id(), coin.spent_height)?)
                } else {
                    None
                }
            } else {
                None
            };
            inputs.push(input);
            spends.push(actual);
        }
        let mut children = vec![];
        for child in &checked.children {
            children.push(network.coin(child.id())?);
        }
        let after = network.synced_tip()?;
        let evidence = MonitoringEvidence {
            network: identity,
            before,
            after,
            inputs,
            spends,
            children,
            receipts: network.take_receipts(),
        };
        let state = observe(&checked, &evidence)?;
        self.append(ContinuationEvent::Observed {
            ordinal,
            state,
            evidence: Box::new(evidence),
        })?;
        Ok(state)
    }
    fn available_actions(&self) -> Vec<&'static str> {
        let mut actions = vec!["reload"];
        let txs = self.outbox.transactions();
        if txs.is_empty() {
            actions.push("prepare_funding");
            return actions;
        }
        let ordinal = txs.len() - 1;
        actions.push("monitor_payment");
        if !self.outbox.pushed(ordinal) && self.outbox.state(ordinal) != Some(LiveState::Confirmed)
        {
            actions.push("submit_payment");
        }
        if self.outbox.state(ordinal) == Some(LiveState::Confirmed)
            && ordinal < self.recipe.plan().steps.len()
        {
            actions.push("prepare_payment");
        }
        actions
    }
    /// One guarded worker-start operation. Public view keeps its independent
    /// guard; startup must not hash the same frozen toolchain again merely to
    /// format a view immediately after the checked reload.
    pub(crate) fn reload_view_current(&mut self) -> Result<serde_json::Value> {
        self.reload_current()?;
        self.view_snapshot()
    }
    pub fn view(&self) -> Result<serde_json::Value> {
        self.recipe.require_current()?;
        self.view_snapshot()
    }
    fn view_snapshot(&self) -> Result<serde_json::Value> {
        let txs = self.outbox.transactions();
        let current = txs.len().checked_sub(1);
        Ok(
            serde_json::json!({"protocol":PROTOCOL,"revision":hex::encode(self.revision),"plan":self.recipe.plan(),"wallet":hex::encode(self.identity.wallet_puzzle_hash()),"funding_fee":self.outbox.binding.funding_fee,"step_fee":self.outbox.binding.step_fee,"prepared_transactions":txs.len(),"current_state":current.and_then(|i|self.outbox.state(i)),"push_started":current.is_some_and(|i|self.outbox.pushed(i)),"available_actions":self.available_actions(),"scope":"Fresh local source/tool consistency and executed exact outputs; provider consistency, not authenticated inclusion or finality. WIP, not ready for use."}),
        )
    }
}
fn record(coin: chia_protocol::Coin) -> CoinRecord {
    CoinRecord {
        parent: coin.parent_coin_info.to_bytes(),
        puzzle_hash: coin.puzzle_hash.to_bytes(),
        amount: coin.amount,
    }
}
