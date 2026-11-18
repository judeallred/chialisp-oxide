//! Original teaching policy (#11): a fixed four-entrant commit-reveal raffle.
//! WIP prototype; not ready to secure real funds.

use crate::ordered_payout;
use chialisp_oxide::continuation::{queue_end, queue_item};
use chialisp_oxide::{
    Amount, AssertHeightRelative, AssertMyAmount, BlockAge, CreateCoin, CreateCoinAnnouncement,
    Message32, PuzzleHash, assert_before_height_relative, assert_height_relative, assert_my_amount,
    create_coin, create_coin_announcement, sha256_parts,
};

/// Each of the four entrants contributes exactly one ticket.
pub const TICKET: Amount = Amount::from_mojos(1_000);
/// The whole pot, four tickets; no organizer fee.
pub const POT: Amount = Amount::from_mojos(4_000);
/// A draw must happen before this age; a refund only at or after it.
pub const REFUND_AGE: BlockAge = BlockAge::from_blocks(10);
const COMMIT_DOMAIN: [u8; 31] = *b"chialisp-oxide/raffle/v1/commit";
const DRAW_DOMAIN: [u8; 29] = *b"chialisp-oxide/raffle/v1/draw";
/// Announced by the refund path (the draw announces its seed instead).
pub const REFUND_INTENT: Message32 = Message32(*b"chialisp-oxide/raffle/v1/refund!");

/// One entrant: payout puzzle hash and `sha256(COMMIT_DOMAIN || salt)`.
pub type Entry = (PuzzleHash, Message32);
/// Curried as ONE proper list: (ordered-payout module hash, four entrants).
/// A host must check, before funding, that the module field names the selected
/// ordered-payout artifact, that the four payout hashes are distinct, and that
/// the coin holds exactly `POT`.
pub type Terms = (PuzzleHash, Entry, Entry, Entry, Entry);
/// The four salts, in roster order.
pub type Salts = (Message32, Message32, Message32, Message32);
/// The before/after height aliases share a layout; both paths emit four conditions.
pub type Conditions = (
    AssertMyAmount,
    AssertHeightRelative,
    CreateCoin,
    CreateCoinAnnouncement,
);

/// The commitment an entrant publishes before the raffle coin is funded.
pub fn commitment(salt: Message32) -> Message32 {
    Message32(sha256_parts(&COMMIT_DOMAIN, &salt.0, &[]))
}

/// The draw seed binds all four salts in roster order.
pub fn seed(salts: Salts) -> Message32 {
    let (first, second, third, fourth) = salts;
    let left = sha256_parts(&first.0, &second.0, &[]);
    let right = sha256_parts(&third.0, &fourth.0, &[]);
    Message32(sha256_parts(&DRAW_DOMAIN, &left, &right))
}

/// Draw: anyone may reveal all four salts before `REFUND_AGE`; the winner is
/// `seed[31] % 4` in roster order and receives the whole pot. Refund: at or after
/// `REFUND_AGE`, anyone may move the pot into an ordered-payout queue that repays
/// each entrant one ticket. The salts are still decoded on refund, then ignored.
#[chialisp_oxide::puzzle]
pub fn puzzle(terms: Terms, draw: bool, salts: Salts) -> Conditions {
    let (ordered_module, first, second, third, fourth) = terms;
    let (age, destination, message) = if draw {
        let (s1, s2, s3, s4) = salts;
        assert!(commitment(s1).0 == first.1.0);
        assert!(commitment(s2).0 == second.1.0);
        assert!(commitment(s3).0 == third.1.0);
        assert!(commitment(s4).0 == fourth.1.0);
        let drawn = seed(salts);
        let index = drawn.0[31] % 4;
        let winner = if index == 0 {
            first.0
        } else if index == 1 {
            second.0
        } else if index == 2 {
            third.0
        } else {
            fourth.0
        };
        (assert_before_height_relative(REFUND_AGE), winner, drawn)
    } else {
        let domain = &ordered_payout::QUEUE_DOMAIN;
        let head = queue_item(
            domain,
            first.0,
            queue_item(
                domain,
                second.0,
                queue_item(
                    domain,
                    third.0,
                    queue_item(domain, fourth.0, queue_end(domain)),
                ),
            ),
        );
        (
            assert_height_relative(REFUND_AGE),
            ordered_payout::instance_hash(ordered_module, head),
            REFUND_INTENT,
        )
    };
    (
        assert_my_amount(POT),
        age,
        create_coin(destination, POT),
        create_coin_announcement(message),
    )
}
