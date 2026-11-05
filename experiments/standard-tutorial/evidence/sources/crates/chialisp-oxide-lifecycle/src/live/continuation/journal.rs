use super::*;
use crate::live::{LiveState, MonitoringEvidence, network::SubmissionOutcome};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationBinding {
    pub plan: ContinuationPlan,
    pub wallet_hash: Digest,
    pub master_public_key: Vec<u8>,
    pub funding_fee: u64,
    pub step_fee: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationTransfer {
    Funding,
    Step(usize),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationTransaction {
    pub kind: ContinuationTransfer,
    pub input: CoinRecord,
    pub sponsor: Option<CoinRecord>,
    pub previous_transaction_height: u32,
    pub signed_bundle: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContinuationEvent {
    Prepared {
        transaction: ContinuationTransaction,
    },
    PushStarted {
        ordinal: usize,
    },
    PushResult {
        ordinal: usize,
        outcome: SubmissionOutcome,
    },
    Observed {
        ordinal: usize,
        state: LiveState,
        evidence: Box<MonitoringEvidence>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationOutbox {
    pub protocol: String,
    pub binding: ContinuationBinding,
    pub events: Vec<ContinuationEvent>,
}
impl ContinuationOutbox {
    pub fn new(binding: ContinuationBinding) -> Result<Self> {
        let value = Self {
            protocol: PROTOCOL.into(),
            binding,
            events: vec![],
        };
        value.validate_shape()?;
        Ok(value)
    }
    pub fn transactions(&self) -> Vec<&ContinuationTransaction> {
        self.events
            .iter()
            .filter_map(|event| match event {
                ContinuationEvent::Prepared { transaction } => Some(transaction),
                _ => None,
            })
            .collect()
    }
    pub fn pushed(&self, ordinal: usize) -> bool {
        self.events
            .iter()
            .any(|event| matches!(event,ContinuationEvent::PushStarted{ordinal:n} if *n==ordinal))
    }
    pub fn state(&self, ordinal: usize) -> Option<LiveState> {
        self.events.iter().rev().find_map(|event| match event {
            ContinuationEvent::Observed {
                ordinal: n, state, ..
            } if *n == ordinal => Some(*state),
            _ => None,
        })
    }
    pub fn follows(&self, previous: &Self) -> bool {
        self.protocol == previous.protocol
            && self.binding == previous.binding
            && self.events.starts_with(&previous.events)
    }
    pub fn validate_shape(&self) -> Result<()> {
        ensure!(
            self.protocol == PROTOCOL && self.events.len() <= 96,
            "continuation protocol/event bounds"
        );
        ensure!(
            (1..=8).contains(&self.binding.plan.recipients.len())
                && self.binding.plan.steps.len() == self.binding.plan.recipients.len(),
            "complete queue dimensions"
        );
        ensure!(
            self.binding.master_public_key.len() == 48
                && self.binding.funding_fee <= crate::live::wallet::MAX_FEE
                && self.binding.step_fee <= crate::live::wallet::MAX_FEE,
            "public identity/fee bounds"
        );
        let mut prepared = 0;
        let mut pushed = false;
        let mut confirmed = false;
        let mut result = false;
        for event in &self.events {
            match event {
                ContinuationEvent::Prepared { transaction } => {
                    ensure!(
                        prepared == 0 || confirmed,
                        "prior transaction must be observed confirmed"
                    );
                    let expected = if prepared == 0 {
                        ContinuationTransfer::Funding
                    } else {
                        ContinuationTransfer::Step(prepared - 1)
                    };
                    ensure!(
                        transaction.kind == expected
                            && prepared <= self.binding.plan.steps.len()
                            && transaction.signed_bundle.len() <= MAX_BUNDLE,
                        "ordered bounded transactions"
                    );
                    ensure!(
                        matches!(transaction.kind, ContinuationTransfer::Step(_))
                            || transaction.sponsor.is_none(),
                        "funding cannot have sponsor"
                    );
                    prepared += 1;
                    pushed = false;
                    confirmed = false;
                    result = false;
                }
                ContinuationEvent::PushStarted { ordinal } => {
                    ensure!(
                        prepared > 0 && *ordinal == prepared - 1 && !pushed && !confirmed,
                        "one push intent per prepared transaction"
                    );
                    pushed = true;
                }
                ContinuationEvent::PushResult { ordinal, .. } => {
                    ensure!(
                        prepared > 0 && *ordinal == prepared - 1 && pushed && !result,
                        "one outcome after push intent"
                    );
                    result = true;
                }
                ContinuationEvent::Observed {
                    ordinal,
                    state,
                    evidence,
                } => {
                    ensure!(
                        prepared > 0
                            && *ordinal == prepared - 1
                            && evidence.inputs.len() <= 2
                            && evidence.children.len() <= 3
                            && evidence.spends.len() <= 2,
                        "bounded exact current observation"
                    );
                    crate::live::check_receipts(&evidence.receipts)?;
                    for spend in evidence.spends.iter().flatten() {
                        ensure!(
                            spend.puzzle_reveal.len() <= MAX_BUNDLE
                                && spend.solution.len() <= MAX_BUNDLE,
                            "bounded observed programs"
                        );
                    }
                    confirmed = *state == LiveState::Confirmed;
                }
            }
        }
        Ok(())
    }
}
