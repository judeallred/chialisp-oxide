// Pure predicate-gated fixture, not authorization: callers control the boolean.
pub fn puzzle(allow: bool) -> ((u64, [u8; 32], u64),) {
    assert!(allow);
    ((51, [0x42; 32], 1000),)
}
