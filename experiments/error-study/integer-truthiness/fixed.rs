use chialisp_oxide::{Amount, CreateCoin, PuzzleHash, create_coin};

pub fn puzzle(approved: u64, recipient: PuzzleHash, amount: Amount) -> (CreateCoin,) {
    assert!(approved == 1);
    (create_coin(recipient, amount),)
}

#[test]
fn approval_one_produces_the_intended_payment() {
    let (condition,) = puzzle(1, PuzzleHash([0x42; 32]), Amount::from_mojos(7));
    assert_eq!(condition.1.0, [0x42; 32]);
    assert_eq!(condition.2.mojos(), 7);
}

#[test]
#[should_panic]
fn approval_zero_is_rejected() {
    puzzle(0, PuzzleHash([0x42; 32]), Amount::from_mojos(7));
}

#[test]
#[should_panic]
fn nonzero_approval_two_is_also_rejected() {
    puzzle(2, PuzzleHash([0x42; 32]), Amount::from_mojos(7));
}
