// Pure arithmetic example, not a coin puzzle: checked Rust u64 increment.
pub fn puzzle(counter: u64) -> u64 {
    assert!(counter < 1000);
    counter + 1
}
