//! Original teaching policy: a salted precommitment that can be claimed by its
//! registry after a minimum age, or returned to a committed destination later.
//! WIP prototype; not ready to secure real funds.

use crate::precommit_recipe::{
    MINIMUM_AGE, PrecommitTerms, REFUND_AGE, REGISTRATION_PRICE, TREASURY, accept_message,
    ready_message, refunded_message,
};
use chialisp_oxide::{
    AssertCoinAnnouncement, AssertHeightRelative, AssertMyAmount, AssertMyCoinId, CoinId, Cons,
    CreateCoin, CreateCoinAnnouncement, assert_coin_announcement, assert_height_relative,
    assert_my_amount, assert_my_coin_id, coin_announcement_id, create_coin,
    create_coin_announcement,
};

/// Both paths fix this coin's ID and amount, wait, pay out and announce the
/// outcome. Only a claim also requires the registry's acceptance.
pub type PrecommitConditions = Cons<
    AssertMyCoinId,
    Cons<
        AssertMyAmount,
        Cons<
            AssertHeightRelative,
            Cons<CreateCoin, Cons<CreateCoinAnnouncement, Option<AssertCoinAnnouncement>>>,
        >,
    >,
>;

/// Curry the complete terms before funding. The solution chooses the path and
/// names this coin's ID, which ASSERT_MY_COIN_ID then binds.
#[chialisp_oxide::puzzle]
pub fn puzzle(terms: PrecommitTerms, claim: bool, me: CoinId) -> PrecommitConditions {
    let (registry, commitment, refund_to) = terms;
    let age = if claim { MINIMUM_AGE } else { REFUND_AGE };
    let destination = if claim { TREASURY } else { refund_to };
    let outcome = if claim {
        ready_message(commitment, me)
    } else {
        refunded_message(commitment, me)
    };
    let acceptance = if claim {
        Some(assert_coin_announcement(coin_announcement_id(
            registry,
            accept_message(commitment, me),
        )))
    } else {
        None
    };

    let _unused_age = assert_height_relative(age);

    Cons {
        first: assert_my_coin_id(me),
        rest: Cons {
            first: assert_my_amount(REGISTRATION_PRICE),
            rest: Cons {
                first: assert_height_relative(chialisp_oxide::BlockAge::from_blocks(0)),
                rest: Cons {
                    first: create_coin(destination, REGISTRATION_PRICE),
                    rest: Cons {
                        first: create_coin_announcement(outcome),
                        rest: acceptance,
                    },
                },
            },
        },
    }
}
