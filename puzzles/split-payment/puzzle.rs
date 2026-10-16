// Permissionless split: both fixed recipients together receive the full coin amount.
use oxide_sdk::{Amount, AssertMyAmount, CreateCoin, PuzzleHash, assert_my_amount, create_coin};

const COIN_AMOUNT: Amount = Amount::from_mojos(1_000);
const FIRST_RECIPIENT: PuzzleHash = PuzzleHash([0x42; 32]);
const SECOND_RECIPIENT: PuzzleHash = PuzzleHash([0x43; 32]);

pub type SplitConditions = (AssertMyAmount, CreateCoin, CreateCoin);

#[oxide_sdk::puzzle]
pub fn puzzle(first_amount: Amount) -> SplitConditions {
    assert!(first_amount.is_at_most(COIN_AMOUNT));
    let second_amount = Amount::from_mojos(COIN_AMOUNT.mojos() - first_amount.mojos());
    (
        assert_my_amount(COIN_AMOUNT),
        create_coin(FIRST_RECIPIENT, first_amount),
        create_coin(SECOND_RECIPIENT, second_amount),
    )
}
