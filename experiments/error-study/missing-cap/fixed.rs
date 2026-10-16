pub fn puzzle(counter: u64) -> u64 {
    assert!(counter < 1000);
    counter + 1
}

#[test]
fn accepted_boundary_is_within_the_cap() {
    assert_eq!(puzzle(0), 1);
    assert_eq!(puzzle(999), 1000);
}

#[test]
#[should_panic]
fn input_at_the_cap_is_rejected() {
    puzzle(1000);
}
