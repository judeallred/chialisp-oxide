// Two fixed recipients share exactly 1000; first_amount controls the split.
pub fn puzzle(first_amount: u64) -> ((u64, u64), (u64, [u8; 32], u64), (u64, [u8; 32], u64)) {
    assert!(first_amount <= 1000);
    let second_amount = 1000 - first_amount;
    (
        (73, 1000),
        (51, [0x42; 32], first_amount),
        (51, [0x43; 32], second_amount),
    )
}
