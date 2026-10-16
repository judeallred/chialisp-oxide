// Permissionless fixture: fixes the recipient and amount, without owner authorization.
#[path = "../../sdk/oxide.rs"]
pub mod oxide;

use oxide::{Amount, AssertMyAmount, CreateCoin, PuzzleHash, assert_my_amount, create_coin};

const COIN_AMOUNT: Amount = Amount::from_mojos(1_000);
const RECIPIENT: PuzzleHash = PuzzleHash([0x42; 32]); // Fixed teaching destination.

pub type PaymentConditions = (AssertMyAmount, CreateCoin);

pub fn puzzle() -> PaymentConditions {
    (
        assert_my_amount(COIN_AMOUNT),
        create_coin(RECIPIENT, COIN_AMOUNT),
    )
}
