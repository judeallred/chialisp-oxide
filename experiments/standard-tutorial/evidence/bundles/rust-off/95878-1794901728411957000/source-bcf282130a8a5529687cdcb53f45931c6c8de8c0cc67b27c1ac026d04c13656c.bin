// Pure predicate gate, not authorization: callers control the boolean.
use chialisp_oxide::{Amount, CreateCoin, PuzzleHash, create_coin};

const PAYMENT_AMOUNT: Amount = Amount::from_mojos(1_000);
const RECIPIENT: PuzzleHash = PuzzleHash([0x42; 32]); // Fixed teaching destination.

#[chialisp_oxide::puzzle]
pub fn puzzle(allow: bool) -> (CreateCoin,) {
    assert!(allow);
    (create_coin(RECIPIENT, PAYMENT_AMOUNT),)
}
