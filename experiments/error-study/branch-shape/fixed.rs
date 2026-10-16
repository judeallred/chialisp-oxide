use chialisp_oxide::{Amount, CreateCoin, PuzzleHash, create_coin};

pub fn puzzle(pay: bool, recipient: PuzzleHash, amount: Amount) -> (CreateCoin,) {
    if pay {
        (create_coin(recipient, amount),)
    } else {
        (create_coin(recipient, amount),)
    }
}

#[test]
fn both_branches_have_the_declared_condition_list_shape() {
    for pay in [false, true] {
        let (condition,) = puzzle(pay, PuzzleHash([0x42; 32]), Amount::from_mojos(7));
        assert_eq!(condition.0, 51);
        assert_eq!(condition.1.0, [0x42; 32]);
        assert_eq!(condition.2.mojos(), 7);
    }
}
