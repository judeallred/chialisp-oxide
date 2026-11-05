pub fn puzzle(enabled: bool) -> ([u8; 4], [u8; 4]) {
    assert!(enabled);
    ([1; 4], [0, 1, 128, 255])
}
