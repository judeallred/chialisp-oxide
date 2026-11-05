// Port of Chialisp Concepts / Currying: (mod (first second) (* first second)).
// The original multiplies unbounded signed CLVM integers; this port multiplies
// two signed 64-bit values. Overflow and noncanonical atoms reject instead of
// wrapping. The page curries `first` to make a doubler; the corpus builds that
// standard curried instance of every compiled artifact separately.
#[chialisp_oxide::puzzle]
pub fn puzzle(first: i64, second: i64) -> i64 {
    first * second
}
