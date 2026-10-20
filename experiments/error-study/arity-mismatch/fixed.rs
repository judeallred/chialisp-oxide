use chialisp_oxide::{Amount, CreateCoin, PuzzleHash, create_coin};

pub fn puzzle(recipient: PuzzleHash, amount: Amount) -> (CreateCoin,) {
    (create_coin(recipient, amount),)
}

#[test]
fn exactly_two_arguments_pay() {
    let (condition,) = puzzle(PuzzleHash([0x42; 32]), Amount::from_mojos(7));
    assert_eq!((condition.0, condition.1.0, condition.2.mojos()), (51, [0x42; 32], 7));
}
