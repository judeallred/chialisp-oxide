pub fn puzzle(primary: bool, amount: u64) -> u64 {
    if primary { amount } else { amount }
}

#[test]
fn both_branches_return_the_supplied_amount() {
    assert_eq!(puzzle(true, 7), 7);
    assert_eq!(puzzle(false, 7), 7);
}
