use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatBinding {
    pub network: crate::Network,
    pub wallet_puzzle_hash: Digest,
    pub master_public_key_hex: String,
    pub source_identity: Digest,
    pub module_hash: Digest,
    pub module_sha256: Digest,
    pub mode: String,
    pub recipient: Digest,
    pub fee: u64,
    pub issuance_fee: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenesisIntent {
    pub genesis: CoinRecord,
    pub asset_id: Digest,
    pub tail_hex: String,
    pub eve: CoinRecord,
    pub funding_change: CoinRecord,
    pub normal: CoinRecord,
    pub normal_parent: CoinRecord,
    pub inner_puzzle_hash: Digest,
    pub lineage_parent_parent: Digest,
    pub lineage_inner: Digest,
    pub lineage_amount: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatStage {
    Funding,
    Issuance,
}

/// Complete deterministic public intent and the exact signed bytes to retry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatPrepared {
    pub kind: CatStage,
    pub inputs: Vec<CoinRecord>,
    pub children: Vec<CoinRecord>,
    pub fee: u64,
    pub bundle_hex: String,
    pub bundle_id: Digest,
    pub bundle_sha256: Digest,
    pub preflight_height: u32,
    pub cost: u64,
    pub execution_cost: u64,
    pub condition_cost: u64,
    pub charged_generator_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum CatEvent {
    GenesisSelected {
        intent: GenesisIntent,
    },
    Prepared {
        transfer: CatPrepared,
    },
    /// Written before an HTTP push. A crash after this event is inconclusive;
    /// reconciliation queries the exact inputs/children before any retry.
    PushStarted {
        kind: CatStage,
        bundle_id: Digest,
    },
    PushResult {
        kind: CatStage,
        bundle_id: Digest,
        outcome: PushOutcome,
        receipts: Vec<network::HttpReceipt>,
    },
    Observed {
        kind: CatStage,
        bundle_id: Digest,
        state: LiveState,
        evidence: MonitoringEvidence,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum PushOutcome {
    Provider { outcome: network::SubmissionOutcome },
    Inconclusive { kind: network::ErrorKind },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveState {
    Missing,
    Unspent,
    Confirmed,
    Conflict,
    Inconclusive,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatOutbox {
    pub(super) protocol: String,
    pub binding: CatBinding,
    pub(super) events: Vec<CatEvent>,
}

impl CatOutbox {
    pub fn genesis(&self) -> Option<&GenesisIntent> {
        self.events.iter().find_map(|event| match event {
            CatEvent::GenesisSelected { intent } => Some(intent),
            _ => None,
        })
    }
    pub fn events(&self) -> &[CatEvent] {
        &self.events
    }
    pub fn transfer(&self, kind: CatStage) -> Option<&CatPrepared> {
        self.events.iter().find_map(|event| match event {
            CatEvent::Prepared { transfer } if transfer.kind == kind => Some(transfer),
            _ => None,
        })
    }
    pub fn state(&self, kind: CatStage) -> LiveState {
        self.events
            .iter()
            .rev()
            .find_map(|event| match event {
                CatEvent::Observed { kind: k, state, .. } if *k == kind => Some(*state),
                _ => None,
            })
            .unwrap_or(LiveState::Missing)
    }
    pub fn append(&mut self, event: CatEvent) -> Result<()> {
        ensure!(self.events.len() < MAX_EVENTS, "live outbox event bound");
        self.events.push(event);
        if let Err(error) = self.validate_shape() {
            self.events.pop();
            return Err(error);
        }
        Ok(())
    }
    pub(crate) fn validate_shape(&self) -> Result<()> {
        ensure!(
            self.protocol == PROTOCOL && self.events.len() <= MAX_EVENTS,
            "live outbox protocol/event bound"
        );
        ensure!(
            self.binding.network == crate::provider::testnet11_network(),
            "explicit Testnet11 only"
        );
        ensure!(
            self.binding.fee <= MAX_FEE
                && self.binding.issuance_fee <= MAX_FEE
                && self.binding.master_public_key_hex.len() == 96,
            "bounded live fee/public key"
        );
        decode_hex(&self.binding.master_public_key_hex, 48)?;
        ensure!(
            self.binding.recipient == self.binding.wallet_puzzle_hash
                && matches!(self.binding.mode.as_str(), "off" | "on"),
            "closed CAT wallet/mode"
        );
        wallet::WalletIdentity::from_master_public_key(
            decode_hex(&self.binding.master_public_key_hex, 48)?
                .as_slice()
                .try_into()?,
            self.binding.wallet_puzzle_hash,
        )?;
        let mut genesis = None;
        let mut funding: Option<&CatPrepared> = None;
        let mut issuance: Option<&CatPrepared> = None;
        let mut pending_push = None;
        let mut funding_state = LiveState::Missing;
        for event in &self.events {
            match event {
                CatEvent::GenesisSelected { intent } => {
                    ensure!(
                        genesis.is_none()
                            && funding.is_none()
                            && issuance.is_none()
                            && pending_push.is_none(),
                        "genesis selection occurs once before preparation"
                    );
                    ensure!(
                        intent.genesis.puzzle_hash == self.binding.wallet_puzzle_hash
                            && intent.genesis.amount
                                >= 1000u64
                                    .checked_add(self.binding.fee)
                                    .and_then(|v| v.checked_add(self.binding.issuance_fee))
                                    .context("CAT fee overflow")?
                            && intent.eve.amount == 1000
                            && intent.normal.amount == 1000
                            && intent.inner_puzzle_hash == self.binding.wallet_puzzle_hash,
                        "bounded same-wallet CAT genesis intent"
                    );
                    decode_hex(&intent.tail_hex, 32 * 1024)?;
                    genesis = Some(intent);
                }
                CatEvent::Prepared { transfer } => {
                    ensure!(pending_push.is_none(), "prepare while push unresolved");
                    ensure!(
                        genesis.is_some(),
                        "preparation requires exact selected genesis"
                    );
                    ensure!(
                        !transfer.inputs.is_empty()
                            && transfer.inputs.len() <= 2
                            && !transfer.children.is_empty()
                            && transfer.children.len() <= 3
                            && transfer.fee <= MAX_FEE
                            && transfer.cost
                                <= match transfer.kind {
                                    CatStage::Funding => wallet::MAX_WALLET_BUNDLE_COST,
                                    CatStage::Issuance => cat::MAX_CAT_BUNDLE_COST,
                                },
                        "bounded transfer dimensions"
                    );
                    let byte_cost = u64::try_from(transfer.charged_generator_bytes)?
                        .checked_mul(TESTNET11_CONSTANTS.cost_per_byte)
                        .context("generator byte cost overflow")?;
                    ensure!(
                        transfer.charged_generator_bytes <= MAX_BUNDLE * 2
                            && transfer
                                .execution_cost
                                .checked_add(transfer.condition_cost)
                                .and_then(|cost| cost.checked_add(byte_cost))
                                == Some(transfer.cost),
                        "plain generator charged bytes and complete cost components"
                    );
                    let bytes = decode_hex(&transfer.bundle_hex, MAX_BUNDLE)?;
                    ensure!(
                        digest(&bytes) == transfer.bundle_sha256,
                        "exact signed bundle digest"
                    );
                    let bundle = decode_bundle(&bytes)?;
                    ensure!(
                        bundle.name().to_bytes() == transfer.bundle_id
                            && bundle
                                .coin_spends
                                .iter()
                                .map(|spend| spend.coin.coin_id().to_bytes())
                                .collect::<Vec<_>>()
                                == transfer
                                    .inputs
                                    .iter()
                                    .map(CoinRecord::id)
                                    .collect::<Vec<_>>(),
                        "exact bounded bundle input identities"
                    );
                    let slot = match transfer.kind {
                        CatStage::Funding => &mut funding,
                        CatStage::Issuance => {
                            ensure!(
                                funding_state == LiveState::Confirmed,
                                "issuance requires observed funding"
                            );
                            &mut issuance
                        }
                    };
                    ensure!(slot.is_none(), "duplicate prepared transfer");
                    *slot = Some(transfer);
                }
                CatEvent::PushStarted { kind, bundle_id } => {
                    ensure!(pending_push.is_none(), "unreconciled push intent");
                    ensure!(
                        match kind {
                            CatStage::Funding => funding,
                            CatStage::Issuance => issuance,
                        }
                        .map(|t| t.bundle_id)
                            == Some(*bundle_id),
                        "push must name exact prepared bundle"
                    );
                    pending_push = Some((*kind, *bundle_id));
                }
                CatEvent::PushResult {
                    kind,
                    bundle_id,
                    outcome,
                    receipts,
                } => {
                    ensure!(
                        pending_push == Some((*kind, *bundle_id)),
                        "push result must match pending exact bundle"
                    );
                    check_receipts(receipts)?;
                    if let PushOutcome::Provider { outcome } = outcome {
                        let id = match outcome {
                            network::SubmissionOutcome::MempoolAccepted { transaction_id }
                            | network::SubmissionOutcome::Pending { transaction_id } => {
                                transaction_id
                            }
                            network::SubmissionOutcome::Rejected {
                                transaction_id,
                                provider_code,
                            } => {
                                ensure!(provider_code.len() <= 128, "bounded provider code");
                                transaction_id
                            }
                        };
                        ensure!(
                            *id == *bundle_id,
                            "provider push result transaction binding"
                        );
                    }
                    pending_push = None;
                }
                CatEvent::Observed {
                    kind,
                    bundle_id,
                    state,
                    evidence,
                } => {
                    let transfer = match kind {
                        CatStage::Funding => funding,
                        CatStage::Issuance => issuance,
                    };
                    ensure!(
                        transfer.map(|t| t.bundle_id) == Some(*bundle_id),
                        "observation must name prepared bundle"
                    );
                    ensure!(
                        observe_state(transfer.context("missing transfer")?, evidence)? == *state,
                        "retained state differs from bounded observation"
                    );
                    // A bounded current observation resolves a prior uncertain
                    // transport attempt; it does not retroactively assert success.
                    pending_push = None;
                    if *kind == CatStage::Funding {
                        funding_state = *state;
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn follows(&self, old: &Self) -> bool {
        self.protocol == old.protocol
            && self.binding == old.binding
            && self.events.starts_with(&old.events)
    }
}
