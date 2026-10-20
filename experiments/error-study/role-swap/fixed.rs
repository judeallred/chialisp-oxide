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
    (pay_recipient(recipient, amount), pay_change(change, change_amount))
}

#[test]
fn each_role_receives_its_own_amount() {
    let (payment, change) = puzzle(
        Recipient(PuzzleHash([0x42; 32])),
        Change(PuzzleHash([0x99; 32])),
        Amount::from_mojos(7),
        Amount::from_mojos(3),
    );
    assert_eq!((payment.0, payment.1.0, payment.2.mojos()), (51, [0x42; 32], 7));
    assert_eq!((change.0, change.1.0, change.2.mojos()), (51, [0x99; 32], 3));
}
