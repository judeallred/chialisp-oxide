//! WIP explicit Testnet11 live operations. Public outboxes contain no keys.
//! A current-process fresh recipe and current provider observations are required;
//! retained JSON is an intent/history record, never execution authentication.
mod bundle;
pub(crate) mod cat;
mod cat_live;
use bundle::decode_bundle;
mod journal;
pub use journal::*;
pub mod http;
mod outbox;
mod view;
pub use outbox::{CommitError, LiveStore, StoredOutbox};
pub use view::LiveView;
pub mod cli;
mod workflow;
pub use workflow::{LiveNetwork, LiveSession, MonitoringEvidence, observe_state};

pub mod network;
pub mod wallet;

use crate::{Artifact, CoinRecord, Digest, Plan, artifact_digest, digest};
use anyhow::{Context, Result, ensure};
use chia_consensus::{
    flags::MEMPOOL_MODE,
    solution_generator::calculate_generator_length,
    spendbundle_validation::{get_flags_for_height_and_constants, validate_clvm_and_signature},
};
use chia_protocol::{CoinSpend, Program, SpendBundle};
use chia_sdk_types::TESTNET11_CONSTANTS;
use chia_traits::Streamable;
use serde::{Deserialize, Serialize};

const PROTOCOL: &str = "chialisp-oxide-testnet11-live-outbox/v1";
pub const DEFAULT_FEE: u64 = 10_000;
pub const MAX_FEE: u64 = 1_000_000;
const MAX_BUNDLE: usize = 128 * 1024;
const MAX_COST: u64 = 10_000_000;
const MAX_EVENTS: usize = 32;

fn check_receipts(receipts: &[network::HttpReceipt]) -> Result<()> {
    ensure!(receipts.len() <= 32, "operation HTTP receipt bound");
    for receipt in receipts {
        ensure!(
            receipt.request_bytes <= network::MAX_PUSH_BYTES
                && receipt.response_bytes <= network::MAX_RESPONSE_BYTES,
            "HTTP receipt byte bounds"
        );
        ensure!(
            decode_hex(&receipt.request_sha256, 32)?.len() == 32
                && decode_hex(&receipt.response_sha256, 32)?.len() == 32,
            "HTTP receipt digest dimensions"
        );
    }
    Ok(())
}

fn decode_hex(value: &str, maximum: usize) -> Result<Vec<u8>> {
    ensure!(
        value.len() <= maximum.checked_mul(2).context("hex bound overflow")?
            && value.len().is_multiple_of(2)
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "bounded lowercase public hex"
    );
    Ok(hex::decode(value)?)
}

/// Process-local capability copied only from actual fresh execution.
#[derive(Clone)]
pub struct LiveRecipe {
    plan: Plan,
    artifact: Artifact,
    binding: LiveBinding,
    mode: &'static str,
}
impl LiveRecipe {
    pub fn from_fresh(
        recipe: &crate::runner::ValidatedRecipe,
        master_public_key: [u8; 48],
        wallet_puzzle_hash: Digest,
        fee: u64,
        payment_fee: u64,
    ) -> Result<Self> {
        let (plan, artifact, checks) = recipe.live_material()?;
        let mode = recipe.live_mode()?;
        ensure!(
            plan.network == crate::provider::testnet11_network()
                && plan.amount == 1000
                && plan.outputs.len() == 1
                && plan.outputs[0].amount == 1000
                && plan.outputs[0].puzzle_hash == wallet_puzzle_hash
                && artifact.solution == [0x80],
            "live recipe must be the checked 1000-mojo payment back to this wallet"
        );
        ensure!(fee <= MAX_FEE && payment_fee <= MAX_FEE, "live fee cap");
        wallet::WalletIdentity::from_master_public_key(master_public_key, wallet_puzzle_hash)?;
        let binding = LiveBinding {
            network: plan.network.clone(),
            wallet_puzzle_hash,
            master_public_key_hex: hex::encode(master_public_key),
            artifact_digest: artifact_digest(&artifact)?,
            native_check_digest: digest(&checks.native_receipt),
            clvm_check_digest: digest(&checks.clvm_receipt),
            recipient: wallet_puzzle_hash,
            fee,
            payment_fee,
        };
        Ok(Self {
            plan,
            artifact,
            binding,
            mode,
        })
    }
    pub fn new_outbox(&self) -> LiveOutbox {
        LiveOutbox {
            protocol: PROTOCOL.into(),
            binding: self.binding.clone(),
            events: vec![],
        }
    }
    pub fn funding_puzzle_hash(&self) -> Result<Digest> {
        self.artifact.puzzle_hash()
    }
    pub fn binding(&self) -> &LiveBinding {
        &self.binding
    }
    pub fn require_binding(&self, outbox: &LiveOutbox) -> Result<()> {
        crate::fresh::require_native_inputs()?;
        outbox.validate_shape()?;
        ensure!(
            outbox.binding == self.binding,
            "fresh recipe/wallet/outbox mismatch"
        );
        Ok(())
    }
    pub fn payment(&self, input: CoinRecord, height: u32) -> Result<PreparedTransfer> {
        crate::fresh::require_native_inputs()?;
        ensure!(
            input.puzzle_hash == self.funding_puzzle_hash()? && input.amount == 1000,
            "payment input must be exact checked recipe coin"
        );
        let bundle = SpendBundle::new(
            vec![CoinSpend::new(
                input.coin(),
                Program::from(self.artifact.reveal.as_slice()),
                Program::from(self.artifact.solution.as_slice()),
            )],
            Default::default(),
        );
        let flags = get_flags_for_height_and_constants(height, &TESTNET11_CONSTANTS) | MEMPOOL_MODE;
        let (conditions, _) =
            validate_clvm_and_signature(&bundle, MAX_COST, &TESTNET11_CONSTANTS, flags).map_err(
                |_| anyhow::anyhow!("live payment SDK CLVM/signature preflight rejected"),
            )?;
        ensure!(
            conditions.spends.len() == 1
                && conditions.removal_amount == 1000
                && conditions.addition_amount == 1000,
            "payment exact input/output totals"
        );
        let outputs = &conditions.spends[0].create_coin;
        ensure!(
            outputs.len() == 1
                && outputs[0].0.to_bytes() == self.plan.outputs[0].puzzle_hash
                && outputs[0].1 == 1000,
            "payment exact complete child set"
        );
        let child = CoinRecord {
            parent: input.id(),
            puzzle_hash: self.plan.outputs[0].puzzle_hash,
            amount: 1000,
        };
        let bytes = bundle.to_bytes()?;
        ensure!(bytes.len() <= MAX_BUNDLE, "payment bundle bound");
        Ok(PreparedTransfer {
            kind: TransferKind::Payment,
            inputs: vec![input],
            children: vec![child],
            fee: 0,
            bundle_hex: hex::encode(&bytes),
            bundle_id: bundle.name().to_bytes(),
            bundle_sha256: digest(&bytes),
            preflight_height: height,
            cost: conditions.cost,
            execution_cost: conditions.execution_cost,
            condition_cost: conditions.condition_cost,
            charged_generator_bytes: calculate_generator_length(&bundle.coin_spends)
                .checked_sub(2)
                .context("plain generator quote charge")?,
        })
    }
    pub fn verify_payment(&self, transfer: &PreparedTransfer, height: u32) -> Result<()> {
        ensure!(
            transfer.kind == TransferKind::Payment && transfer.inputs.len() == 1,
            "single-input zero-fee payment intent"
        );
        let expected = self.payment(transfer.inputs[0].clone(), height)?;
        ensure!(
            expected.bundle_hex == transfer.bundle_hex
                && expected.bundle_id == transfer.bundle_id
                && expected.bundle_sha256 == transfer.bundle_sha256
                && expected.children == transfer.children
                && transfer.fee == 0
                && transfer.cost
                    == self
                        .payment(transfer.inputs[0].clone(), transfer.preflight_height)?
                        .cost,
            "retained payment differs from exact fresh recipe"
        );
        ensure!(
            expected.execution_cost == transfer.execution_cost
                && expected.condition_cost == transfer.condition_cost
                && expected.charged_generator_bytes == transfer.charged_generator_bytes,
            "retained payment cost components"
        );
        Ok(())
    }
}
