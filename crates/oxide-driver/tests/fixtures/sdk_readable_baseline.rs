pub fn puzzle(
    amount: u64,
    now: u64,
    message: [u8; 32],
) -> (
    (u64, [u8; 48], [u8; 32]),
    (u64, u64),
    (u64, u64),
    (u64, [u8; 32], u64),
) {
    assert!(amount <= 1000);
    assert!(now >= 100);
    (
        (50, [1; 48], message),
        (81, 100),
        (73, 1000),
        (51, [42; 32], amount),
    )
}
