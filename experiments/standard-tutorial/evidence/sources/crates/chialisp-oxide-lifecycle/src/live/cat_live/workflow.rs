use super::*;

/// A source capability and CAS revision exist only in this process. Saved bytes
/// are always rebuilt and fully BLS-validated against the fresh closed intent.
pub(super) struct CatSession {
    recipe: CatRecipe,
    store: CatStore,
    outbox: CatOutbox,
    revision: Digest,
}
impl CatSession {
    pub(super) fn create(recipe: CatRecipe, store: CatStore) -> Result<Self> {
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
    pub(super) fn load(recipe: CatRecipe, store: CatStore) -> Result<Self> {
        let stored = store.load()?;
        recipe.require_binding(&stored.outbox)?;
        Ok(Self {
            recipe,
            store,
            outbox: stored.outbox,
            revision: stored.byte_digest,
        })
    }
    pub(super) fn outbox(&self) -> &CatOutbox {
        &self.outbox
    }
    /// Cooperating writers can invalidate this loaded session. Check the exact
    /// durable byte revision and current broad source binding before external
    /// work and again immediately before a signer can read a credential.
    /// The write CAS remains authoritative for races after this read check.
    fn require_current_revision(&self) -> Result<()> {
        let stored = self.store.load()?;
        self.recipe.require_binding(&stored.outbox)?;
        ensure!(
            stored.byte_digest == self.revision,
            "CAT outbox revision changed; reload before network or signing"
        );
        Ok(())
    }
    fn commit(&mut self, event: CatEvent) -> Result<()> {
        let mut next = self.outbox.clone();
        next.append(event)?;
        self.revision = self.store.commit(Some(self.revision), &next)?;
        self.outbox = next;
        Ok(())
    }
    fn mint(&self, height: u32, token: &Testnet11Network) -> Result<CatMintPlan> {
        let saved = self.outbox.genesis().context("CAT genesis not selected")?;
        let mint = self.recipe.mint(&saved.genesis, height, token)?;
        ensure!(
            &intent(&mint, saved.genesis.clone()) == saved,
            "saved CAT genesis/asset/TAIL/Eve/lineage differs from fresh source intent"
        );
        Ok(mint)
    }
    fn verify_transfer(
        &self,
        transfer: &CatPrepared,
        height: u32,
        token: &Testnet11Network,
    ) -> Result<SpendBundle> {
        ensure!(
            transfer.preflight_height <= height,
            "CAT retained preflight is ahead of current tip"
        );
        let mint = self.mint(height, token)?;
        let historical = self.mint(transfer.preflight_height, token)?;
        let bytes = decode_hex(&transfer.bundle_hex, MAX_BUNDLE)?;
        let expected = match transfer.kind {
            CatStage::Funding => {
                let verified = mint.funding_plan().verify_signed_bundle(&bytes)?;
                let prior = historical.funding_plan().verify_signed_bundle(&bytes)?;
                let expected = prepared_funding(&prior, transfer.preflight_height);
                ensure!(
                    expected == *transfer,
                    "CAT saved funding differs from exact verified intent/costs"
                );
                verified.bundle().clone()
            }
            CatStage::Issuance => {
                let verified = mint.issuance_plan().verify_signed_bundle(&bytes)?;
                let prior = historical.issuance_plan().verify_signed_bundle(&bytes)?;
                let expected = prepared_issuance(&historical, &prior, transfer.preflight_height);
                ensure!(
                    expected == *transfer,
                    "CAT saved issuance differs from exact verified intent/costs"
                );
                verified.bundle().clone()
            }
        };
        Ok(expected)
    }
    fn verify_all(&self, height: u32, token: &Testnet11Network) -> Result<()> {
        self.recipe.require_binding(&self.outbox)?;
        if self.outbox.genesis().is_some() {
            self.mint(height, token)?;
        }
        for stage in [CatStage::Funding, CatStage::Issuance] {
            if let Some(transfer) = self.outbox.transfer(stage) {
                self.verify_transfer(transfer, height, token)?;
            }
        }
        Ok(())
    }
    pub(super) fn status<N: LiveNetwork>(&self, network: &mut N) -> Result<()> {
        self.require_current_revision()?;
        let identity = network.verify_network()?;
        let token = require_network(&identity)?;
        let tip = network.synced_tip()?;
        self.verify_all(tip.height, &token)
    }
    pub(super) fn prepare_funding<N: LiveNetwork>(&mut self, network: &mut N) -> Result<()> {
        self.prepare_funding_with(network, |plan, coin, token| {
            Ok(plan.sign_hidden_tty(coin, token)?)
        })
    }
    fn prepare_funding_with<N: LiveNetwork>(
        &mut self,
        network: &mut N,
        sign: impl FnOnce(
            &wallet::FundingPlan,
            chia_protocol::Coin,
            &Testnet11Network,
        ) -> Result<VerifiedTransaction>,
    ) -> Result<()> {
        self.require_current_revision()?;
        ensure!(
            self.outbox.transfer(CatStage::Funding).is_none(),
            "CAT funding already prepared; retry exact saved bytes"
        );
        let token = require_network(&network.verify_network()?)?;
        let tip = network.synced_tip()?;
        self.verify_all(tip.height, &token)?;
        if self.outbox.genesis().is_none() {
            let minimum = 1000u64
                .checked_add(self.recipe.binding.fee)
                .and_then(|n| n.checked_add(self.recipe.binding.issuance_fee))
                .context("CAT fee overflow")?;
            let genesis = select_coin(network, self.recipe.binding.wallet_puzzle_hash, minimum)?;
            let mint = self.recipe.mint(&genesis, tip.height, &token)?;
            self.commit(CatEvent::GenesisSelected {
                intent: intent(&mint, genesis),
            })?;
        }
        let mint = self.mint(tip.height, &token)?;
        let current = unspent(
            network,
            &self.outbox.genesis().context("missing G")?.genesis,
            tip.height,
        )?;
        // Equal tip brackets the selected input requery before the first key read.
        ensure!(
            network.synced_tip()? == tip,
            "tip moved before CAT funding signing; retry"
        );
        self.require_current_revision()?;
        let verified = sign(mint.funding_plan(), current.coin(), &token)?;
        self.commit(CatEvent::Prepared {
            transfer: prepared_funding(&verified, tip.height),
        })
    }
    pub(super) fn prepare_issuance<N: LiveNetwork>(&mut self, network: &mut N) -> Result<()> {
        self.prepare_issuance_with(network, |plan, coins, height, token| {
            Ok(wallet::cat_signer::sign_cat_plan_hidden_tty(
                plan, coins, height, token,
            )?)
        })
    }
    fn prepare_issuance_with<N: LiveNetwork>(
        &mut self,
        network: &mut N,
        sign: impl FnOnce(
            &cat::CatSpendPlan,
            &[chia_protocol::Coin],
            u32,
            &Testnet11Network,
        ) -> Result<VerifiedCatBundle>,
    ) -> Result<()> {
        self.require_current_revision()?;
        ensure!(
            self.outbox.transfer(CatStage::Issuance).is_none(),
            "CAT issuance already prepared; retry exact saved bytes"
        );
        ensure!(
            self.monitor(network, CatStage::Funding)? == LiveState::Confirmed,
            "CAT issuance requires current exact funding effects"
        );
        let token = require_network(&network.verify_network()?)?;
        let tip = network.synced_tip()?;
        self.verify_all(tip.height, &token)?;
        let mint = self.mint(tip.height, &token)?;
        let mut current = vec![];
        for coin in mint.issuance_plan().input_coins() {
            current.push(unspent(network, &record(coin), tip.height)?.coin());
        }
        ensure!(
            network.synced_tip()? == tip,
            "tip moved before CAT issuance signing; retry"
        );
        self.require_current_revision()?;
        let verified = sign(mint.issuance_plan(), &current, tip.height, &token)?;
        self.commit(CatEvent::Prepared {
            transfer: prepared_issuance(&mint, &verified, tip.height),
        })
    }
    pub(super) fn monitor<N: LiveNetwork>(
        &mut self,
        network: &mut N,
        kind: CatStage,
    ) -> Result<LiveState> {
        self.require_current_revision()?;
        let declaration = network.verify_network()?;
        let token = require_network(&declaration)?;
        let before = network.synced_tip()?;
        self.verify_all(before.height, &token)?;
        let transfer = self
            .outbox
            .transfer(kind)
            .context("CAT stage not prepared")?
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
        let all_spent = inputs.iter().all(|r| r.as_ref().is_some_and(|r| r.spent));
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
        self.commit(CatEvent::Observed {
            kind,
            bundle_id: transfer.bundle_id,
            state,
            evidence,
        })?;
        Ok(state)
    }
    pub(super) fn submit<N: LiveNetwork>(&mut self, network: &mut N, kind: CatStage) -> Result<()> {
        self.require_current_revision()?;
        let transfer = self
            .outbox
            .transfer(kind)
            .context("CAT stage not prepared")?;
        let reserve = decode_hex(&transfer.bundle_hex, MAX_BUNDLE)?
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add(128 * 1024))
            .context("CAT push reserve overflow")?;
        ensure!(
            self.outbox.events().len() <= MAX_EVENTS - 4
                && serde_json::to_vec(&self.outbox)?
                    .len()
                    .checked_add(reserve)
                    .is_some_and(|n| n <= store::MAX_BYTES),
            "CAT durable push/reconciliation capacity"
        );
        let state = self.monitor(network, kind)?;
        if state == LiveState::Confirmed {
            return Ok(());
        }
        ensure!(
            state == LiveState::Unspent,
            "CAT push requires all exact inputs currently unspent"
        );
        let token = require_network(&network.verify_network()?)?;
        let before = network.synced_tip()?;
        let transfer = self
            .outbox
            .transfer(kind)
            .context("CAT stage missing")?
            .clone();
        let bundle = self.verify_transfer(&transfer, before.height, &token)?;
        // Requery after reconciliation and after local validation. A point query
        // cannot guarantee future chain eligibility or prevent a later reorg.
        for input in &transfer.inputs {
            unspent(network, input, before.height)?;
        }
        ensure!(
            network.synced_tip()? == before,
            "tip moved before CAT push; reconcile again"
        );
        self.commit(CatEvent::PushStarted {
            kind,
            bundle_id: transfer.bundle_id,
        })?;
        let outcome = match network.push(&bundle) {
            Ok(outcome) => PushOutcome::Provider { outcome },
            Err(error) => PushOutcome::Inconclusive { kind: error.kind },
        };
        self.commit(CatEvent::PushResult {
            kind,
            bundle_id: transfer.bundle_id,
            outcome,
            receipts: network.take_receipts(),
        })
    }
}
fn select_coin<N: LiveNetwork>(network: &mut N, hash: Digest, minimum: u64) -> Result<CoinRecord> {
    let selection = network.address_coins(hash)?;
    ensure!(
        selection.records.len() <= network::MAX_COINS,
        "CAT candidate bound"
    );
    let mut coins = selection
        .records
        .into_iter()
        .filter(|c| !c.spent && c.spent_height == 0 && c.puzzle_hash == hash && c.amount >= minimum)
        .collect::<Vec<_>>();
    coins.sort_by_key(|c| (c.amount, c.id()));
    Ok(record(
        coins
            .first()
            .context("no sufficient unspent same-wallet CAT genesis")?
            .coin(),
    ))
}
fn unspent<N: LiveNetwork>(
    network: &mut N,
    expected: &CoinRecord,
    height: u32,
) -> Result<LiveCoinRecord> {
    let current = network
        .coin(expected.id())?
        .context("CAT exact input absent")?;
    ensure!(
        !current.spent
            && current.spent_height == 0
            && same_record(&current, expected)
            && current.confirmed_height <= height,
        "CAT input is not exact current unspent coin"
    );
    Ok(current)
}
fn prepared_funding(verified: &VerifiedTransaction, height: u32) -> CatPrepared {
    CatPrepared {
        kind: CatStage::Funding,
        inputs: verified
            .bundle()
            .coin_spends
            .iter()
            .map(|s| record(s.coin))
            .collect(),
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
fn prepared_issuance(mint: &CatMintPlan, verified: &VerifiedCatBundle, height: u32) -> CatPrepared {
    let cost = verified.costs();
    CatPrepared {
        kind: CatStage::Issuance,
        inputs: verified
            .bundle()
            .coin_spends
            .iter()
            .map(|s| record(s.coin))
            .collect(),
        children: vec![
            record(verified.normal_cat().coin()),
            record(verified.fee_change()),
        ],
        fee: mint.issuance_plan().fee(),
        bundle_hex: hex::encode(verified.bundle_bytes()),
        bundle_id: verified.transaction_id(),
        bundle_sha256: digest(verified.bundle_bytes()),
        preflight_height: height,
        cost: cost.total,
        execution_cost: cost.execution,
        condition_cost: cost.conditions,
        charged_generator_bytes: cost.charged_generator_bytes,
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
