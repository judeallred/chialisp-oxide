use chialisp_oxide::{Amount, CreateCoin, PuzzleHash, create_coin};

pub struct Recipient(pub PuzzleHash);
pub struct Change(pub PuzzleHash);

fn pay_recipient(to: Recipient, amount: Amount) -> CreateCoin {
    create_coin(to.0, amount)
}

fn pay_change(to: Change, amount: Amount) -> CreateCoin {
    create_coin(to.0, amount)
}

pub fn puzzle(
    recipient: Recipient,
    change: Change,
    amount: Amount,
    change_amount: Amount,
) -> (CreateCoin, CreateCoin) {
    (pay_recipient(change, amount), pay_change(recipient, change_amount))
}
