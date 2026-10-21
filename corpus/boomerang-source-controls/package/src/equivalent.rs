//! Original teaching policy: a signed claim before a deadline, or an unsigned refund after it.
//! WIP prototype; not ready to secure real funds.

use chialisp_oxide::{
    AggSigMe, Amount, AssertMyAmount, AssertSecondsAbsolute, Cons, CreateCoin, Message32,
    PublicKey, PuzzleHash, Timestamp, agg_sig_me, assert_before_seconds_absolute, assert_my_amount,
    assert_seconds_absolute, create_coin,
};

/// Curry this complete tuple into a funded instance before accepting a bool-only solution.
/// The recipient key authorizes claiming; the two hashes fix the complete payout destinations.
pub type Terms = (PublicKey, PuzzleHash, PuzzleHash, Amount, Timestamp);

/// The named before/after helpers share a representation, but select different conditions.
/// None ends the refund list; Some appends exactly one claim signature condition.
pub type BoomerangConditions =
    Cons<AssertSecondsAbsolute, Cons<AssertMyAmount, Cons<CreateCoin, Option<AggSigMe>>>>;

/// Fixed claim intent; AGG_SIG_ME additionally binds the actual coin ID and network.
pub const CLAIM_INTENT: Message32 = Message32(*b"chialisp-oxide/boomerang/claim:1");

#[chialisp_oxide::puzzle]
pub fn puzzle(terms: Terms, claim: bool) -> BoomerangConditions {
    let (recipient_key, recipient, refund_to, amount, deadline) = terms;
    let time_condition = if claim {
        assert_before_seconds_absolute(deadline)
    } else {
        assert_seconds_absolute(deadline)
    };
    let destination = if !claim { refund_to } else { recipient };
    let authorization = if claim {
        Some(agg_sig_me(recipient_key, CLAIM_INTENT))
    } else {
        None
    };

    Cons {
        first: time_condition,
        rest: Cons {
            first: assert_my_amount(amount),
            rest: Cons {
                first: create_coin(destination, amount),
                rest: authorization,
            },
        },
    }
}
