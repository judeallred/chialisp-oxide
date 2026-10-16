// Permissionless fixture: callers select a payment; the remainder becomes a fee.
#[path = "../../sdk/oxide.rs"]
pub mod oxide;

use oxide::{Amount, AssertMyAmount, CreateCoin, PuzzleHash, assert_my_amount, create_coin};

const COIN_AMOUNT: Amount = Amount::from_mojos(1_000);
const PAYMENT_CAP: Amount = COIN_AMOUNT;
const RECIPIENT: PuzzleHash = PuzzleHash([0x42; 32]); // Fixed teaching destination.

pub type PaymentConditions = (AssertMyAmount, CreateCoin);

pub fn puzzle(amount: Amount) -> PaymentConditions {
    assert!(amount.is_at_most(PAYMENT_CAP));
    (
        assert_my_amount(COIN_AMOUNT),
        create_coin(RECIPIENT, amount),
    )
}
