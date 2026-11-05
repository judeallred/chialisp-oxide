//! Actual source for bounded acyclic table fallback controls.
#[inline(never)]
fn leaf(value: u8) -> u8 {
    assert!(value < 100);
    value + 1
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
fn overflow_leaf(value: u8) -> u8 {
    value + 1
}
#[inline(never)]
fn overflow_dag1(value: u8) -> u8 {
    let answer = overflow_leaf(value);
    let _checked = overflow_leaf(value);
    answer
}
#[inline(never)]
fn overflow_dag2(value: u8) -> u8 {
    let answer = overflow_dag1(value);
    let _checked = overflow_dag1(value);
    answer
}
#[inline(never)]
fn overflow_dag3(value: u8) -> u8 {
    let answer = overflow_dag2(value);
    let _checked = overflow_dag2(value);
    answer
}
#[inline(never)]
fn overflow_dag4(value: u8) -> u8 {
    let answer = overflow_dag3(value);
    let _checked = overflow_dag3(value);
    answer
}
#[inline(never)]
fn overflow_dag5(value: u8) -> u8 {
    let answer = overflow_dag4(value);
    let _checked = overflow_dag4(value);
    answer
}
#[inline(never)]
fn overflow_dag6(value: u8) -> u8 {
    let answer = overflow_dag5(value);
    let _checked = overflow_dag5(value);
    answer
}
#[inline(never)]
fn overflow_dag7(value: u8) -> u8 {
    let answer = overflow_dag6(value);
    let _checked = overflow_dag6(value);
    answer
}
#[inline(never)]
fn seven(_value: u8) -> u8 {
    7
}
#[chialisp_oxide::puzzle]
pub fn dag_guard(value: u8) -> u8 {
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
pub fn unused_overflow(value: u8) -> u8 {
    seven(overflow_dag7(value))
}
#[chialisp_oxide::puzzle]
pub fn copied_result(value: u8) -> u8 {
    let next = dag7(value);
    next + next
}
#[chialisp_oxide::puzzle]
pub fn lazy(value: u8, choose: bool) -> u8 {
    if choose { dag7(value) } else { 7 }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_native_domain_matches_independent_policy() {
        std::panic::set_hook(Box::new(|_| {}));
        for value in 0..=u8::MAX {
            let guarded = if value < 100 {
                value.checked_add(1)
            } else {
                None
            };
            assert_eq!(std::panic::catch_unwind(|| dag_guard(value)).ok(), guarded);
            assert_eq!(
                std::panic::catch_unwind(|| copied_result(value)).ok(),
                guarded.and_then(|n| n.checked_add(n))
            );
            for function in [unused_argument, unused_initializer] {
                assert_eq!(
                    std::panic::catch_unwind(|| function(value)).ok(),
                    guarded.map(|_| 7)
                );
            }
            assert_eq!(
                std::panic::catch_unwind(|| unused_overflow(value)).ok(),
                value.checked_add(1).map(|_| 7)
            );
            assert_eq!(std::panic::catch_unwind(|| lazy(value, true)).ok(), guarded);
            assert_eq!(lazy(value, false), 7);
        }
    }
}
