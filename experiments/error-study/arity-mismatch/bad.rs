use chialisp_oxide::{Amount, CreateCoin, PuzzleHash, create_coin};

pub fn puzzle(recipient: PuzzleHash, amount: Amount) -> (CreateCoin,) {
    (create_coin(recipient, amount),)
}

// A host caller cannot pass an extra, silently ignored field.
pub fn spend_with_extra_field() -> (CreateCoin,) {
    puzzle(PuzzleHash([0x42; 32]), Amount::from_mojos(7), 99u64)
}
