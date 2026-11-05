//! Closed Testnet11 CAT prototype. WIP, not ready for use.
//!
//! Production source authority comes only from opaque actual-process Cargo and
//! native checks followed by executed SDK protocol admission. Local tests also
//! use explicitly labeled recorded bytes. A report or module hash cannot grant
//! source authority. No network/Sage API or real key is invoked here; the
//! separate purpose-specific wallet signer is not wired to the CLI.
use super::wallet::{Testnet11Network, WalletIdentity};
use chia_bls::{PublicKey, Signature};
use chia_consensus::{
    flags::MEMPOOL_MODE,
    solution_generator::calculate_generator_length,
    spendbundle_conditions::get_conditions_from_spendbundle,
    spendbundle_validation::{get_flags_for_height_and_constants, validate_clvm_and_signature},
    validation_error::ErrorCode,
};
use chia_protocol::{Coin, CoinSpend, SpendBundle};
use chia_sdk_signer::{AggSigConstants, RequiredBlsSignature};
use chia_sdk_types::{
    TESTNET11_CONSTANTS,
    conditions::{AggSig, AggSigKind},
};
use chia_traits::Streamable;
use clvmr_sdk::Allocator;
use std::fmt;

#[path = "cat/authority.rs"]
mod authority;
#[path = "cat/codec.rs"]
mod codec;
#[path = "cat/mint.rs"]
mod mint;
#[path = "cat/transfer.rs"]
mod transfer;
pub use authority::{CatMode, CatTailAuthority};
pub use mint::CatMintPlan;
pub use transfer::CatTransferPlan;
#[path = "cat/admission.rs"]
mod admission;
pub(crate) use admission::{CatSdkCheckReport, certify_fresh_source};

pub const CAT_AMOUNT: u64 = 1_000;
pub const DEFAULT_CAT_FEE: u64 = 10_000;
pub const MAX_CAT_FEE: u64 = 1_000_000;
const MAX_CAT_BUNDLE_BYTES: usize = 64 * 1024;
const MAX_CAT_MODULE_BYTES: usize = 16 * 1024;
const MAX_CODEC_HEAP: usize = 4 * 1024 * 1024;
// New CAT host-bundle ceiling, reviewed after actual recorded-artifact fixture
// measurements:73.69M Rust/off,52.20M Rust/on,41.83M canonical issuance.
// Existing25M standard-wallet and10M policy/compiler limits remain separate.
pub const MAX_CAT_BUNDLE_COST: u64 = 100_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatError {
    Height,
    Budget,
    SourceChanged,
    Identity,
    Fee,
    Amount,
    Coin,
    Asset,
    Lineage,
    Module,
    Codec,
    VmInconclusive(ErrorCode),
    Conditions,
    Signature,
    ChangedBundle,
    Consensus(ErrorCode),
}
impl fmt::Display for CatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "closed Testnet11 CAT {:?}", self)
    }
}
impl std::error::Error for CatError {}
pub type Result<T> = std::result::Result<T, CatError>;

fn vm_preflight_error(code: ErrorCode) -> CatError {
    match code {
        ErrorCode::CostExceeded => CatError::VmInconclusive(code),
        _ => CatError::Consensus(code),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CatFees {
    funding: u64,
    issuance: u64,
}
impl CatFees {
    pub fn new(funding: u64, issuance: u64) -> Result<Self> {
        if funding > MAX_CAT_FEE || issuance > MAX_CAT_FEE {
            return Err(CatError::Fee);
        }
        Ok(Self { funding, issuance })
    }
    pub fn funding(&self) -> u64 {
        self.funding
    }
    pub fn issuance(&self) -> u64 {
        self.issuance
    }
}

/// A planned normal child, including its exact parent-derived lineage. This is
/// not proof that funding or issuance was submitted/confirmed. The live parent
/// must independently observe the exact unspent coin before signing a transfer.
#[derive(Clone, Debug)]
pub struct NormalCatCoin {
    cat: chia_sdk_driver::Cat,
    genesis: [u8; 32],
    module_hash: [u8; 32],
    mode: Option<CatMode>,
    parent: Coin,
    parent_inner: [u8; 32],
}
impl NormalCatCoin {
    pub fn coin(&self) -> Coin {
        self.cat.coin
    }
    pub fn asset_id(&self) -> [u8; 32] {
        self.cat.info.asset_id.to_bytes()
    }
    pub fn inner_puzzle_hash(&self) -> [u8; 32] {
        self.cat.info.p2_puzzle_hash.to_bytes()
    }
    pub fn parent_coin(&self) -> Coin {
        self.parent
    }
    pub fn lineage(&self) -> chia_puzzle_types::LineageProof {
        self.cat
            .lineage_proof
            .expect("private normal CAT invariant")
    }
    fn check(&self, authority: &CatTailAuthority, owner: &WalletIdentity) -> Result<()> {
        if self.mode != authority.mode() || self.module_hash != authority.module_hash() {
            return Err(CatError::Asset);
        }
        let (_, asset) = codec::tail(authority, self.genesis)?;
        if self.asset_id() != asset || self.cat.info.hidden_puzzle_hash.is_some() {
            return Err(CatError::Asset);
        }
        if self.cat.coin.amount != CAT_AMOUNT
            || self.inner_puzzle_hash() != owner.wallet_puzzle_hash()
            || self.cat.coin.puzzle_hash != self.cat.info.puzzle_hash().into()
        {
            return Err(CatError::Coin);
        }
        let expected = chia_puzzle_types::LineageProof {
            parent_parent_coin_info: self.parent.parent_coin_info,
            parent_inner_puzzle_hash: self.parent_inner.into(),
            parent_amount: self.parent.amount,
        };
        if self.cat.coin.parent_coin_info != self.parent.coin_id()
            || self.cat.lineage_proof != Some(expected)
        {
            return Err(CatError::Lineage);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CatCosts {
    pub total: u64,
    pub execution: u64,
    pub conditions: u64,
    pub charged_generator_bytes: usize,
    pub cost_per_byte: u64,
    pub max_cost: u64,
    pub height: u32,
    pub bls_verified: bool,
}
#[derive(Clone, Debug)]
struct ExpectedOutput {
    coin: Coin,
    hint: Option<[u8; 32]>,
}

/// Opaque plan produced only by the sealed mint or transfer constructor. The
/// authorized wallet adapter may inspect public requirements but must freshly
/// requery its exact inputs and verify the immutable intent before key access.
pub struct CatSpendPlan {
    owner: WalletIdentity,
    spends: Vec<CoinSpend>,
    unsigned: Vec<u8>,
    expected: Vec<Vec<ExpectedOutput>>,
    requirements: Vec<RequiredBlsSignature>,
    costs: CatCosts,
    source_identity: Option<[u8; 32]>,
    fee: u64,
    asset: [u8; 32],
    child: NormalCatCoin,
}
impl CatSpendPlan {
    pub fn unsigned_bundle_bytes(&self) -> &[u8] {
        &self.unsigned
    }
    pub fn input_coins(&self) -> Vec<Coin> {
        self.spends.iter().map(|s| s.coin).collect()
    }
    pub fn expected_cat(&self) -> &NormalCatCoin {
        &self.child
    }
    pub fn fee_change(&self) -> Coin {
        self.expected[1][0].coin
    }
    pub fn asset_id(&self) -> [u8; 32] {
        self.asset
    }
    pub fn costs(&self) -> CatCosts {
        self.costs
    }
    pub fn fee(&self) -> u64 {
        self.fee
    }
    pub(super) fn owner_identity(&self) -> WalletIdentity {
        self.owner
    }
    pub(super) fn required_bls(&self) -> &[RequiredBlsSignature] {
        &self.requirements
    }
    /// Re-execute this exact closed intent before the authorized wallet opens
    /// a secret source. Coin equality and the caller's height are bindings, not
    /// independent proof that the caller actually requeried a live provider.
    pub(super) fn recheck_for_signing(
        &self,
        coins: &[Coin],
        current_height: u32,
        network: &Testnet11Network,
    ) -> Result<()> {
        self.require_requeried_inputs(coins, network)?;
        if self.source_identity.is_some() {
            crate::fresh::require_native_inputs().map_err(|_| CatError::SourceChanged)?;
        }
        if current_height != self.costs.height {
            return Err(CatError::Height);
        }
        if self.costs.max_cost != MAX_CAT_BUNDLE_COST || self.unsigned.len() > MAX_CAT_BUNDLE_BYTES
        {
            return Err(CatError::Budget);
        }
        let checked = Self::new(
            self.owner,
            self.spends.clone(),
            self.expected.clone(),
            self.fee,
            current_height,
            MAX_CAT_BUNDLE_COST,
            self.source_identity,
            self.asset,
            self.child.clone(),
        )?;
        if checked.unsigned != self.unsigned
            || checked.costs != self.costs
            || checked.requirements.len() != 2
            || self.requirements.len() != 2
        {
            return Err(CatError::Conditions);
        }
        if checked
            .requirements
            .iter()
            .zip(&self.requirements)
            .any(|(actual, stored)| {
                actual.public_key != stored.public_key || actual.message() != stored.message()
            })
        {
            return Err(CatError::Conditions);
        }
        Ok(())
    }

    pub(super) fn verify_returned_signature(
        &self,
        signature: &Signature,
    ) -> Result<VerifiedCatBundle> {
        let mut bytes = self.unsigned.clone();
        let offset = bytes.len() - 96;
        bytes[offset..].copy_from_slice(&signature.to_bytes());
        self.verify_signed_bundle(&bytes)
    }
    pub(super) fn require_requeried_inputs(
        &self,
        coins: &[Coin],
        network: &Testnet11Network,
    ) -> Result<()> {
        check_network(network)?;
        if coins.len() != self.spends.len()
            || coins.iter().zip(&self.spends).any(|(a, b)| a != &b.coin)
        {
            return Err(CatError::Coin);
        }
        Ok(())
    }
    /// Public persisted bytes never confer source authority. Rebuild this plan
    /// from the sealed actual-process source capability first, then compare its
    /// complete fixed prefix before inspecting a single signature point.
    pub fn verify_signed_bundle(&self, raw: &[u8]) -> Result<VerifiedCatBundle> {
        let prefix = self.unsigned.len().checked_sub(96).ok_or(CatError::Codec)?;
        if raw.len() != self.unsigned.len()
            || raw.len() > MAX_CAT_BUNDLE_BYTES
            || raw[..prefix] != self.unsigned[..prefix]
        {
            return Err(CatError::ChangedBundle);
        }
        let sig = Signature::from_bytes(raw[prefix..].try_into().map_err(|_| CatError::Signature)?)
            .map_err(|_| CatError::Signature)?;
        if sig.to_bytes().as_slice() != &raw[prefix..] {
            return Err(CatError::Signature);
        }
        let bundle = SpendBundle::new(self.spends.clone(), sig);
        let flags = get_flags_for_height_and_constants(self.costs.height, &TESTNET11_CONSTANTS)
            | MEMPOOL_MODE;
        let (actual, _) =
            validate_clvm_and_signature(&bundle, self.costs.max_cost, &TESTNET11_CONSTANTS, flags)
                .map_err(|e| vm_preflight_error(e.1))?;
        if actual.spends.len() != 2
            || actual.reserve_fee != self.fee
            || !actual.agg_sig_unsafe.is_empty()
        {
            return Err(CatError::Conditions);
        }
        let (removed, added) = self.amounts();
        if actual.removal_amount != removed
            || actual.addition_amount != added
            || removed - added != u128::from(self.fee)
        {
            return Err(CatError::Amount);
        }
        for (spend, expected) in self.spends.iter().zip(&self.expected) {
            let observed = actual
                .spends
                .iter()
                .find(|r| r.coin_id == spend.coin.coin_id())
                .ok_or(CatError::Conditions)?;
            let mut a = observed
                .create_coin
                .iter()
                .map(|(hash, amount, hint)| {
                    (
                        (*hash).to_bytes(),
                        *amount,
                        hint.as_ref().map(|h| h.to_vec()),
                    )
                })
                .collect::<Vec<_>>();
            let mut b = expected
                .iter()
                .map(|r| {
                    (
                        r.coin.puzzle_hash.to_bytes(),
                        r.coin.amount,
                        r.hint.map(|h| h.to_vec()),
                    )
                })
                .collect::<Vec<_>>();
            a.sort();
            b.sort();
            if a != b {
                return Err(CatError::Conditions);
            }
        }
        if actual.cost != self.costs.total
            || actual.execution_cost != self.costs.execution
            || actual.condition_cost != self.costs.conditions
        {
            return Err(CatError::Conditions);
        }
        let mut costs = self.costs;
        costs.bls_verified = true;
        Ok(VerifiedCatBundle {
            bytes: raw.to_vec(),
            bundle,
            costs,
            asset: self.asset,
            child: self.child.clone(),
            fee_change: self.fee_change(),
        })
    }
    fn amounts(&self) -> (u128, u128) {
        (
            self.spends.iter().map(|s| u128::from(s.coin.amount)).sum(),
            self.expected
                .iter()
                .flatten()
                .map(|r| u128::from(r.coin.amount))
                .sum(),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn new(
        owner: WalletIdentity,
        spends: Vec<CoinSpend>,
        expected: Vec<Vec<ExpectedOutput>>,
        fee: u64,
        height: u32,
        max_cost: u64,
        source_identity: Option<[u8; 32]>,
        asset: [u8; 32],
        child: NormalCatCoin,
    ) -> Result<Self> {
        if spends.len() != 2
            || expected.len() != 2
            || spends[0].coin.coin_id() == spends[1].coin.coin_id()
        {
            return Err(CatError::Coin);
        }
        let unsigned = SpendBundle::new(spends.clone(), Signature::default())
            .to_bytes()
            .map_err(|_| CatError::Codec)?;
        if unsigned.len() > MAX_CAT_BUNDLE_BYTES {
            return Err(CatError::Codec);
        }
        let bundle = SpendBundle::new(spends.clone(), Signature::default());
        let mut allocator = Allocator::new_limited(MAX_CODEC_HEAP);
        let actual = get_conditions_from_spendbundle(
            &mut allocator,
            &bundle,
            max_cost,
            height,
            &TESTNET11_CONSTANTS,
        )
        .map_err(|e| vm_preflight_error(e.1))?;
        if actual.spends.len() != 2
            || actual.reserve_fee != fee
            || !actual.agg_sig_unsafe.is_empty()
        {
            return Err(CatError::Conditions);
        }
        let removed: u128 = spends.iter().map(|s| u128::from(s.coin.amount)).sum();
        let added: u128 = expected
            .iter()
            .flatten()
            .map(|r| u128::from(r.coin.amount))
            .sum();
        if actual.removal_amount != removed
            || actual.addition_amount != added
            || removed.checked_sub(added) != Some(u128::from(fee))
        {
            return Err(CatError::Amount);
        }
        let synthetic =
            PublicKey::from_bytes(&owner.synthetic_public_key()).map_err(|_| CatError::Identity)?;
        let constants = AggSigConstants::new(TESTNET11_CONSTANTS.agg_sig_me_additional_data);
        let mut requirements = vec![];
        for (spend, expected) in spends.iter().zip(&expected) {
            let observed = actual
                .spends
                .iter()
                .find(|r| *r.coin_id == spend.coin.coin_id())
                .ok_or(CatError::Conditions)?;
            if observed.agg_sig_me.len() != 1
                || !observed.agg_sig_parent.is_empty()
                || !observed.agg_sig_puzzle.is_empty()
                || !observed.agg_sig_amount.is_empty()
                || !observed.agg_sig_puzzle_amount.is_empty()
                || !observed.agg_sig_parent_amount.is_empty()
                || !observed.agg_sig_parent_puzzle.is_empty()
            {
                return Err(CatError::Conditions);
            }
            let (key, message) = observed.agg_sig_me[0];
            let bytes = allocator.atom(message);
            if key != synthetic || key.is_inf() || bytes.as_ref().len() != 32 {
                return Err(CatError::Identity);
            }
            requirements.push(RequiredBlsSignature::from_condition(
                &spend.coin,
                AggSig::new(AggSigKind::Me, key, bytes.as_ref().to_vec().into()),
                &constants,
            ));
            let mut a = observed
                .create_coin
                .iter()
                .map(|r| {
                    (
                        r.puzzle_hash.to_bytes(),
                        r.amount,
                        if r.hint == allocator.nil() {
                            None
                        } else {
                            Some(allocator.atom(r.hint).as_ref().to_vec())
                        },
                    )
                })
                .collect::<Vec<_>>();
            let mut b = expected
                .iter()
                .map(|r| {
                    (
                        r.coin.puzzle_hash.to_bytes(),
                        r.coin.amount,
                        r.hint.map(|h| h.to_vec()),
                    )
                })
                .collect::<Vec<_>>();
            a.sort();
            b.sort();
            if a != b {
                return Err(CatError::Conditions);
            }
        }
        let charged_generator_bytes = calculate_generator_length(&spends)
            .checked_sub(2)
            .ok_or(CatError::Codec)?;
        if actual.cost
            != (charged_generator_bytes as u64) * TESTNET11_CONSTANTS.cost_per_byte
                + actual.execution_cost
                + actual.condition_cost
        {
            return Err(CatError::Conditions);
        }
        let costs = CatCosts {
            total: actual.cost,
            execution: actual.execution_cost,
            conditions: actual.condition_cost,
            charged_generator_bytes,
            cost_per_byte: TESTNET11_CONSTANTS.cost_per_byte,
            max_cost,
            height,
            bls_verified: false,
        };
        Ok(Self {
            owner,
            spends,
            unsigned,
            expected,
            requirements,
            costs,
            source_identity,
            fee,
            asset,
            child,
        })
    }
}

/// Not deserializable; only complete actual BLS/consensus verification produces
/// this public value. It is neither a submission nor a chain observation.
pub struct VerifiedCatBundle {
    bytes: Vec<u8>,
    bundle: SpendBundle,
    costs: CatCosts,
    asset: [u8; 32],
    child: NormalCatCoin,
    fee_change: Coin,
}
impl VerifiedCatBundle {
    pub fn bundle_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn bundle(&self) -> &SpendBundle {
        &self.bundle
    }
    pub fn transaction_id(&self) -> [u8; 32] {
        self.bundle.name().to_bytes()
    }
    pub fn costs(&self) -> CatCosts {
        self.costs
    }
    pub fn asset_id(&self) -> [u8; 32] {
        self.asset
    }
    pub fn normal_cat(&self) -> &NormalCatCoin {
        &self.child
    }
    pub fn fee_change(&self) -> Coin {
        self.fee_change
    }
}
fn check_network(_network: &Testnet11Network) -> Result<()> {
    // The sealed wallet token has already checked provider labels separately
    // from pinned local constants. It has no public/raw constructor.
    Ok(())
}
fn fee_change(owner: &WalletIdentity, coin: Coin, fee: u64) -> Result<Coin> {
    if coin.puzzle_hash.to_bytes() != owner.wallet_puzzle_hash() {
        return Err(CatError::Coin);
    }
    if fee > MAX_CAT_FEE {
        return Err(CatError::Fee);
    }
    let amount = coin.amount.checked_sub(fee).ok_or(CatError::Amount)?;
    Ok(Coin::new(
        coin.coin_id(),
        owner.wallet_puzzle_hash().into(),
        amount,
    ))
}
#[cfg(test)]
#[path = "cat/tests.rs"]
mod tests;
