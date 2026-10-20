use chialisp_oxide::{Amount, CreateCoin, PuzzleHash, create_coin};

pub fn puzzle(recipient: PuzzleHash, amount: Amount) -> (CreateCoin,) {
    (create_coin(recipient, amount),)
}

#[test]
fn native_amounts_have_one_representation() {
    let (condition,) = puzzle(PuzzleHash([0x42; 32]), Amount::from_mojos(7));
    assert_eq!(condition.2.mojos(), 7);
    assert_eq!(Amount::from_mojos(7).mojos(), Amount(7).mojos());
}
