//! Shared policy and commitment recipe for the precommit-claim and
//! precommit-registry teaching puzzles. WIP prototype; not ready for real funds.
//!
//! Every hash is SHA-256 over exact concatenated bytes with a named domain
//! prefix. All non-domain inputs have fixed lengths, so their boundaries are
//! unambiguous. Collision and preimage resistance are cryptographic
//! assumptions, not facts proved by these puzzles or their tests.

use chialisp_oxide::curry::{atom_hash, curried_puzzle_hash, pair_hash};
use chialisp_oxide::{Amount, BlockAge, CoinId, Message32, PublicKey, PuzzleHash, sha256_parts};

/// The single name this one-slot teaching registry can allocate.
pub const NAME: [u8; 5] = *b"alice";
/// Paid to the treasury by a successful claim, and the precommit coin's amount.
pub const REGISTRATION_PRICE: Amount = Amount::from_mojos(1_000);
/// The precommit amount as a canonical CLVM integer payload, as coin IDs hash it.
pub const REGISTRATION_PRICE_COIN_ID_BYTES: [u8; 2] = [0x03, 0xe8];
/// The registry slot coin's amount; it becomes the owner receipt.
pub const SLOT_AMOUNT: Amount = Amount::from_mojos(1);
/// A claim needs a precommit at least this many blocks old.
pub const MINIMUM_AGE: BlockAge = BlockAge::from_blocks(2);
/// After this age, anyone may return the precommit to its refund destination.
pub const REFUND_AGE: BlockAge = BlockAge::from_blocks(8);
/// Fixed teaching treasury destination.
pub const TREASURY: PuzzleHash = PuzzleHash([0x7e; 32]);

/// Curried into every precommit coin before it is funded:
/// (registry coin ID, commitment, refund destination).
pub type PrecommitTerms = (CoinId, Message32, PuzzleHash);
/// Revealed only when registering: (owner key, owner receipt destination, salt).
pub type Opening = (PublicKey, PuzzleHash, [u8; 32]);

const OWNER_DOMAIN: [u8; 33] = *b"chialisp-oxide/precommit/v1/owner";
const TARGET_DOMAIN: [u8; 34] = *b"chialisp-oxide/precommit/v1/target";
const OPENING_DOMAIN: [u8; 35] = *b"chialisp-oxide/precommit/v1/opening";
const COMMIT_DOMAIN: [u8; 34] = *b"chialisp-oxide/precommit/v1/commit";
const READY_DOMAIN: [u8; 33] = *b"chialisp-oxide/precommit/v1/ready";
const REFUNDED_DOMAIN: [u8; 36] = *b"chialisp-oxide/precommit/v1/refunded";
const ACCEPT_DOMAIN: [u8; 34] = *b"chialisp-oxide/precommit/v1/accept";
const AUTHORIZE_DOMAIN: [u8; 37] = *b"chialisp-oxide/precommit/v1/authorize";

/// The commitment C binds the registry slot, the name, the refund destination,
/// the owner's key and receipt destination, and a secret salt. The salt must be
/// unpredictable: a guessable name stays hidden only while the salt is secret.
pub fn commitment(registry: CoinId, refund_to: PuzzleHash, opening: Opening) -> Message32 {
    let (owner_key, owner_destination, salt) = opening;
    let owner = sha256_parts(&OWNER_DOMAIN, &owner_key.0, &owner_destination.0);
    let opened = sha256_parts(&OPENING_DOMAIN, &owner, &salt);
    let target = sha256_parts(&TARGET_DOMAIN, &registry.0, &NAME);
    let context = sha256_parts(&target, &refund_to.0, &opened);
    Message32(sha256_parts(&COMMIT_DOMAIN, &context, &[]))
}

/// Announced by a precommit coin that is ready to be claimed.
pub fn ready_message(commitment: Message32, precommit: CoinId) -> Message32 {
    Message32(sha256_parts(&READY_DOMAIN, &commitment.0, &precommit.0))
}

/// Announced by a precommit coin that was returned after the refund age.
pub fn refunded_message(commitment: Message32, precommit: CoinId) -> Message32 {
    Message32(sha256_parts(&REFUNDED_DOMAIN, &commitment.0, &precommit.0))
}

/// Announced by the registry when it accepts this commitment from this precommit.
pub fn accept_message(commitment: Message32, precommit: CoinId) -> Message32 {
    Message32(sha256_parts(&ACCEPT_DOMAIN, &commitment.0, &precommit.0))
}

/// Signed by the owner key; AGG_SIG_ME also binds the registry coin and network.
pub fn authorization_message(commitment: Message32) -> Message32 {
    Message32(sha256_parts(&AUTHORIZE_DOMAIN, &commitment.0, &[]))
}

/// Puzzle hash of the precommit module curried with its complete terms list.
pub fn precommit_puzzle_hash(module: PuzzleHash, terms: PrecommitTerms) -> PuzzleHash {
    let (registry, commitment, refund_to) = terms;
    let nil = atom_hash(&[]);
    let refund_list = pair_hash(&atom_hash(&refund_to.0), &nil);
    let commitment_list = pair_hash(&atom_hash(&commitment.0), &refund_list);
    let terms_hash = pair_hash(&atom_hash(&registry.0), &commitment_list);
    PuzzleHash(curried_puzzle_hash(&module.0, &terms_hash))
}

/// Coin ID of a precommit funded with exactly the registration price.
pub fn precommit_coin_id(parent: CoinId, puzzle_hash: PuzzleHash) -> CoinId {
    CoinId(sha256_parts(
        &parent.0,
        &puzzle_hash.0,
        &REGISTRATION_PRICE_COIN_ID_BYTES,
    ))
}
