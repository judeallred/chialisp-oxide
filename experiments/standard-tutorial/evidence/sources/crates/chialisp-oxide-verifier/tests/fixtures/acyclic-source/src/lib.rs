//! WIP native/actual-source controls. The current frontend emits these as
//! legacy expression IR; runtime-table adapters are separately constructed.
#[inline(never)]
fn capped_increment(value: u8) -> u8 {
    assert!(value < 100);
    value + 1
}
#[inline(never)]
fn uncapped_increment(value: u8) -> u8 {
    value + 1
}
#[inline(never)]
fn seven(_value: u8) -> u8 {
    7
}
#[chialisp_oxide::puzzle]
pub fn helper_guard(value: u8) -> u8 {
    capped_increment(value)
}
#[chialisp_oxide::puzzle]
pub fn missing_guard(value: u8) -> u8 {
    uncapped_increment(value)
}
#[chialisp_oxide::puzzle]
pub fn unused_argument(value: u8) -> u8 {
    seven(value + 1)
}
#[chialisp_oxide::puzzle]
pub fn missing_argument_check(value: u8) -> u8 {
    seven(value)
}
#[chialisp_oxide::puzzle]
pub fn unused_initializer(value: u8) -> u8 {
    let _checked_value = value + 1;
    seven(value)
}
#[chialisp_oxide::puzzle]
pub fn missing_initializer(value: u8) -> u8 {
    seven(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_u8_native_inputs_match_independent_checked_policies() {
        std::panic::set_hook(Box::new(|_| {}));
        for value in 0..=u8::MAX {
            assert_eq!(
                std::panic::catch_unwind(|| helper_guard(value)).ok(),
                if value < 100 {
                    value.checked_add(1)
                } else {
                    None
                }
            );
            assert_eq!(
                std::panic::catch_unwind(|| missing_guard(value)).ok(),
                value.checked_add(1)
            );
            for function in [unused_argument, unused_initializer] {
                assert_eq!(
                    std::panic::catch_unwind(|| function(value)).ok(),
                    value.checked_add(1).map(|_| 7)
                );
            }
            assert_eq!(missing_argument_check(value), 7);
            assert_eq!(missing_initializer(value), 7);
        }
    }
}
