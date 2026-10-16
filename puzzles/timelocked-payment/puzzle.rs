// Permissionless after the absolute timestamp; intended to demonstrate consensus checks.
pub fn puzzle() -> ((u64, u64), (u64, u64), (u64, [u8; 32], u64)) {
    ((81, 100), (73, 1000), (51, [0x42; 32], 1000))
}
