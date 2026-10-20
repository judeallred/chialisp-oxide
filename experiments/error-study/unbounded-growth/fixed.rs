pub fn puzzle(balance: u64, deposit: u64) -> u64 {
    balance.checked_add(deposit).expect("balance overflow")
}

#[test]
fn ordinary_deposits_pass() {
    assert_eq!(puzzle(7, 1), 8);
    assert_eq!(puzzle(u64::MAX - 1, 1), u64::MAX);
}

#[test]
#[should_panic]
fn overflowing_deposit_is_rejected_regardless_of_compiler_settings() {
    puzzle(u64::MAX, 1);
}
