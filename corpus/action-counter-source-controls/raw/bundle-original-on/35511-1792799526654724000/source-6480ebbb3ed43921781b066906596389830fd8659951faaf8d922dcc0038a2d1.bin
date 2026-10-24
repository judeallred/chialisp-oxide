// AGG_SIG_ME binds the owner's signature over the message to this coin.
use crate::fixture_keys;

use chialisp_oxide::{
    AggSigMe, Amount, AssertMyAmount, CreateCoin, Message32, PublicKey, PuzzleHash, agg_sig_me,
    assert_my_amount, create_coin,
};

const OWNER: PublicKey = PublicKey(fixture_keys::OWNER_PUBLIC_KEY);
const COIN_AMOUNT: Amount = Amount::from_mojos(1_000);
const RECIPIENT: PuzzleHash = PuzzleHash([0x42; 32]);

pub type SignedConditions = (AggSigMe, AssertMyAmount, CreateCoin);

#[chialisp_oxide::puzzle]
pub fn puzzle(message: Message32) -> SignedConditions {
    (
        agg_sig_me(OWNER, message),
        assert_my_amount(COIN_AMOUNT),
        create_coin(RECIPIENT, COIN_AMOUNT),
    )
}
