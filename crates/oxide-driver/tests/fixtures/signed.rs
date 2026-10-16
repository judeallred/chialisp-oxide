pub fn puzzle(amount: i64, minimum: i64) -> i64 {
    assert!(amount > minimum);
    if amount < -1 {
        amount + 1
    } else {
        amount - 1
    }
}
