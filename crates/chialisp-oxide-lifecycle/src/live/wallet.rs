//! Narrow Testnet11 standard-wallet transactions. WIP, not ready for use.
//!
//! This module signs only its own immutable funding or explicit fee-sponsor
//! plans. It provides no arbitrary-message or arbitrary-spend signing API.
//! Public provider declarations and local validation do not authenticate chain
//! inclusion, coin availability, compiler execution or finality. The live runner
//! must freshly requery every exact input before invoking the hidden TTY signer.
use chia_bls::{PublicKey, SecretKey, Signature, master_to_wallet_unhardened};
use chia_consensus::{
    flags::MEMPOOL_MODE,
    solution_generator::calculate_generator_length,
    spendbundle_validation::{get_flags_for_height_and_constants, validate_clvm_and_signature},
    validation_error::ErrorCode,
};
use chia_protocol::{Coin, CoinSpend, Program, SpendBundle};
use chia_puzzle_types::{
    DeriveSynthetic, Memos,
    standard::{StandardArgs, StandardSolution},
};
use chia_sdk_signer::{AggSigConstants, RequiredBlsSignature};
use chia_sdk_types::{
    Conditions, TESTNET11_CONSTANTS,
    conditions::{AggSigKind, AssertConcurrentSpend, AssertMyAmount, CreateCoin, ReserveFee},
};
use chia_traits::Streamable;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, tree_hash};
use clvmr_sdk::{
    Allocator, ChiaDialect, NodePtr, run_program,
    serde::{node_from_bytes, node_to_bytes},
};
use std::fmt;

#[path = "wallet/tty.rs"]
mod tty;

pub const PAYMENT_AMOUNT: u64 = 1_000;
pub const DEFAULT_FEE: u64 = 10_000;
pub const MAX_FEE: u64 = 1_000_000;
pub const MAX_POLICY_VM_COST: u64 = 10_000_000;
// Separate host-wallet total ceiling: actual canonical funding measured10.18M
// and the quoted payment sponsor11.21M, including12,000cost units per
// charged generator byte. This does not change any compiler or policy VM cap;
// larger fresh recipe artifacts must pass under this fixed ceiling as well.
pub const MAX_WALLET_BUNDLE_COST: u64 = 25_000_000;
pub const MAX_PAYMENT_REVEAL: usize = 16 * 1024;
const MAX_BUNDLE: usize = 64 * 1024;
const VM_HEAP: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletError {
    NetworkMismatch,
    InvalidPublicKey,
    WalletHashMismatch,
    InputMismatch,
    FeeBound,
    AmountBound,
    DuplicateOutput,
    ProgramBuild,
    VmFailure,
    ConditionMismatch,
    SignatureEncoding,
    ChangedBundle,
    Consensus(ErrorCode),
    SecretInput,
    KeyMismatch,
    TtyUnavailable,
}
impl fmt::Display for WalletError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Only closed, nonsensitive codes reach errors. In particular upstream
        // SecretKey Debug prints the private bytes and must never be used here.
        write!(output, "Testnet11 wallet {:?}", self)
    }
}
impl std::error::Error for WalletError {}
pub type Result<T> = std::result::Result<T, WalletError>;

#[derive(Clone, Copy, Debug)]
pub struct Testnet11Network {
    genesis: [u8; 32],
}
impl Testnet11Network {
    /// Check only the provider's labels. Coinset does not supply a genesis
    /// challenge here: the genesis/domain are pinned local constants, never
    /// invented provider fields or authenticated remote chain identity.
    pub fn from_provider_labels(name: &str, prefix: &str) -> Result<Self> {
        if name != "testnet11" || prefix != "txch" {
            return Err(WalletError::NetworkMismatch);
        }
        Ok(Self {
            genesis: TESTNET11_CONSTANTS.genesis_challenge.to_bytes(),
        })
    }
    fn check(&self) -> Result<()> {
        if self.genesis != TESTNET11_CONSTANTS.genesis_challenge.to_bytes() {
            return Err(WalletError::NetworkMismatch);
        }
        Ok(())
    }
}

/// Public identity for one reviewed role/path, never a master secret.
#[derive(Clone, Copy, Debug)]
pub struct WalletIdentity {
    master: PublicKey,
    synthetic: PublicKey,
    puzzle_hash: [u8; 32],
}
impl WalletIdentity {
    /// The caller separately validates the txch address checksum/prefix. This
    /// function binds its exact decoded hash to the fixed all-unhardened
    /// m/12381/8444/2/0 public derivation and default hidden puzzle.
    pub fn from_master_public_key(
        master: [u8; 48],
        expected_wallet_hash: [u8; 32],
    ) -> Result<Self> {
        let master = PublicKey::from_bytes(&master).map_err(|_| WalletError::InvalidPublicKey)?;
        if master.is_inf() {
            return Err(WalletError::InvalidPublicKey);
        }
        let synthetic = master_to_wallet_unhardened(&master, 0).derive_synthetic();
        if synthetic.is_inf() {
            return Err(WalletError::InvalidPublicKey);
        }
        let puzzle_hash = StandardArgs::curry_tree_hash(synthetic).to_bytes();
        if puzzle_hash != expected_wallet_hash {
            return Err(WalletError::WalletHashMismatch);
        }
        Ok(Self {
            master,
            synthetic,
            puzzle_hash,
        })
    }
    pub fn master_public_key(&self) -> [u8; 48] {
        self.master.to_bytes()
    }
    pub fn synthetic_public_key(&self) -> [u8; 48] {
        self.synthetic.to_bytes()
    }
    pub fn wallet_puzzle_hash(&self) -> [u8; 32] {
        self.puzzle_hash
    }
}

/// Not Clone, Serialize or Debug: upstream SecretKey's Debug exposes its bytes.
struct MasterSecret(SecretKey);
impl MasterSecret {
    fn synthetic_for(&self, identity: &WalletIdentity) -> Result<SecretKey> {
        if self.0.public_key() != identity.master {
            return Err(WalletError::KeyMismatch);
        }
        let synthetic = master_to_wallet_unhardened(&self.0, 0).derive_synthetic();
        if synthetic.public_key() != identity.synthetic {
            return Err(WalletError::KeyMismatch);
        }
        Ok(synthetic)
    }
}

struct TransactionPlan {
    identity: WalletIdentity,
    spends: Vec<CoinSpend>,
    unsigned: Vec<u8>,
    requirement: RequiredBlsSignature,
    outputs: Vec<Vec<Coin>>,
    payment: Coin,
    change: Coin,
    fee: u64,
    height: u32,
}

/// Exactly one standard-wallet input funds a 1,000-mojo checked puzzle.
/// Every remaining mojo except the bounded explicit fee returns to this wallet.
pub struct FundingPlan(TransactionPlan);
impl FundingPlan {
    pub fn new(
        identity: WalletIdentity,
        input: Coin,
        destination: [u8; 32],
        fee: u64,
        height: u32,
        network: &Testnet11Network,
    ) -> Result<Self> {
        network.check()?;
        let change_amount = checked_change(&identity, input, PAYMENT_AMOUNT, fee)?;
        if destination == identity.puzzle_hash && change_amount == PAYMENT_AMOUNT {
            return Err(WalletError::DuplicateOutput);
        }
        let payment = Coin::new(input.coin_id(), destination.into(), PAYMENT_AMOUNT);
        let change = Coin::new(input.coin_id(), identity.puzzle_hash.into(), change_amount);
        let delegated = Conditions::new()
            .with(CreateCoin::new(
                destination.into(),
                PAYMENT_AMOUNT,
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
    pub fn input_coin(&self) -> Coin {
        self.0.spends[0].coin
    }
    pub fn payment_coin(&self) -> Coin {
        self.0.payment
    }
    pub fn change_coin(&self) -> Coin {
        self.0.change
    }
    pub fn unsigned_bundle_bytes(&self) -> &[u8] {
        &self.0.unsigned
    }
    pub fn verify_signed_bundle(&self, bytes: &[u8]) -> Result<VerifiedTransaction> {
        self.0.verify(bytes)
    }
    /// Local fixture-only signer; absent from every production build.
    #[cfg(test)]
    pub(crate) fn sign_fixture(&self, fixture_seed: [u8; 32]) -> Result<VerifiedTransaction> {
        self.0
            .sign(&MasterSecret(SecretKey::from_seed(&fixture_seed)))
    }
    pub fn sign_hidden_tty(
        &self,
        requeried_input: Coin,
        network: &Testnet11Network,
    ) -> Result<VerifiedTransaction> {
        self.0.require_inputs(&[requeried_input], network)?;
        self.0.sign(&tty::read_master_secret()?)
    }
}

/// A separate two-input transaction: the unchanged permissionless 1,000-mojo
/// payment plus one standard-wallet fee input. It does not broaden the existing
/// single-input payment journal or grant fresh-source execution credit.
pub struct PaymentSponsorPlan(TransactionPlan);
impl PaymentSponsorPlan {
    #[allow(clippy::too_many_arguments)]
    /// Trusted live-parent boundary: `payment_reveal` MUST come from the
    /// private current-process fresh fixed-pay recipe with its exact strict
    /// nullary ABI. Executing nil alone cannot establish this property for an
    /// arbitrary program: another solution might redirect the payment while
    /// the sponsor's concurrent-spend assertion still succeeds. Never expose
    /// this constructor through imported reports or caller-selected programs.
    pub(super) fn new(
        identity: WalletIdentity,
        payment: Coin,
        payment_reveal: &[u8],
        sponsor: Coin,
        fee: u64,
        height: u32,
        network: &Testnet11Network,
    ) -> Result<Self> {
        network.check()?;
        if payment.amount != PAYMENT_AMOUNT || payment.coin_id() == sponsor.coin_id() {
            return Err(WalletError::InputMismatch);
        }
        let expected_payment = Conditions::new()
            .with(AssertMyAmount::new(PAYMENT_AMOUNT))
            .with(CreateCoin::new(
                identity.puzzle_hash.into(),
                PAYMENT_AMOUNT,
                Memos::<NodePtr>::None,
            ));
        validate_payment(payment, payment_reveal, &expected_payment)?;
        let change_amount = checked_change(&identity, sponsor, 0, fee)?;
        let output = Coin::new(
            payment.coin_id(),
            identity.puzzle_hash.into(),
            PAYMENT_AMOUNT,
        );
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
            .with(AssertConcurrentSpend::new(payment.coin_id()));
        let (wallet_spend, requirement) = standard_spend(&identity, sponsor, delegated)?;
        let payment_spend = CoinSpend::new(
            payment,
            Program::from(payment_reveal),
            Program::from(vec![0x80]),
        );
        Ok(Self(TransactionPlan::new(
            identity,
            vec![payment_spend, wallet_spend],
            requirement,
            vec![vec![output], vec![change]],
            output,
            change,
            fee,
            height,
        )?))
    }
    pub fn input_coin(&self) -> Coin {
        self.0.spends[0].coin
    }
    pub fn sponsor_coin(&self) -> Coin {
        self.0.spends[1].coin
    }
    pub fn payment_coin(&self) -> Coin {
        self.0.payment
    }
    pub fn change_coin(&self) -> Coin {
        self.0.change
    }
    pub fn unsigned_bundle_bytes(&self) -> &[u8] {
        &self.0.unsigned
    }
    pub fn verify_signed_bundle(&self, bytes: &[u8]) -> Result<VerifiedTransaction> {
        self.0.verify(bytes)
    }
    /// Local fixture-only signer; absent from every production build.
    #[cfg(test)]
    pub(crate) fn sign_fixture(&self, fixture_seed: [u8; 32]) -> Result<VerifiedTransaction> {
        self.0
            .sign(&MasterSecret(SecretKey::from_seed(&fixture_seed)))
    }
    pub fn sign_hidden_tty(
        &self,
        requeried_payment: Coin,
        requeried_sponsor: Coin,
        network: &Testnet11Network,
    ) -> Result<VerifiedTransaction> {
        self.0
            .require_inputs(&[requeried_payment, requeried_sponsor], network)?;
        self.0.sign(&tty::read_master_secret()?)
    }
}

/// Public locally executed validation result, constructed only by exact plan
/// verification. It is deliberately not deserializable from an evidence report.
pub struct VerifiedTransaction {
    bundle: SpendBundle,
    bytes: Vec<u8>,
    cost: u64,
    execution_cost: u64,
    condition_cost: u64,
    charged_generator_bytes: usize,
    fee: u64,
    payment: Coin,
    change: Coin,
}
pub type VerifiedFunding = VerifiedTransaction;
impl VerifiedTransaction {
    pub fn bundle(&self) -> &SpendBundle {
        &self.bundle
    }
    pub fn bundle_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn transaction_id(&self) -> [u8; 32] {
        self.bundle.name().to_bytes()
    }
    pub fn cost(&self) -> u64 {
        self.cost
    }
    pub fn execution_cost(&self) -> u64 {
        self.execution_cost
    }
    pub fn condition_cost(&self) -> u64 {
        self.condition_cost
    }
    /// Plain generator bytes charged by the pinned spendbundle validator;
    /// excludes its two quote bytes. No backreference optimization is claimed.
    pub fn charged_generator_bytes(&self) -> usize {
        self.charged_generator_bytes
    }
    pub fn fee(&self) -> u64 {
        self.fee
    }
    pub fn payment_coin(&self) -> Coin {
        self.payment
    }
    pub fn change_coin(&self) -> Coin {
        self.change
    }
}

fn checked_change(identity: &WalletIdentity, input: Coin, principal: u64, fee: u64) -> Result<u64> {
    if input.puzzle_hash.to_bytes() != identity.puzzle_hash {
        return Err(WalletError::InputMismatch);
    }
    if fee > MAX_FEE {
        return Err(WalletError::FeeBound);
    }
    input
        .amount
        .checked_sub(principal.checked_add(fee).ok_or(WalletError::AmountBound)?)
        .ok_or(WalletError::AmountBound)
}

fn encode<T: ToClvm<Allocator>>(allocator: &mut Allocator, value: &T) -> Result<Vec<u8>> {
    let node = value
        .to_clvm(allocator)
        .map_err(|_| WalletError::ProgramBuild)?;
    node_to_bytes(allocator, node).map_err(|_| WalletError::ProgramBuild)
}
fn standard_spend(
    identity: &WalletIdentity,
    coin: Coin,
    delegated: Conditions,
) -> Result<(CoinSpend, RequiredBlsSignature)> {
    let mut allocator = Allocator::new_limited(VM_HEAP);
    let module = node_from_bytes(
        &mut allocator,
        &chia_puzzles::P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE,
    )
    .map_err(|_| WalletError::ProgramBuild)?;
    let reveal = encode(
        &mut allocator,
        &CurriedProgram {
            program: module,
            args: StandardArgs::new(identity.synthetic),
        },
    )?;
    let puzzle = node_from_bytes(&mut allocator, &reveal).map_err(|_| WalletError::ProgramBuild)?;
    if tree_hash(&allocator, puzzle).to_bytes() != identity.puzzle_hash {
        return Err(WalletError::WalletHashMismatch);
    }
    let solution = encode(
        &mut allocator,
        &StandardSolution::from_conditions(delegated.clone()),
    )?;
    let args = node_from_bytes(&mut allocator, &solution).map_err(|_| WalletError::ProgramBuild)?;
    let output = run_program(
        &mut allocator,
        &ChiaDialect::new(0),
        puzzle,
        args,
        MAX_POLICY_VM_COST,
    )
    .map_err(|_| WalletError::VmFailure)?
    .1;
    let actual = Conditions::from_clvm(&allocator, output)
        .map_err(|_| WalletError::ConditionMismatch)?
        .into_vec();
    if actual.len() != delegated.len() + 1 || actual[1..] != delegated.into_vec() {
        return Err(WalletError::ConditionMismatch);
    }
    let signature = actual[0]
        .clone()
        .into_agg_sig()
        .ok_or(WalletError::ConditionMismatch)?;
    if signature.kind != AggSigKind::Me
        || signature.public_key != identity.synthetic
        || signature.public_key.is_inf()
    {
        return Err(WalletError::ConditionMismatch);
    }
    let constants = AggSigConstants::new(TESTNET11_CONSTANTS.agg_sig_me_additional_data);
    let requirement = RequiredBlsSignature::from_condition(&coin, signature, &constants);
    Ok((
        CoinSpend::new(coin, Program::from(reveal), Program::from(solution)),
        requirement,
    ))
}
fn validate_payment(coin: Coin, reveal: &[u8], expected: &Conditions) -> Result<()> {
    if reveal.is_empty() || reveal.len() > MAX_PAYMENT_REVEAL {
        return Err(WalletError::ProgramBuild);
    }
    let mut allocator = Allocator::new_limited(VM_HEAP);
    let puzzle = node_from_bytes(&mut allocator, reveal).map_err(|_| WalletError::ProgramBuild)?;
    if node_to_bytes(&allocator, puzzle).map_err(|_| WalletError::ProgramBuild)? != reveal {
        return Err(WalletError::ProgramBuild);
    }
    if tree_hash(&allocator, puzzle).to_bytes() != coin.puzzle_hash.to_bytes() {
        return Err(WalletError::InputMismatch);
    }
    let output = run_program(
        &mut allocator,
        &ChiaDialect::new(0),
        puzzle,
        NodePtr::NIL,
        MAX_POLICY_VM_COST,
    )
    .map_err(|_| WalletError::VmFailure)?
    .1;
    let conditions =
        Conditions::from_clvm(&allocator, output).map_err(|_| WalletError::ConditionMismatch)?;
    if &conditions != expected {
        return Err(WalletError::ConditionMismatch);
    }
    Ok(())
}
impl TransactionPlan {
    #[allow(clippy::too_many_arguments)]
    fn new(
        identity: WalletIdentity,
        spends: Vec<CoinSpend>,
        requirement: RequiredBlsSignature,
        outputs: Vec<Vec<Coin>>,
        payment: Coin,
        change: Coin,
        fee: u64,
        height: u32,
    ) -> Result<Self> {
        let unsigned = SpendBundle::new(spends.clone(), Signature::default())
            .to_bytes()
            .map_err(|_| WalletError::ProgramBuild)?;
        if unsigned.len() > MAX_BUNDLE {
            return Err(WalletError::ProgramBuild);
        }
        Ok(Self {
            identity,
            spends,
            unsigned,
            requirement,
            outputs,
            payment,
            change,
            fee,
            height,
        })
    }
    fn require_inputs(&self, coins: &[Coin], network: &Testnet11Network) -> Result<()> {
        network.check()?;
        if coins.len() != self.spends.len()
            || coins.iter().zip(&self.spends).any(|(a, b)| a != &b.coin)
        {
            return Err(WalletError::InputMismatch);
        }
        Ok(())
    }
    fn sign(&self, secret: &MasterSecret) -> Result<VerifiedTransaction> {
        let synthetic = secret.synthetic_for(&self.identity)?;
        let signature = chia_bls::sign(&synthetic, self.requirement.message());
        let bundle = SpendBundle::new(self.spends.clone(), signature);
        self.verify(&bundle.to_bytes().map_err(|_| WalletError::ProgramBuild)?)
    }
    fn verify(&self, bytes: &[u8]) -> Result<VerifiedTransaction> {
        // The exact compiler-owned/public plan prefix closes all vector/program
        // lengths before inspecting untrusted signature bytes. Never decode a
        // caller-supplied SpendBundle with attacker-selected allocation lengths.
        let prefix = self
            .unsigned
            .len()
            .checked_sub(96)
            .ok_or(WalletError::ProgramBuild)?;
        if bytes.len() != self.unsigned.len()
            || bytes.len() > MAX_BUNDLE
            || bytes[..prefix] != self.unsigned[..prefix]
        {
            return Err(WalletError::ChangedBundle);
        }
        let signature = Signature::from_bytes(
            bytes[prefix..]
                .try_into()
                .map_err(|_| WalletError::SignatureEncoding)?,
        )
        .map_err(|_| WalletError::SignatureEncoding)?;
        if signature.to_bytes().as_slice() != &bytes[prefix..] {
            return Err(WalletError::SignatureEncoding);
        }
        let bundle = SpendBundle::new(self.spends.clone(), signature);
        let flags =
            get_flags_for_height_and_constants(self.height, &TESTNET11_CONSTANTS) | MEMPOOL_MODE;
        let (conditions, _) = validate_clvm_and_signature(
            &bundle,
            MAX_WALLET_BUNDLE_COST,
            &TESTNET11_CONSTANTS,
            flags,
        )
        .map_err(|failure| WalletError::Consensus(failure.1))?;
        let removed: u128 = self
            .spends
            .iter()
            .map(|spend| u128::from(spend.coin.amount))
            .sum();
        let added: u128 = self
            .outputs
            .iter()
            .flatten()
            .map(|coin| u128::from(coin.amount))
            .sum();
        if conditions.spends.len() != self.spends.len()
            || conditions.removal_amount != removed
            || conditions.addition_amount != added
            || removed.checked_sub(added) != Some(u128::from(self.fee))
            || conditions.reserve_fee != self.fee
            || !conditions.agg_sig_unsafe.is_empty()
        {
            return Err(WalletError::ConditionMismatch);
        }
        for (spend, expected) in self.spends.iter().zip(&self.outputs) {
            let actual = conditions
                .spends
                .iter()
                .find(|candidate| candidate.coin_id == spend.coin.coin_id())
                .ok_or(WalletError::ConditionMismatch)?;
            let mut outputs = actual
                .create_coin
                .iter()
                .map(|(hash, amount, hint)| ((*hash).to_bytes(), *amount, hint.is_none()))
                .collect::<Vec<_>>();
            let mut expected = expected
                .iter()
                .map(|coin| (coin.puzzle_hash.to_bytes(), coin.amount, true))
                .collect::<Vec<_>>();
            outputs.sort();
            expected.sort();
            if outputs != expected {
                return Err(WalletError::ConditionMismatch);
            }
        }
        let charged_generator_bytes = calculate_generator_length(&bundle.coin_spends)
            .checked_sub(2)
            .ok_or(WalletError::ConditionMismatch)?;
        let byte_cost = (charged_generator_bytes as u64)
            .checked_mul(TESTNET11_CONSTANTS.cost_per_byte)
            .ok_or(WalletError::ConditionMismatch)?;
        if conditions
            .execution_cost
            .checked_add(conditions.condition_cost)
            .and_then(|cost| cost.checked_add(byte_cost))
            != Some(conditions.cost)
        {
            return Err(WalletError::ConditionMismatch);
        }
        Ok(VerifiedTransaction {
            bundle,
            bytes: bytes.to_vec(),
            cost: conditions.cost,
            execution_cost: conditions.execution_cost,
            condition_cost: conditions.condition_cost,
            charged_generator_bytes,
            fee: self.fee,
            payment: self.payment,
            change: self.change,
        })
    }
}

#[cfg(test)]
#[path = "wallet/tests.rs"]
mod tests;
