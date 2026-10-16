// Both independent signers must authorize the same coin-bound message.
use crate::fixture_keys;

use oxide_sdk::{
    AggSigMe, Amount, AssertMyAmount, CreateCoin, Message32, PublicKey, PuzzleHash, agg_sig_me,
    assert_my_amount, create_coin,
};

const FIRST_SIGNER: PublicKey = PublicKey(fixture_keys::OWNER_PUBLIC_KEY);
const SECOND_SIGNER: PublicKey = PublicKey(fixture_keys::RECOVERY_PUBLIC_KEY);
const COIN_AMOUNT: Amount = Amount::from_mojos(1_000);
const RECIPIENT: PuzzleHash = PuzzleHash([0x42; 32]);

pub type TwoSignatureConditions = (AggSigMe, AggSigMe, AssertMyAmount, CreateCoin);

#[oxide_sdk::puzzle]
pub fn puzzle(message: Message32) -> TwoSignatureConditions {
    (
        agg_sig_me(FIRST_SIGNER, message),
        agg_sig_me(SECOND_SIGNER, message),
        assert_my_amount(COIN_AMOUNT),
        create_coin(RECIPIENT, COIN_AMOUNT),
    )
}
