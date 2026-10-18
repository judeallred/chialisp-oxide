//! Actual ordinary Rust Cargo exports for the opt-in bounded scalar model.
#[inline(never)]
fn leaf(value: u8) -> u8 {
    assert!(value < 100);
    value + 2
}
#[inline(never)]
fn dag1(value: u8) -> u8 {
    let answer = leaf(value);
    let _checked = leaf(value);
    answer
}
#[inline(never)]
fn dag2(value: u8) -> u8 {
    let answer = dag1(value);
    let _checked = dag1(value);
    answer
}
#[inline(never)]
fn dag3(value: u8) -> u8 {
    let answer = dag2(value);
    let _checked = dag2(value);
    answer
}
#[inline(never)]
fn dag4(value: u8) -> u8 {
    let answer = dag3(value);
    let _checked = dag3(value);
    answer
}
#[inline(never)]
fn dag5(value: u8) -> u8 {
    let answer = dag4(value);
    let _checked = dag4(value);
    answer
}
#[inline(never)]
fn dag6(value: u8) -> u8 {
    let answer = dag5(value);
    let _checked = dag5(value);
    answer
}
#[inline(never)]
fn dag7(value: u8) -> u8 {
    let answer = dag6(value);
    let _checked = dag6(value);
    answer
}
#[inline(never)]
fn seven(_value: u8) -> u8 {
    7
}
#[chialisp_oxide::puzzle]
pub fn guarded(value: u8) -> u8 {
    dag7(value)
}
#[chialisp_oxide::puzzle]
pub fn unused_argument(value: u8) -> u8 {
    seven(dag7(value))
}
#[chialisp_oxide::puzzle]
pub fn unused_initializer(value: u8) -> u8 {
    let _checked = dag7(value);
    7
}
#[chialisp_oxide::puzzle]
pub fn copied_result(value: u8) -> u8 {
    let next = dag7(value);
    next + next
}
