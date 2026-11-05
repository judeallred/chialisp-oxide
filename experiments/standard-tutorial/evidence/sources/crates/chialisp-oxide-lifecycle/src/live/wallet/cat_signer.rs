//! Purpose-specific CAT signer. Only an opaque closed CAT plan can reach this
//! entry point; there is no message, raw-spend, secret-byte, argv/env or file API.
//! Live provider freshness, unspent/maturity checks and the sealed actual-source
//! capability belong to the caller. This module checks their exact Coin/height
//! bindings and re-executes the full bounded unsigned intent before opening TTY.
use super::{MasterSecret, Testnet11Network, WalletError, tty};
use crate::live::cat::{CatError, CatSpendPlan, VerifiedCatBundle};
use chia_bls::{PublicKey, Signature};
use chia_protocol::Coin;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::live) enum CatSigningError {
    Wallet(WalletError),
    Plan(CatError),
}
impl fmt::Display for CatSigningError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Both payload enums contain only closed nonsensitive codes. Never
        // format an upstream SecretKey: its Debug prints private bytes.
        write!(out, "closed Testnet11 CAT signing {self:?}")
    }
}
impl std::error::Error for CatSigningError {}
type Result<T> = std::result::Result<T, CatSigningError>;

/// The caller rebuilds an immutable plan at the freshly observed height and
/// supplies its complete freshly requeried unspent inputs. Equality here cannot
/// authenticate that observation. Production plans require the opaque actual-
/// process paired-Cargo/source and SDK capability. The current check-only CLI
/// drops that capability and exposes no live CAT outbox or signing command.
// The current production CLI exposes CAT admission checks only. Keep this
// purpose-specific entry point compiled and tested, but not wired to signing.
#[allow(dead_code)]
pub(in crate::live) fn sign_cat_plan_hidden_tty(
    plan: &CatSpendPlan,
    requeried_inputs: &[Coin],
    current_height: u32,
    network: &Testnet11Network,
) -> Result<VerifiedCatBundle> {
    sign_with_secret_source(
        plan,
        requeried_inputs,
        current_height,
        network,
        tty::read_master_secret,
    )
}

// Private dependency injection permits only local fixture tests to substitute
// a secret source. It is not exported or selectable from production input.
fn sign_with_secret_source(
    plan: &CatSpendPlan,
    requeried_inputs: &[Coin],
    current_height: u32,
    network: &Testnet11Network,
    read: impl FnOnce() -> super::Result<MasterSecret>,
) -> Result<VerifiedCatBundle> {
    network.check().map_err(CatSigningError::Wallet)?;
    plan.recheck_for_signing(requeried_inputs, current_height, network)
        .map_err(CatSigningError::Plan)?;
    let identity = plan.owner_identity();
    let expected = PublicKey::from_bytes(&identity.synthetic_public_key())
        .map_err(|_| CatSigningError::Wallet(WalletError::InvalidPublicKey))?;
    let required = plan.required_bls();
    if expected.is_inf()
        || required.len() != 2
        || required
            .iter()
            .any(|requirement| requirement.public_key != expected)
    {
        return Err(CatSigningError::Plan(CatError::Conditions));
    }
    // This is the first point where any credential can be read. All public
    // source/program/input/height/fee/output/message checks already returned.
    let master = read().map_err(CatSigningError::Wallet)?;
    let synthetic = master
        .synthetic_for(&identity)
        .map_err(CatSigningError::Wallet)?;
    let mut aggregate = Signature::default();
    for requirement in required {
        aggregate += &chia_bls::sign(&synthetic, requirement.message());
    }
    // Unsigned checks above deliberately skipped BLS. Only this successful
    // complete consensus/aggregate verification grants a VerifiedCatBundle.
    plan.verify_returned_signature(&aggregate)
        .map_err(CatSigningError::Plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::cat::{CatFees, CatMintPlan, CatTailAuthority, CatTransferPlan};
    use chia_bls::{SecretKey, master_to_wallet_unhardened};
    use chia_puzzle_types::DeriveSynthetic;
    use std::cell::Cell;

    const FIXTURE_SEED: [u8; 32] = [0x39; 32];
    const WRONG_FIXTURE_SEED: [u8; 32] = [0x67; 32];
    fn network() -> Testnet11Network {
        Testnet11Network::from_provider_labels("testnet11", "txch").unwrap()
    }
    fn fixture() -> (CatTailAuthority, CatMintPlan) {
        let master = SecretKey::from_seed(&FIXTURE_SEED);
        let synthetic = master_to_wallet_unhardened(&master, 0).derive_synthetic();
        let hash =
            chia_puzzle_types::standard::StandardArgs::curry_tree_hash(synthetic.public_key())
                .to_bytes();
        let identity = super::super::WalletIdentity::from_master_public_key(
            master.public_key().to_bytes(),
            hash,
        )
        .unwrap();
        let authority =
            CatTailAuthority::fixture_reference(&chia_puzzles::GENESIS_BY_COIN_ID).unwrap();
        let input = Coin::new([0x94; 32].into(), hash.into(), 1_000_000_000_000);
        let plan = CatMintPlan::new(
            &authority,
            identity,
            input,
            hash,
            CatFees::new(10_000, 10_000).unwrap(),
            0,
            &network(),
        )
        .unwrap();
        (authority, plan)
    }
    fn read_fixture() -> super::super::Result<MasterSecret> {
        Ok(MasterSecret(SecretKey::from_seed(&FIXTURE_SEED)))
    }
    #[test]
    fn closed_mint_and_transfer_signer_verify_actual_bls_without_exposing_secret_sources() {
        let (authority, mint) = fixture();
        let plan = mint.issuance_plan();
        assert!(!plan.costs().bls_verified);
        let signed =
            sign_with_secret_source(plan, &plan.input_coins(), 0, &network(), read_fixture)
                .unwrap();
        assert!(signed.costs().bls_verified);
        let owner = plan.owner_identity();
        let transfer = CatTransferPlan::new(
            &authority,
            signed.normal_cat(),
            owner,
            [0x81; 32],
            signed.fee_change(),
            10_000,
            0,
            &network(),
        )
        .unwrap();
        let plan = transfer.spend_plan();
        let signed =
            sign_with_secret_source(plan, &plan.input_coins(), 0, &network(), read_fixture)
                .unwrap();
        assert!(signed.costs().bls_verified);
        assert_eq!(signed.normal_cat().inner_puzzle_hash(), [0x81; 32]);
    }
    #[test]
    fn complete_coin_and_height_mismatches_never_read_a_secret() {
        let (_, mint) = fixture();
        let plan = mint.issuance_plan();
        let called = Cell::new(false);
        let denied = || {
            called.set(true);
            Err(WalletError::TtyUnavailable)
        };
        let mut wrong = plan.input_coins();
        wrong[0].amount += 1;
        assert!(matches!(
            sign_with_secret_source(plan, &wrong, 0, &network(), denied),
            Err(CatSigningError::Plan(CatError::Coin))
        ));
        assert!(!called.get());
        assert!(matches!(
            sign_with_secret_source(plan, &plan.input_coins(), 1, &network(), denied),
            Err(CatSigningError::Plan(CatError::Height))
        ));
        assert!(!called.get());
        let invalid = Testnet11Network { genesis: [0; 32] };
        assert!(matches!(
            sign_with_secret_source(plan, &plan.input_coins(), 0, &invalid, denied),
            Err(CatSigningError::Wallet(WalletError::NetworkMismatch))
        ));
        assert!(!called.get());
    }
    #[test]
    fn wrong_master_and_secret_source_errors_are_closed_and_redacted() {
        let (_, mint) = fixture();
        let plan = mint.issuance_plan();
        let wrong = || Ok(MasterSecret(SecretKey::from_seed(&WRONG_FIXTURE_SEED)));
        let error = sign_with_secret_source(plan, &plan.input_coins(), 0, &network(), wrong)
            .err()
            .unwrap();
        assert_eq!(error, CatSigningError::Wallet(WalletError::KeyMismatch));
        let secret = hex::encode(SecretKey::from_seed(&WRONG_FIXTURE_SEED).to_bytes());
        assert!(!format!("{error:?} {error}").contains(&secret));
        let error = sign_with_secret_source(plan, &plan.input_coins(), 0, &network(), || {
            Err(WalletError::TtyUnavailable)
        })
        .err()
        .unwrap();
        assert_eq!(error, CatSigningError::Wallet(WalletError::TtyUnavailable));
    }
}
