// Permissionless fixture: any caller may select an amount within the cap.
pub fn puzzle(amount: u64) -> ((u64, u64), (u64, [u8; 32], u64)) {
    assert!(amount <= 1000);
    ((73, 1000), (51, [0x42; 32], amount))
}
