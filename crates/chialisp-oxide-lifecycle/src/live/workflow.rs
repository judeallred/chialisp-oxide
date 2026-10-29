use super::*;
use network::{
    CoinSelection, HttpReceipt, LiveCoinRecord, LiveTip, NetworkIdentity, ObservedSpend,
    SubmissionOutcome,
};
use wallet::{
    FundingPlan, PaymentSponsorPlan, Testnet11Network, VerifiedTransaction, WalletIdentity,
};

/// Closed operations required by the live runner. Scripted tests implement the
/// same boundary; production uses only the fixed Testnet11 client.
pub trait LiveNetwork {
    fn verify_network(&mut self) -> network::Result<NetworkIdentity>;
    fn synced_tip(&mut self) -> network::Result<LiveTip>;
    fn coin(&mut self, id: Digest) -> network::Result<Option<LiveCoinRecord>>;
    fn observed_spend(&mut self, id: Digest, height: u32) -> network::Result<ObservedSpend>;
    fn address_coins(&mut self, puzzle_hash: Digest) -> network::Result<CoinSelection>;
    fn push(&mut self, bundle: &SpendBundle) -> network::Result<SubmissionOutcome>;
    fn take_receipts(&mut self) -> Vec<HttpReceipt>;
}
impl LiveNetwork for network::Testnet11Client {
    fn verify_network(&mut self) -> network::Result<NetworkIdentity> {
        self.verify_network()
    }
    fn synced_tip(&mut self) -> network::Result<LiveTip> {
        self.synced_tip()
    }
    fn coin(&mut self, id: Digest) -> network::Result<Option<LiveCoinRecord>> {
        self.coin(id)
    }
    fn observed_spend(&mut self, id: Digest, height: u32) -> network::Result<ObservedSpend> {
        self.observed_spend(id, height)
    }
    fn address_coins(&mut self, puzzle_hash: Digest) -> network::Result<CoinSelection> {
        self.address_coins(puzzle_hash)
    }
    fn push(&mut self, bundle: &SpendBundle) -> network::Result<SubmissionOutcome> {
        self.push(bundle)
    }
    fn take_receipts(&mut self) -> Vec<HttpReceipt> {
        self.take_receipts()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonitoringEvidence {
    pub network: NetworkIdentity,
    pub before: LiveTip,
    pub after: LiveTip,
    pub inputs: Vec<Option<LiveCoinRecord>>,
    pub spends: Vec<Option<ObservedSpend>>,
    pub children: Vec<Option<LiveCoinRecord>>,
    pub receipts: Vec<HttpReceipt>,
}

fn require_network(identity: &NetworkIdentity) -> Result<Testnet11Network> {
    ensure!(identity.scope.len() <= 1024, "bounded provider scope");
    ensure!(
        identity.origin == network::ORIGIN
            && identity.local_agg_sig_me_domain
                == TESTNET11_CONSTANTS.agg_sig_me_additional_data.to_bytes(),
        "fixed Testnet11 local signing domain"
    );
    // The genesis value is pinned locally. The provider reports only labels;
    // neither this token nor bracketing tips authenticate chain genesis/history.
    ensure!(
        identity.local_genesis_challenge == TESTNET11_CONSTANTS.genesis_challenge.to_bytes(),
        "pinned local Testnet11 genesis constant"
    );
    Ok(Testnet11Network::from_provider_labels(
        &identity.provider_network_name,
        &identity.provider_network_prefix,
    )?)
}

fn record(coin: chia_protocol::Coin) -> CoinRecord {
    CoinRecord {
        parent: coin.parent_coin_info.to_bytes(),
        puzzle_hash: coin.puzzle_hash.to_bytes(),
        amount: coin.amount,
    }
}

fn same_record(actual: &LiveCoinRecord, expected: &CoinRecord) -> bool {
    actual.parent == expected.parent
        && actual.puzzle_hash == expected.puzzle_hash
        && actual.amount == expected.amount
}

/// Check complete expected effects against exact spend bytes and child point
/// records. This is provider consistency, not an authenticated inclusion proof.
pub fn observe_state(
    transfer: &PreparedTransfer,
    evidence: &MonitoringEvidence,
) -> Result<LiveState> {
    require_network(&evidence.network)?;
    check_receipts(&evidence.receipts)?;
    ensure!(
        evidence.inputs.len() == transfer.inputs.len()
            && evidence.spends.len() == transfer.inputs.len()
            && evidence.children.len() == transfer.children.len(),
        "complete bounded observation dimensions"
    );
    for (input, spend) in evidence.inputs.iter().zip(&evidence.spends) {
        if let Some(spend) = spend {
            ensure!(
                input.as_ref().is_some_and(|input| input.spent)
                    && spend.puzzle_reveal.len() <= MAX_BUNDLE
                    && spend.solution.len() <= MAX_BUNDLE,
                "bounded spend only for a reported spent input"
            );
        }
    }
    if evidence.before != evidence.after {
        return Ok(LiveState::Inconclusive);
    }
    let mut spent = 0;
    let mut unspent = 0;
    let mut height = None;
    for (expected, actual) in transfer.inputs.iter().zip(&evidence.inputs) {
        let Some(actual) = actual else {
            return Ok(LiveState::Missing);
        };
        if !same_record(actual, expected) || actual.confirmed_height > evidence.after.height {
            return Ok(LiveState::Conflict);
        }
        if actual.spent {
            if actual.spent_height < actual.confirmed_height
                || actual.spent_height > evidence.after.height
                || actual.spent_height == 0
                || height.is_some_and(|h| h != actual.spent_height)
            {
                return Ok(LiveState::Conflict);
            }
            spent += 1;
            height = Some(actual.spent_height);
        } else {
            if actual.spent_height != 0 {
                return Ok(LiveState::Conflict);
            }
            unspent += 1;
        }
    }
    if unspent == transfer.inputs.len() {
        ensure!(
            evidence.spends.iter().all(Option::is_none)
                && evidence.children.iter().all(Option::is_none),
            "unspent inputs cannot carry spent effects"
        );
        return Ok(LiveState::Unspent);
    }
    if spent != transfer.inputs.len() {
        return Ok(LiveState::Inconclusive);
    }
    let bundle = decode_bundle(&decode_hex(&transfer.bundle_hex, MAX_BUNDLE)?)?;
    ensure!(
        bundle.coin_spends.len() == transfer.inputs.len(),
        "complete prepared spend set"
    );
    for (expected, actual) in bundle.coin_spends.iter().zip(&evidence.spends) {
        let Some(actual) = actual else {
            return Ok(LiveState::Inconclusive);
        };
        ensure!(
            actual.puzzle_reveal.len() <= MAX_BUNDLE && actual.solution.len() <= MAX_BUNDLE,
            "bounded observed spend"
        );
        if actual.coin_spend() != *expected {
            return Ok(LiveState::Conflict);
        }
    }
    for (expected, actual) in transfer.children.iter().zip(&evidence.children) {
        let Some(actual) = actual else {
            return Ok(LiveState::Inconclusive);
        };
        if !same_record(actual, expected) || Some(actual.confirmed_height) != height {
            return Ok(LiveState::Conflict);
        }
    }
    Ok(LiveState::Confirmed)
}

/// One loaded outbox is bound to fresh checks and an exact byte revision.
/// All methods commit durable public events before external push side effects.
pub struct LiveSession {
    recipe: LiveRecipe,
    store: LiveStore,
    outbox: LiveOutbox,
    revision: Digest,
}
impl LiveSession {
    pub fn create(recipe: LiveRecipe, store: LiveStore) -> Result<Self> {
        let outbox = recipe.new_outbox();
        recipe.require_binding(&outbox)?;
        let revision = store.create(&outbox)?;
        Ok(Self {
            recipe,
            store,
            outbox,
            revision,
        })
    }
    pub fn load(recipe: LiveRecipe, store: LiveStore) -> Result<Self> {
        let stored = store.load()?;
        recipe.require_binding(&stored.outbox)?;
        Ok(Self {
            recipe,
            store,
            outbox: stored.outbox,
            revision: stored.byte_digest,
        })
    }
    pub fn outbox(&self) -> &LiveOutbox {
        &self.outbox
    }
    /// Exact durable bytes, never a reserialized semantic approximation.
    pub fn revision(&self) -> Digest {
        self.revision
    }

    pub fn require_revision(&self, expected: &Digest) -> Result<()> {
        ensure!(*expected == self.revision, "stale live view revision");
        let stored = self.store.load()?;
        self.recipe.require_binding(&stored.outbox)?;
        ensure!(
            stored.byte_digest == self.revision,
            "durable outbox changed; reload required"
        );
        Ok(())
    }

    pub fn reload_current<N: LiveNetwork>(&mut self, network: &mut N) -> Result<()> {
        let stored = self.store.load()?;
        self.recipe.require_binding(&stored.outbox)?;
        let candidate = Self {
            recipe: self.recipe.clone(),
            store: self.store.clone(),
            outbox: stored.outbox,
            revision: stored.byte_digest,
        };
        let identity = network.verify_network()?;
        let tip = network.synced_tip()?;
        candidate.verify_retained(&identity, tip.height)?;
        self.outbox = candidate.outbox;
        self.revision = candidate.revision;
        Ok(())
    }

    pub(super) fn view_material(&self) -> Result<(&LiveRecipe, &LiveOutbox, Digest)> {
        self.recipe.require_binding(&self.outbox)?;
        Ok((&self.recipe, &self.outbox, self.revision))
    }

    pub fn funding_puzzle_hash(&self) -> Result<Digest> {
        self.recipe.funding_puzzle_hash()
    }
    fn commit(&mut self, event: LiveEvent) -> Result<()> {
        let mut next = self.outbox.clone();
        next.append(event)?;
        self.revision = self.store.commit(Some(self.revision), &next)?;
        self.outbox = next;
        Ok(())
    }
    fn identity(&self) -> Result<WalletIdentity> {
        let raw = decode_hex(&self.recipe.binding.master_public_key_hex, 48)?;
        Ok(WalletIdentity::from_master_public_key(
            raw.as_slice().try_into()?,
            self.recipe.binding.wallet_puzzle_hash,
        )?)
    }
    fn verify_transfer(
        &self,
        transfer: &PreparedTransfer,
        height: u32,
        token: &Testnet11Network,
    ) -> Result<SpendBundle> {
        ensure!(
            transfer.preflight_height <= height,
            "retained preflight height exceeds current provider tip"
        );
        match transfer.kind {
            TransferKind::Funding => {
                ensure!(
                    transfer.inputs.len() == 1 && transfer.fee == self.recipe.binding.fee,
                    "fixed funding inputs/fee"
                );
                let plan = FundingPlan::new(
                    self.identity()?,
                    transfer.inputs[0].coin(),
                    self.recipe.funding_puzzle_hash()?,
                    transfer.fee,
                    height,
                    token,
                )?;
                let verified =
                    plan.verify_signed_bundle(&decode_hex(&transfer.bundle_hex, MAX_BUNDLE)?)?;
                verify_fields(transfer, &verified)?;
                let historical = FundingPlan::new(
                    self.identity()?,
                    transfer.inputs[0].coin(),
                    self.recipe.funding_puzzle_hash()?,
                    transfer.fee,
                    transfer.preflight_height,
                    token,
                )?
                .verify_signed_bundle(&decode_hex(&transfer.bundle_hex, MAX_BUNDLE)?)?;
                ensure!(
                    transfer.cost == historical.cost(),
                    "retained funding cost differs from executed height-specific preflight"
                );
                Ok(verified.bundle().clone())
            }
            TransferKind::Payment if self.recipe.binding.payment_fee == 0 => {
                self.recipe.verify_payment(transfer, height)?;
                Ok(decode_bundle(&decode_hex(
                    &transfer.bundle_hex,
                    MAX_BUNDLE,
                )?)?)
            }
            TransferKind::Payment => {
                ensure!(
                    transfer.inputs.len() == 2 && transfer.fee == self.recipe.binding.payment_fee,
                    "fixed sponsored payment inputs/fee"
                );
                let plan = PaymentSponsorPlan::new(
                    self.identity()?,
                    transfer.inputs[0].coin(),
                    &self.recipe.artifact.reveal,
                    transfer.inputs[1].coin(),
                    transfer.fee,
                    height,
                    token,
                )?;
                let verified =
                    plan.verify_signed_bundle(&decode_hex(&transfer.bundle_hex, MAX_BUNDLE)?)?;
                verify_fields(transfer, &verified)?;
                let historical = PaymentSponsorPlan::new(
                    self.identity()?,
                    transfer.inputs[0].coin(),
                    &self.recipe.artifact.reveal,
                    transfer.inputs[1].coin(),
                    transfer.fee,
                    transfer.preflight_height,
                    token,
                )?
                .verify_signed_bundle(&decode_hex(&transfer.bundle_hex, MAX_BUNDLE)?)?;
                ensure!(
                    transfer.cost == historical.cost(),
                    "retained payment cost differs from executed height-specific preflight"
                );
                Ok(verified.bundle().clone())
            }
        }
    }
    fn verify_all(&self, height: u32, token: &Testnet11Network) -> Result<()> {
        self.recipe.require_binding(&self.outbox)?;
        for kind in [TransferKind::Funding, TransferKind::Payment] {
            if let Some(transfer) = self.outbox.transfer(kind) {
                self.verify_transfer(transfer, height, token)?;
            }
        }
        if let Some(payment) = self.outbox.transfer(TransferKind::Payment) {
            let funding_hash = self.recipe.funding_puzzle_hash()?;
            let funding = self
                .outbox
                .transfer(TransferKind::Funding)
                .context("missing funding transfer")?;
            let expected = funding
                .children
                .iter()
                .find(|coin| coin.puzzle_hash == funding_hash && coin.amount == 1000)
                .context("missing exact funded payment coin")?;
            ensure!(
                payment.inputs.first() == Some(expected),
                "payment must spend this funding transaction's exact child"
            );
        }
        Ok(())
    }
    /// Verify saved bytes against fresh recipe/SDK rules before status/network use.
    pub fn verify_retained(&self, network: &NetworkIdentity, height: u32) -> Result<()> {
        self.verify_all(height, &require_network(network)?)
    }
    pub fn prepare_funding<N: LiveNetwork>(&mut self, network: &mut N) -> Result<()> {
        self.prepare_funding_with(network, |plan, coin, token| {
            Ok(plan.sign_hidden_tty(coin, token)?)
        })
    }
    fn prepare_funding_with<N: LiveNetwork>(
        &mut self,
        network: &mut N,
        sign: impl FnOnce(
            &FundingPlan,
            chia_protocol::Coin,
            &Testnet11Network,
        ) -> Result<VerifiedTransaction>,
    ) -> Result<()> {
        self.require_revision(&self.revision)?;
        ensure!(
            self.outbox.transfer(TransferKind::Funding).is_none(),
            "funding already prepared; retry exact saved bundle"
        );
        let declaration = network.verify_network()?;
        let token = require_network(&declaration)?;
        let tip = network.synced_tip()?;
        self.verify_all(tip.height, &token)?;
        let input = select_coin(
            network,
            self.recipe.binding.wallet_puzzle_hash,
            self.recipe
                .binding
                .fee
                .checked_add(1000)
                .context("funding amount overflow")?,
            None,
        )?;
        let plan = FundingPlan::new(
            self.identity()?,
            input.coin(),
            self.recipe.funding_puzzle_hash()?,
            self.recipe.binding.fee,
            tip.height,
            &token,
        )?;
        // Requery immediately before the hidden TTY prompt. The signer itself
        // rejects mismatch before reading a secret or producing a signature.
        let current = unspent(network, &input, tip.height)?;
        // A cooperating writer may have appended while provider calls ran.
        // Check exact durable bytes again before a credential can be read;
        // the write CAS remains authoritative for races after this check.
        self.require_revision(&self.revision)?;
        let verified = sign(&plan, current.coin(), &token)?;
        let transfer = prepared(TransferKind::Funding, vec![input], &verified, tip.height);
        self.commit(LiveEvent::Prepared { transfer })
    }
    pub fn prepare_payment<N: LiveNetwork>(&mut self, network: &mut N) -> Result<()> {
        self.prepare_payment_with(network, |plan, payment, sponsor, token| {
            Ok(plan.sign_hidden_tty(payment, sponsor, token)?)
        })
    }
    fn prepare_payment_with<N: LiveNetwork>(
        &mut self,
        network: &mut N,
        sign: impl FnOnce(
            &PaymentSponsorPlan,
            chia_protocol::Coin,
            chia_protocol::Coin,
            &Testnet11Network,
        ) -> Result<VerifiedTransaction>,
    ) -> Result<()> {
        self.require_revision(&self.revision)?;
        ensure!(
            self.outbox.transfer(TransferKind::Payment).is_none(),
            "payment already prepared; retry exact saved bundle"
        );
        ensure!(
            self.monitor(network, TransferKind::Funding)? == LiveState::Confirmed,
            "current provider must observe exact funding effects before payment"
        );
        let declaration = network.verify_network()?;
        let token = require_network(&declaration)?;
        let tip = network.synced_tip()?;
        self.verify_all(tip.height, &token)?;
        let funding_hash = self.recipe.funding_puzzle_hash()?;
        let input = self
            .outbox
            .transfer(TransferKind::Funding)
            .context("funding missing")?
            .children
            .iter()
            .find(|coin| coin.puzzle_hash == funding_hash && coin.amount == 1000)
            .context("funded payment child missing")?
            .clone();
        unspent(network, &input, tip.height)?;
        let transfer = if self.recipe.binding.payment_fee == 0 {
            self.recipe.payment(input, tip.height)?
        } else {
            let sponsor = select_coin(
                network,
                self.recipe.binding.wallet_puzzle_hash,
                self.recipe.binding.payment_fee,
                Some(input.id()),
            )?;
            let plan = PaymentSponsorPlan::new(
                self.identity()?,
                input.coin(),
                &self.recipe.artifact.reveal,
                sponsor.coin(),
                self.recipe.binding.payment_fee,
                tip.height,
                &token,
            )?;
            let current_payment = unspent(network, &input, tip.height)?;
            let current_sponsor = unspent(network, &sponsor, tip.height)?;
            self.require_revision(&self.revision)?;
            let verified = sign(
                &plan,
                current_payment.coin(),
                current_sponsor.coin(),
                &token,
            )?;
            prepared(
                TransferKind::Payment,
                vec![input, sponsor],
                &verified,
                tip.height,
            )
        };
        self.commit(LiveEvent::Prepared { transfer })
    }
    pub fn monitor<N: LiveNetwork>(
        &mut self,
        network: &mut N,
        kind: TransferKind,
    ) -> Result<LiveState> {
        self.require_revision(&self.revision)?;
        let declaration = network.verify_network()?;
        let token = require_network(&declaration)?;
        let before = network.synced_tip()?;
        self.verify_all(before.height, &token)?;
        let transfer = self
            .outbox
            .transfer(kind)
            .context("transfer not prepared")?
            .clone();
        let mut inputs = vec![];
        let mut spends = vec![];
        for input in &transfer.inputs {
            let actual = network.coin(input.id())?;
            spends.push(match &actual {
                Some(actual) if actual.spent => {
                    Some(network.observed_spend(input.id(), actual.spent_height)?)
                }
                _ => None,
            });
            inputs.push(actual);
        }
        let all_spent = inputs
            .iter()
            .all(|value| value.as_ref().is_some_and(|coin| coin.spent));
        let mut children = vec![];
        for child in &transfer.children {
            children.push(if all_spent {
                network.coin(child.id())?
            } else {
                None
            });
        }
        let after = network.synced_tip()?;
        let evidence = MonitoringEvidence {
            network: declaration,
            before,
            after,
            inputs,
            spends,
            children,
            receipts: network.take_receipts(),
        };
        let state = observe_state(&transfer, &evidence)?;
        self.commit(LiveEvent::Observed {
            kind,
            bundle_id: transfer.bundle_id,
            state,
            evidence,
        })?;
        Ok(state)
    }
    /// Reconcile before every push, including retries after a lost response.
    /// Only all currently unspent inputs permit an explicit retry of saved bytes.
    pub fn submit<N: LiveNetwork>(&mut self, network: &mut N, kind: TransferKind) -> Result<()> {
        self.require_revision(&self.revision)?;
        let transfer = self
            .outbox
            .transfer(kind)
            .context("transfer not prepared")?;
        // Reserve the reconciliation, intent, response and one confirmation
        // event before HTTP side effects. Observed program byte arrays use at
        // most four JSON characters per byte; 128 KiB additionally covers the
        // bounded records, labels, receipts and push-result envelope. Exhaustion
        // must not first appear after a successful push.
        let reserve = decode_hex(&transfer.bundle_hex, MAX_BUNDLE)?
            .len()
            .checked_mul(4)
            .and_then(|bytes| bytes.checked_add(128 * 1024))
            .context("push reserve overflow")?;
        ensure!(
            self.outbox.events().len() <= MAX_EVENTS - 4
                && serde_json::to_vec(&self.outbox)?
                    .len()
                    .checked_add(reserve)
                    .is_some_and(|bytes| bytes <= outbox::MAX_BYTES),
            "live outbox has insufficient durable space for push/reconciliation"
        );
        let state = self.monitor(network, kind)?;
        if state == LiveState::Confirmed {
            return Ok(());
        }
        ensure!(
            state == LiveState::Unspent,
            "submission is blocked until all exact inputs are currently unspent"
        );
        let declaration = network.verify_network()?;
        let token = require_network(&declaration)?;
        let tip = network.synced_tip()?;
        let transfer = self
            .outbox
            .transfer(kind)
            .context("transfer not prepared")?
            .clone();
        let bundle = self.verify_transfer(&transfer, tip.height, &token)?;
        self.commit(LiveEvent::PushStarted {
            kind,
            bundle_id: transfer.bundle_id,
        })?;
        let outcome = match network.push(&bundle) {
            Ok(outcome) => PushOutcome::Provider { outcome },
            Err(error) => PushOutcome::Inconclusive { kind: error.kind },
        };
        self.commit(LiveEvent::PushResult {
            kind,
            bundle_id: transfer.bundle_id,
            outcome,
            receipts: network.take_receipts(),
        })
    }
}

fn select_coin<N: LiveNetwork>(
    network: &mut N,
    puzzle_hash: Digest,
    minimum: u64,
    exclude: Option<Digest>,
) -> Result<CoinRecord> {
    let candidates = network.address_coins(puzzle_hash)?;
    // A candidate set need not be a complete wallet balance; selection checks
    // the chosen exact coin again and makes no whole-wallet claim.
    ensure!(
        candidates.records.len() <= network::MAX_COINS,
        "coin candidate bound"
    );
    let mut candidates = candidates
        .records
        .into_iter()
        .filter(|coin| {
            !coin.spent
                && coin.puzzle_hash == puzzle_hash
                && coin.amount >= minimum
                && Some(coin.id()) != exclude
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|coin| (coin.amount, coin.id()));
    let selected = candidates
        .first()
        .context("no sufficient unspent Testnet11 wallet coin in bounded candidate set")?;
    Ok(record(selected.coin()))
}

fn unspent<N: LiveNetwork>(
    network: &mut N,
    expected: &CoinRecord,
    height: u32,
) -> Result<LiveCoinRecord> {
    let current = network
        .coin(expected.id())?
        .context("selected input absent")?;
    ensure!(
        !current.spent && current.spent_height == 0 && same_record(&current, expected),
        "selected input is not the exact unspent coin"
    );
    ensure!(
        current.confirmed_height <= height,
        "selected input has future confirmation height relative to fresh provider tip"
    );
    // The upper bound is a provider-consistency check, not authentication of
    // coin availability, reward maturity or complete chain eligibility.
    Ok(current)
}

fn prepared(
    kind: TransferKind,
    inputs: Vec<CoinRecord>,
    verified: &VerifiedTransaction,
    height: u32,
) -> PreparedTransfer {
    PreparedTransfer {
        kind,
        inputs,
        children: vec![
            record(verified.payment_coin()),
            record(verified.change_coin()),
        ],
        fee: verified.fee(),
        bundle_hex: hex::encode(verified.bundle_bytes()),
        bundle_id: verified.transaction_id(),
        bundle_sha256: digest(verified.bundle_bytes()),
        preflight_height: height,
        cost: verified.cost(),
        execution_cost: verified.execution_cost(),
        condition_cost: verified.condition_cost(),
        charged_generator_bytes: verified.charged_generator_bytes(),
    }
}
fn verify_fields(transfer: &PreparedTransfer, verified: &VerifiedTransaction) -> Result<()> {
    ensure!(
        transfer.bundle_id == verified.transaction_id()
            && transfer.bundle_sha256 == digest(verified.bundle_bytes())
            && transfer.bundle_hex == hex::encode(verified.bundle_bytes())
            && transfer.fee == verified.fee()
            && transfer.execution_cost == verified.execution_cost()
            && transfer.condition_cost == verified.condition_cost()
            && transfer.charged_generator_bytes == verified.charged_generator_bytes()
            && transfer.children
                == [
                    record(verified.payment_coin()),
                    record(verified.change_coin())
                ],
        "retained signed transfer differs from exact locally validated intent"
    );
    Ok(())
}

#[cfg(test)]
#[path = "workflow_tests.rs"]
mod tests;
