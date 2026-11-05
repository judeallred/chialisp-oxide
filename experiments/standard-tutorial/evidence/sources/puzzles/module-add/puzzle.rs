// Port of Chialisp Primer / Using Modules: (mod (first second) (+ first second)).
// The original adds unbounded signed CLVM integers; this port adds two signed
// 64-bit values. Overflow and noncanonical atoms reject instead of wrapping.
#[chialisp_oxide::puzzle]
pub fn puzzle(first: i64, second: i64) -> i64 {
    first + second
}
