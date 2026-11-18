//! Original finite hot-potato policy. WIP; not ready to secure real funds.
use chialisp_oxide::curry::{atom_hash, curried_puzzle_hash, pair_hash};
use chialisp_oxide::{
    AggSigMe, Amount, AssertHeightRelative, AssertMyAmount, BlockAge, CreateCoin,
    CreateCoinAnnouncement, Message32, PublicKey, PuzzleHash, agg_sig_me,
    assert_before_height_relative, assert_height_relative, assert_my_amount, create_coin,
    create_coin_announcement, sha256_parts,
};

pub const MINIMUM_RAISE: Amount = Amount::from_mojos(1_000);
pub const MAXIMUM_POT: Amount = Amount::from_mojos(8_000);
pub const MAXIMUM_ROUND: u8 = 8;
pub const MINIMUM_COIN_AGE: BlockAge = BlockAge::from_blocks(1);
pub const TAKE_WINDOW: BlockAge = BlockAge::from_blocks(5);
const TAKE_DOMAIN: [u8; 28] = *b"chialisp-oxide/spuds/take:v1";
const CLAIM_DOMAIN: [u8; 29] = *b"chialisp-oxide/spuds/claim:v1";

/// ONE curried proper list: actual module, holder key, payout hash, finite round.
/// The creator must independently bind the module to the selected artifact.
pub type Terms = (PuzzleHash, PublicKey, PuzzleHash, u8);
/// The before/after height aliases have the same wire layout. Every path emits
/// a signature, exact amount, minimum age, boundary, one output and announcement.
pub type Conditions = (
    AggSigMe,
    AssertMyAmount,
    AssertHeightRelative,
    AssertHeightRelative,
    CreateCoin,
    CreateCoinAnnouncement,
);

pub fn pot(round: u8) -> Amount {
    match round {
        1 => Amount::from_mojos(1_000),
        2 => Amount::from_mojos(2_000),
        3 => Amount::from_mojos(3_000),
        4 => Amount::from_mojos(4_000),
        5 => Amount::from_mojos(5_000),
        6 => Amount::from_mojos(6_000),
        7 => Amount::from_mojos(7_000),
        8 => MAXIMUM_POT,
        _ => panic!(),
    }
}

pub fn round_atom_hash(round: u8) -> [u8; 32] {
    match round {
        1 => atom_hash(&[1]),
        2 => atom_hash(&[2]),
        3 => atom_hash(&[3]),
        4 => atom_hash(&[4]),
        5 => atom_hash(&[5]),
        6 => atom_hash(&[6]),
        7 => atom_hash(&[7]),
        8 => atom_hash(&[8]),
        _ => panic!(),
    }
}

pub fn instance_hash(terms: Terms) -> PuzzleHash {
    let (module, holder_key, holder_payout, round) = terms;
    assert!(round >= 1);
    assert!(round <= MAXIMUM_ROUND);
    let tail = pair_hash(&round_atom_hash(round), &atom_hash(&[]));
    let payout = pair_hash(&atom_hash(&holder_payout.0), &tail);
    let key = pair_hash(&atom_hash(&holder_key.0), &payout);
    let terms_hash = pair_hash(&atom_hash(&module.0), &key);
    PuzzleHash(curried_puzzle_hash(&module.0, &terms_hash))
}

/// Bind the take signature and funding announcement to the complete successor.
pub fn take_message(successor: PuzzleHash) -> Message32 {
    Message32(sha256_parts(&TAKE_DOMAIN, &successor.0, &[]))
}

pub fn claim_message() -> Message32 {
    Message32(sha256_parts(&CLAIM_DOMAIN, &[], &[]))
}

/// Anyone may take using their own key and a real external 1,000-mojo top-up.
/// Claim ignores the two new-holder fields after strict ABI decoding; it always
/// uses the curried current holder and payout. Neither branch checks a signature
/// or coin age natively: these returned conditions require actual consensus.
#[chialisp_oxide::puzzle]
pub fn step(terms: Terms, take: bool, next_key: PublicKey, next_payout: PuzzleHash) -> Conditions {
    let (module, holder_key, holder_payout, round) = terms;
    let amount = pot(round);
    let (key, message, boundary, output) = if take {
        assert!(round < MAXIMUM_ROUND);
        let next_round = round + 1;
        let next_amount = pot(next_round);
        assert!(next_amount.mojos() == amount.mojos() + MINIMUM_RAISE.mojos());
        let successor = instance_hash((module, next_key, next_payout, next_round));
        (
            next_key,
            take_message(successor),
            assert_before_height_relative(TAKE_WINDOW),
            create_coin(successor, next_amount),
        )
    } else {
        (
            holder_key,
            claim_message(),
            assert_height_relative(TAKE_WINDOW),
            create_coin(holder_payout, amount),
        )
    };
    (
        agg_sig_me(key, message),
        assert_my_amount(Amount::from_mojos(1_001)),
        assert_height_relative(MINIMUM_COIN_AGE),
        boundary,
        output,
        create_coin_announcement(message),
    )
}

/// Equality check only, not full artifact/source admission or funding authority.
pub fn funding_hash(selected_module: PuzzleHash, terms: Terms) -> PuzzleHash {
    assert!(selected_module.0 == terms.0.0);
    instance_hash(terms)
}

#[cfg(test)]
mod fixture_keys;
#[cfg(test)]
mod tests;
