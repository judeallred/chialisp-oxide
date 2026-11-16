// The owner can spend immediately; the recovery signer waits for absolute Unix time.
use crate::fixture_keys;

use chialisp_oxide::{
    AggSigMe, Amount, AssertMyAmount, AssertSecondsAbsolute, CreateCoin, Message32, PublicKey,
    PuzzleHash, Timestamp, agg_sig_me, assert_my_amount, assert_seconds_absolute, create_coin,
};

const OWNER: PublicKey = PublicKey(fixture_keys::OWNER_PUBLIC_KEY);
const RECOVERY_SIGNER: PublicKey = PublicKey(fixture_keys::RECOVERY_PUBLIC_KEY);
const COIN_AMOUNT: Amount = Amount::from_mojos(1_000);
const RECIPIENT: PuzzleHash = PuzzleHash([0x42; 32]);
const OWNER_UNLOCK_AT: Timestamp = Timestamp::from_unix_seconds(0); // Unix epoch: no wait.
const RECOVERY_UNLOCK_AT: Timestamp = Timestamp::from_unix_seconds(100); // Mock-chain time.

pub type RecoveryConditions = (AggSigMe, AssertSecondsAbsolute, AssertMyAmount, CreateCoin);

#[chialisp_oxide::puzzle]
pub fn puzzle(recovery: bool, message: Message32) -> RecoveryConditions {
    let signer = if recovery { RECOVERY_SIGNER } else { OWNER };
    let unlock_at = if recovery {
        RECOVERY_UNLOCK_AT
    } else {
        OWNER_UNLOCK_AT
    };
    (
        agg_sig_me(signer, message),
        assert_seconds_absolute(unlock_at),
        assert_my_amount(COIN_AMOUNT),
        create_coin(RECIPIENT, COIN_AMOUNT),
    )
}
