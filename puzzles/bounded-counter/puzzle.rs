// Pure arithmetic example, not a coin puzzle: checked Rust u64 increment.
const COUNTER_LIMIT: u64 = 1_000;

pub fn puzzle(counter: u64) -> u64 {
    assert!(counter < COUNTER_LIMIT);
    counter + 1
}
