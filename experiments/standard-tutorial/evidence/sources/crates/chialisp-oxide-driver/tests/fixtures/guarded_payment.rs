pub fn puzzle(amount: u64, minimum: u64) -> (u8, u64) {
    assert!(amount >= minimum);
    (51, amount)
}
