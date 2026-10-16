#[path = "../../../../sdk/chialisp_oxide.rs"]
pub mod chialisp_oxide;

use chialisp_oxide::{
    AggSigMe, Amount, AssertMyAmount, AssertSecondsAbsolute, CreateCoin, Message32, PublicKey,
    PuzzleHash, Timestamp, agg_sig_me, assert_my_amount, assert_seconds_absolute, create_coin,
};

const PAYMENT_CAP: Amount = Amount::from_mojos(1000);
const UNLOCK_TIME: Timestamp = Timestamp::from_unix_seconds(100);
const OWNER: PublicKey = PublicKey([1; 48]);
const RECIPIENT: PuzzleHash = PuzzleHash([42; 32]);

pub fn puzzle(
    amount: Amount,
    now: Timestamp,
    message: Message32,
) -> (AggSigMe, AssertSecondsAbsolute, AssertMyAmount, CreateCoin) {
    assert!(amount.is_at_most(PAYMENT_CAP));
    assert!(now.is_at_or_after(UNLOCK_TIME));
    let payment = Amount::from_mojos(amount.mojos());
    (
        agg_sig_me(OWNER, message),
        assert_seconds_absolute(UNLOCK_TIME),
        assert_my_amount(PAYMENT_CAP),
        create_coin(RECIPIENT, payment),
    )
}
