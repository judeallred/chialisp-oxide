pub fn puzzle(counter: u64) -> u64 {
    counter + 1
}

#[test]
fn ordinary_small_values_pass() {
    assert_eq!(puzzle(0), 1);
    assert_eq!(puzzle(7), 8);
}

#[test]
fn independent_output_cap_catches_the_missing_guard() {
    assert!(puzzle(1000) <= 1000);
}
