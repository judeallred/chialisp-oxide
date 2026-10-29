//! Compact public GUI projection. Amounts and costs retain exact decimal text.
use super::*;

#[derive(Clone, Serialize)]
pub struct LiveView {
    pub protocol: &'static str,
    pub network: &'static str,
    pub revision: String,
    pub wallet_puzzle_hash: String,
    pub funding_puzzle_hash: String,
    pub reveal_sha256: String,
    pub mode: &'static str,
    pub fresh_execution: &'static str,
    pub amount_mojos: String,
    pub funding_fee_mojos: String,
    pub payment_fee_mojos: String,
    pub scope: &'static str,
    pub funding: TransferView,
    pub payment: TransferView,
    pub available_actions: Vec<&'static str>,
    pub event_count: usize,
    pub remaining_events: usize,
}

#[derive(Clone, Serialize)]
pub struct TransferView {
    pub prepared: bool,
    pub state: LiveState,
    pub bundle_id: Option<String>,
    pub bundle_sha256: Option<String>,
    pub preflight_height: Option<String>,
    pub cost: Option<CostView>,
    pub inputs: Vec<CoinView>,
    pub outputs: Vec<CoinView>,
    pub latest_observation: Option<ObservationView>,
    pub provider_outcome: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct CostView {
    pub total: String,
    pub execution: String,
    pub conditions: String,
    pub charged_generator_bytes: String,
    pub byte_cost: String,
}

#[derive(Clone, Serialize)]
pub struct CoinView {
    pub coin_id: String,
    pub parent: String,
    pub puzzle_hash: String,
    pub amount_mojos: String,
}
impl From<&CoinRecord> for CoinView {
    fn from(coin: &CoinRecord) -> Self {
        Self {
            coin_id: hex::encode(coin.id()),
            parent: hex::encode(coin.parent),
            puzzle_hash: hex::encode(coin.puzzle_hash),
            amount_mojos: coin.amount.to_string(),
        }
    }
}

#[derive(Clone, Serialize)]
pub struct ObservationView {
    pub tip_height: String,
    pub spent_height: Option<String>,
    pub scope: &'static str,
}

impl TransferView {
    fn from_outbox(outbox: &LiveOutbox, kind: TransferKind) -> Result<Self> {
        let transfer = outbox.transfer(kind);
        let observation = outbox.events().iter().rev().find_map(|event| match event {
            LiveEvent::Observed { kind: actual, evidence, state, .. } if *actual == kind => {
                let spent_height = if *state == LiveState::Confirmed {
                    evidence.inputs.first().and_then(Option::as_ref)
                        .map(|coin| coin.spent_height.to_string())
                } else { None };
                Some(ObservationView { tip_height: evidence.after.height.to_string(), spent_height,
                    scope: "Last saved provider observation. Monitor refreshes exact spend bytes and expected child points; no authenticated inclusion or finality proof." })
            }
            _ => None,
        });
        let outcome = outbox.events().iter().rev().find_map(|event| match event {
            LiveEvent::PushResult {
                kind: actual,
                outcome,
                ..
            } if *actual == kind => Some(match outcome {
                PushOutcome::Provider { outcome } => match outcome {
                    network::SubmissionOutcome::MempoolAccepted { .. } => "mempool_accepted".into(),
                    network::SubmissionOutcome::Pending { .. } => "pending".into(),
                    network::SubmissionOutcome::Rejected { provider_code, .. } => {
                        format!("provider_rejected:{provider_code}")
                    }
                },
                PushOutcome::Inconclusive { kind } => format!("inconclusive:{kind:?}"),
            }),
            _ => None,
        });
        let cost = transfer
            .map(|value| -> Result<CostView> {
                let byte_cost = value
                    .charged_generator_bytes
                    .checked_mul(12_000)
                    .context("GUI byte charge overflow")?;
                Ok(CostView {
                    total: value.cost.to_string(),
                    execution: value.execution_cost.to_string(),
                    conditions: value.condition_cost.to_string(),
                    charged_generator_bytes: value.charged_generator_bytes.to_string(),
                    byte_cost: byte_cost.to_string(),
                })
            })
            .transpose()?;
        Ok(Self {
            prepared: transfer.is_some(),
            state: outbox.state(kind),
            bundle_id: transfer.map(|value| hex::encode(value.bundle_id)),
            bundle_sha256: transfer.map(|value| hex::encode(value.bundle_sha256)),
            preflight_height: transfer.map(|value| value.preflight_height.to_string()),
            cost,
            inputs: transfer
                .map(|value| value.inputs.iter().map(CoinView::from).collect())
                .unwrap_or_default(),
            outputs: transfer
                .map(|value| value.children.iter().map(CoinView::from).collect())
                .unwrap_or_default(),
            latest_observation: observation,
            provider_outcome: outcome,
        })
    }
}

impl LiveView {
    pub fn from_session(session: &LiveSession) -> Result<Self> {
        let (recipe, outbox, revision) = session.view_material()?;
        let funding = TransferView::from_outbox(outbox, TransferKind::Funding)?;
        let payment = TransferView::from_outbox(outbox, TransferKind::Payment)?;
        let mut actions = vec!["reload"];
        if !funding.prepared {
            actions.push("prepare_funding");
        } else {
            actions.push("monitor_funding");
            if funding.latest_observation.is_none() || funding.state == LiveState::Unspent {
                actions.push("submit_funding");
            }
        }
        if funding.state == LiveState::Confirmed && !payment.prepared {
            actions.push("prepare_payment");
        }
        if payment.prepared {
            actions.push("monitor_payment");
            if payment.latest_observation.is_none() || payment.state == LiveState::Unspent {
                actions.push("submit_payment");
            }
        }
        ensure!(
            matches!(recipe.mode, "on" | "off") || cfg!(test),
            "unknown GUI recipe mode"
        );
        Ok(Self {
            protocol: "chialisp-oxide-testnet11-live-gui-view/v1",
            network: "testnet11",
            revision: hex::encode(revision),
            wallet_puzzle_hash: hex::encode(recipe.binding.wallet_puzzle_hash),
            funding_puzzle_hash: hex::encode(recipe.funding_puzzle_hash()?),
            reveal_sha256: hex::encode(digest(&recipe.artifact.reveal)),
            mode: recipe.mode,
            fresh_execution: if matches!(recipe.mode, "on" | "off") {
                "executed_this_process"
            } else {
                "constructed_test_control"
            },
            amount_mojos: recipe.plan.amount.to_string(),
            funding_fee_mojos: recipe.binding.fee.to_string(),
            payment_fee_mojos: recipe.binding.payment_fee.to_string(),
            scope: "WIP Testnet11 prototype. Source checks executed at process startup; every action rechecks source and exact intent. Displayed spend states are saved provider observations; use Monitor for a new observation. Not ready for mainnet use.",
            funding,
            payment,
            available_actions: actions,
            event_count: outbox.events().len(),
            remaining_events: MAX_EVENTS.saturating_sub(outbox.events().len()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coin_projection_preserves_u64_without_javascript_rounding() {
        let coin = CoinRecord {
            parent: [0x11; 32],
            puzzle_hash: [0x22; 32],
            amount: u64::MAX,
        };
        let value = serde_json::to_value(CoinView::from(&coin)).unwrap();
        assert_eq!(value["amount_mojos"], "18446744073709551615");
        assert_eq!(value["coin_id"].as_str().unwrap().len(), 64);
        assert!(value.get("bundle_hex").is_none());
    }
}
