use chialisp_oxide::{Amount, CreateCoin, PuzzleHash, create_coin};

// Native Rust cannot even express a padded amount: u64 has one value seven.
// The protection is the compiled strict ABI decoder, not this source.
pub fn puzzle(recipient: PuzzleHash, amount: Amount) -> (CreateCoin,) {
    (create_coin(recipient, amount),)
}
