use chialisp_oxide::{Amount, CreateCoin, PuzzleHash, create_coin};

pub fn puzzle(pay: bool, recipient: PuzzleHash, amount: Amount) -> (CreateCoin,) {
    if pay {
        (create_coin(recipient, amount),)
    } else {
        create_coin(recipient, amount)
    }
}
