fn validate(amount: u64) -> bool {
    assert!(amount >= 10);
    true
}
pub fn puzzle(amount: u64) -> u64 {
    validate(amount);
    amount
}
