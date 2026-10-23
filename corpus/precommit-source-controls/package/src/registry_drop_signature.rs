//! Original teaching policy: a one-name registry slot that accepts exactly one
//! owner-signed opening of a mature, genuine precommit coin.
//! WIP prototype; not ready to secure real funds.

use crate::precommit_recipe::{
    Opening, SLOT_AMOUNT, accept_message, authorization_message, commitment, precommit_coin_id,
    precommit_puzzle_hash, ready_message,
};
use chialisp_oxide::{
    AssertCoinAnnouncement, AssertMyAmount, AssertMyCoinId, CoinId, CreateCoin,
    CreateCoinAnnouncement, PuzzleHash, agg_sig_me, assert_coin_announcement, assert_my_amount,
    assert_my_coin_id, coin_announcement_id, create_coin, create_coin_announcement,
};

pub type RegistryConditions = (
    AssertMyCoinId,
    AssertMyAmount,
    AssertCoinAnnouncement,
    CreateCoinAnnouncement,
    CreateCoin,
);

/// Curry the exact precommit module hash before funding the slot. The solution
/// names this slot's coin ID, the precommit's parent and refund destination,
/// and the complete opening.
///
/// The registry recomputes the commitment and the genuine precommit coin ID
/// rather than trusting either from the solution. That coin's claim path is
/// what enforces the minimum age and the treasury payment.
#[chialisp_oxide::puzzle]
pub fn puzzle(
    precommit_module: PuzzleHash,
    me: CoinId,
    precommit_parent: CoinId,
    refund_to: PuzzleHash,
    opening: Opening,
) -> RegistryConditions {
    let (owner_key, owner_destination, _) = opening;
    let committed = commitment(me, refund_to, opening);
    let precommit = precommit_coin_id(
        precommit_parent,
        precommit_puzzle_hash(precommit_module, (me, committed, refund_to)),
    );

    let _unused_authorization = agg_sig_me(owner_key, authorization_message(committed));

    (
        assert_my_coin_id(me),
        assert_my_amount(SLOT_AMOUNT),
        assert_coin_announcement(coin_announcement_id(
            precommit,
            ready_message(committed, precommit),
        )),
        create_coin_announcement(accept_message(committed, precommit)),
        create_coin(owner_destination, SLOT_AMOUNT),
    )
}
