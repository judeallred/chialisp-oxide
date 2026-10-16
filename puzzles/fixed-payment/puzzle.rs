// Permissionless fixture: fixes the recipient and amount, but has no owner key.
pub fn puzzle() -> ((u64, u64), (u64, [u8; 32], u64)) {
    ((73, 1000), (51, [0x42; 32], 1000))
}
