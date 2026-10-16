use chialisp_oxide::{Amount, CreateCoin, PuzzleHash, create_coin};

pub fn puzzle(approved: u64, recipient: PuzzleHash, amount: Amount) -> (CreateCoin,) {
    if approved {
        (create_coin(recipient, amount),)
    } else {
        panic!("not approved")
    }
}
