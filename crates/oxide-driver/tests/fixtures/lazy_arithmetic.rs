pub fn puzzle(amount: u64, enabled: bool) -> u64 {
    if enabled { amount + 1 } else { 0 }
}
