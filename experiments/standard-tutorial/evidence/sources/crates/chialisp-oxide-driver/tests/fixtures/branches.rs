pub fn puzzle(amount: u64, enabled: bool) -> u64 {
    if enabled {
        match amount {
            0 => 10,
            1 => 20,
            _ => amount,
        }
    } else {
        assert!(amount == 0);
        0
    }
}
