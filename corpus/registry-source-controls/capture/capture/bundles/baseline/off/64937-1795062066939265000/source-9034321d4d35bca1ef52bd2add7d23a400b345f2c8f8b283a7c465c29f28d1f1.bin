// Port of Chialisp Primer / Using Modules: a named constant and a helper that
// multiplies by it. The original multiplies unbounded signed CLVM integers;
// this port uses signed 64-bit values with checked multiplication.

/// Defines a constant value with a name.
const ORDER_OF_MAGNITUDE: i64 = 10;

/// Defines a function that can be called with a value.
fn raise_magnitude(value: i64) -> i64 {
    value * ORDER_OF_MAGNITUDE
}

/// Calls the previously defined function.
#[chialisp_oxide::puzzle]
pub fn puzzle(value: i64) -> i64 {
    raise_magnitude(value)
}
