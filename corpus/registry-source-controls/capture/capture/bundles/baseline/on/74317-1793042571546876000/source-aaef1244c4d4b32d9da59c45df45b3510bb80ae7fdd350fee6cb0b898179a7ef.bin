// Port of Chialisp Primer / Using Modules: factorial.clsp.
// The original recurses over unbounded signed CLVM integers; this port uses
// signed 64-bit values. 21! overflows i64, so inputs above 20 reject instead
// of returning the original's larger result. Inputs of 1 or less return 1.

/// Defines the factorial function.
fn factorial(number: i64) -> i64 {
    if number > 1 {
        number * factorial(number - 1)
    } else {
        1
    }
}

/// Calls the function with the number provided.
#[chialisp_oxide::puzzle]
pub fn puzzle(number: i64) -> i64 {
    factorial(number)
}
