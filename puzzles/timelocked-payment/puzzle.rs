// Permissionless after an absolute Unix timestamp; demonstrates a consensus check.
#[path = "../../sdk/oxide.rs"]
pub mod oxide;

use oxide::{
    Amount, AssertMyAmount, AssertSecondsAbsolute, CreateCoin, PuzzleHash, Timestamp,
    assert_my_amount, assert_seconds_absolute, create_coin,
};

const COIN_AMOUNT: Amount = Amount::from_mojos(1_000);
const RECIPIENT: PuzzleHash = PuzzleHash([0x42; 32]);
// Mock-chain Unix time, not a delay of 100 seconds after coin creation.
const UNLOCK_AT: Timestamp = Timestamp::from_unix_seconds(100);

pub type TimelockedConditions = (AssertSecondsAbsolute, AssertMyAmount, CreateCoin);

pub fn puzzle() -> TimelockedConditions {
    (
        assert_seconds_absolute(UNLOCK_AT),
        assert_my_amount(COIN_AMOUNT),
        create_coin(RECIPIENT, COIN_AMOUNT),
    )
}
