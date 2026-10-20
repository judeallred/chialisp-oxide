use chialisp_oxide::{Amount, CreateCoin, PuzzleHash, Tree, create_coin};

pub fn puzzle(approvals: &Tree<'_>, recipient: PuzzleHash, amount: Amount) -> (CreateCoin,) {
    if approvals {
        (create_coin(recipient, amount),)
    } else {
        panic!("no approvals")
    }
}
