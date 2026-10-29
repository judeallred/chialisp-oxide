//! WIP closed Testnet11 issuance. Persisted public intents do not confer source
//! authority. Every command repeats the actual Cargo/native/VM/SDK source gate.
//! Provider observations are consistency checks, not authenticated chain proofs.
use super::*;
use cat::{CatFees, CatMintPlan, CatMode, CatTailAuthority, VerifiedCatBundle};
use network::{LiveCoinRecord, NetworkIdentity};
use wallet::{Testnet11Network, VerifiedTransaction, WalletIdentity};
mod journal;
use journal::{
    CatBinding, CatEvent, CatOutbox, CatPrepared, CatStage, GenesisIntent, LiveState, PushOutcome,
};
mod store;
use store::CatStore;
mod observe;
use observe::{observe_state, record, require_network, same_record};
mod workflow;
use workflow::CatSession;
pub(super) mod cli;

const PROTOCOL: &str = "chialisp-oxide-testnet11-cat-live-outbox/v1";
const MAX_EVENTS: usize = 32;
const MAX_BUNDLE: usize = 64 * 1024;

/// Only an actual-process fresh source capability reaches this coordinator.
/// There is no persisted-report, module-byte or deserialization constructor.
struct CatRecipe {
    authority: CatTailAuthority,
    binding: CatBinding,
}
impl CatRecipe {
    fn new(
        authority: CatTailAuthority,
        master: [u8; 48],
        wallet: Digest,
        fee: u64,
        issuance_fee: u64,
    ) -> Result<Self> {
        let source = authority
            .source_identity()
            .context("CAT live requires actual fresh source authority")?;
        Self::with_source(authority, master, wallet, fee, issuance_fee, source)
    }
    fn with_source(
        authority: CatTailAuthority,
        master: [u8; 48],
        wallet: Digest,
        fee: u64,
        issuance_fee: u64,
        source: Digest,
    ) -> Result<Self> {
        WalletIdentity::from_master_public_key(master, wallet)?;
        CatFees::new(fee, issuance_fee)?;
        let mode = match authority
            .mode()
            .context("canonical probe is not live authority")?
        {
            CatMode::RustOff => "off",
            CatMode::RustOn => "on",
        };
        let binding = CatBinding {
            network: crate::provider::testnet11_network(),
            wallet_puzzle_hash: wallet,
            master_public_key_hex: hex::encode(master),
            source_identity: source,
            module_sha256: digest(authority.module_bytes()),
            module_hash: authority.module_hash(),
            mode: mode.into(),
            recipient: wallet,
            fee,
            issuance_fee,
        };
        Ok(Self { authority, binding })
    }
    fn identity(&self) -> Result<WalletIdentity> {
        Ok(WalletIdentity::from_master_public_key(
            decode_hex(&self.binding.master_public_key_hex, 48)?
                .as_slice()
                .try_into()?,
            self.binding.wallet_puzzle_hash,
        )?)
    }
    fn mint(
        &self,
        genesis: &CoinRecord,
        height: u32,
        token: &Testnet11Network,
    ) -> Result<CatMintPlan> {
        Ok(CatMintPlan::new(
            &self.authority,
            self.identity()?,
            genesis.coin(),
            self.binding.wallet_puzzle_hash,
            CatFees::new(self.binding.fee, self.binding.issuance_fee)?,
            height,
            token,
        )?)
    }
    fn new_outbox(&self) -> CatOutbox {
        CatOutbox {
            protocol: PROTOCOL.into(),
            binding: self.binding.clone(),
            events: vec![],
        }
    }
    fn require_binding(&self, outbox: &CatOutbox) -> Result<()> {
        crate::fresh::require_native_inputs()?;
        outbox.validate_shape()?;
        ensure!(
            outbox.binding == self.binding,
            "CAT current-source/wallet/fees/outbox mismatch"
        );
        Ok(())
    }
}
fn intent(mint: &CatMintPlan, genesis: CoinRecord) -> GenesisIntent {
    let child = mint.expected_post_eve_cat();
    let lineage = child.lineage();
    GenesisIntent {
        genesis,
        asset_id: mint.asset_id(),
        tail_hex: hex::encode(mint.tail_reveal()),
        eve: record(mint.eve_coin()),
        funding_change: record(mint.funding_plan().change_coin()),
        normal: record(child.coin()),
        normal_parent: record(child.parent_coin()),
        inner_puzzle_hash: child.inner_puzzle_hash(),
        lineage_parent_parent: lineage.parent_parent_coin_info.to_bytes(),
        lineage_inner: lineage.parent_inner_puzzle_hash.to_bytes(),
        lineage_amount: lineage.parent_amount,
    }
}
