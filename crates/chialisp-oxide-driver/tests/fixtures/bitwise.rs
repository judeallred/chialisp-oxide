const READ: u8 = 0b1;
const WRITE: u8 = 0b10;
const KNOWN: u8 = READ | WRITE;

#[chialisp_oxide::puzzle]
pub fn validate_flags(flags: u8) -> bool {
    assert!((flags & !KNOWN) == 0);
    (flags & WRITE) != 0
}

#[chialisp_oxide::puzzle]
pub fn comparison_is_true(flags: u8) -> bool {
    ((flags & WRITE) != 0) == true
}
#[chialisp_oxide::puzzle]
pub fn comparison_tuple(flags: u8) -> (bool, bool) {
    ((flags & WRITE) != 0, (flags & READ) != 0)
}
#[chialisp_oxide::puzzle]
pub fn comparison_nested(flags: u8) -> bool {
    ((flags & WRITE) != 0) & !((flags & READ) != 0)
}

// Finite-width adapter for the published `(defun odd (X) (logand X 1))`.
// Its strict i64 ABI is deliberately narrower than the original CLVM domain.
#[chialisp_oxide::puzzle]
pub fn odd(value: i64) -> i64 {
    value & 1
}

#[chialisp_oxide::puzzle]
pub fn bool_ops(a: bool, b: bool) -> (bool, bool, bool, bool) {
    (a & b, a | b, a ^ b, !a)
}
fn fail_bool() -> bool {
    panic!()
}
#[chialisp_oxide::puzzle]
pub fn eager_and(flag: bool) -> bool {
    flag & fail_bool()
}
#[chialisp_oxide::puzzle]
pub fn lazy_and(flag: bool) -> bool {
    flag && fail_bool()
}
#[chialisp_oxide::puzzle]
pub fn eager_or(flag: bool) -> bool {
    flag | fail_bool()
}
#[chialisp_oxide::puzzle]
pub fn lazy_or(flag: bool) -> bool {
    flag || fail_bool()
}
fn overflow(value: u8) -> u8 {
    value + 1
}
fn mask(value: u8) -> u8 {
    value & 3
}
#[chialisp_oxide::puzzle]
pub fn unused(value: u8) -> u8 {
    let _ = overflow(value) ^ 0;
    7
}
#[chialisp_oxide::puzzle]
pub fn unused_not(value: u8) -> u8 {
    let _ = !overflow(value);
    7
}
#[chialisp_oxide::puzzle]
pub fn unused_helper(value: u8) -> u8 {
    let _ = mask(overflow(value));
    7
}
#[chialisp_oxide::puzzle]
pub fn lazy(value: u8, flag: bool) -> u8 {
    if flag { overflow(value) & 3 } else { 7 }
}
fn descend(depth: u8) {
    if depth > 0 {
        descend(depth - 1)
    }
}
#[chialisp_oxide::puzzle]
pub fn runtime_snapshot(value: u8) -> (u8, u8) {
    let mut current = value;
    let before = current & 0x0f;
    current ^= 0xff;
    descend(2);
    (before, current)
}
#[chialisp_oxide::puzzle]
pub fn recursive(value: u8, depth: u8) -> u8 {
    if depth == 0 {
        !value
    } else {
        recursive(value, depth - 1) ^ 1
    }
}
#[allow(unconditional_recursion)]
fn spin() -> u8 {
    spin()
}
fn fail() -> u8 {
    panic!()
}
#[chialisp_oxide::puzzle]
pub fn reject_then_spin() -> u8 {
    fail() & spin()
}
#[chialisp_oxide::puzzle]
pub fn spin_then_reject() -> u8 {
    spin() | fail()
}
#[allow(unconditional_recursion)]
fn spin_bool() -> bool {
    spin_bool()
}
#[chialisp_oxide::puzzle]
pub fn eager_false_spin() -> bool {
    false & spin_bool()
}
#[chialisp_oxide::puzzle]
pub fn eager_true_spin() -> bool {
    true | spin_bool()
}
#[chialisp_oxide::puzzle]
pub fn lazy_false_spin() -> bool {
    false && spin_bool()
}
#[chialisp_oxide::puzzle]
pub fn lazy_true_spin() -> bool {
    true || spin_bool()
}
mod names {
    pub fn bitand(a: u8, b: u8) -> u8 {
        a | b
    }
    pub fn not(value: u8) -> u8 {
        value
    }
}
#[chialisp_oxide::puzzle]
pub fn misleading_names(a: u8, b: u8) -> (u8, u8) {
    (names::bitand(a, b), names::not(a))
}

#[cfg(test)]
mod native {
    use super::*;
    #[test]
    fn exact_mask_policy_and_eager_boolean_operators() {
        for flags in 0..=u8::MAX {
            assert_eq!(comparison_is_true(flags), flags & WRITE == WRITE);
            assert_eq!(
                comparison_tuple(flags),
                (flags & WRITE == WRITE, flags & READ == READ)
            );
            assert_eq!(
                comparison_nested(flags),
                flags & WRITE == WRITE && flags & READ == 0
            );
            let result = std::panic::catch_unwind(|| validate_flags(flags));
            if flags <= 3 {
                assert_eq!(result.unwrap(), flags == 2 || flags == 3);
            } else {
                assert!(result.is_err());
            }
        }
        for a in [false, true] {
            for b in [false, true] {
                assert_eq!(bool_ops(a, b), (a && b, a || b, a != b, !a));
            }
        }
        for flag in [false, true] {
            assert!(std::panic::catch_unwind(|| eager_and(flag)).is_err());
            assert!(std::panic::catch_unwind(|| eager_or(flag)).is_err());
        }
        assert!(!lazy_and(false));
        assert!(lazy_or(true));
        assert!(!lazy_false_spin());
        assert!(lazy_true_spin());
        assert!(std::panic::catch_unwind(|| lazy_and(true)).is_err());
        assert!(std::panic::catch_unwind(|| lazy_or(false)).is_err());
        assert_eq!(runtime_snapshot(0xa5), (5, 0x5a));
        assert_eq!(recursive(0xa5, 3), 0x5b);
        assert_eq!(misleading_names(1, 2), (3, 1));
        assert_eq!(lazy(255, false), 7);
        assert_eq!(odd(i64::MIN), 0);
        assert_eq!(odd(i64::MAX), 1);
        assert_eq!(odd(-3), 1);
        for function in [unused, unused_not, unused_helper] {
            assert_eq!(function(0), 7);
            assert!(std::panic::catch_unwind(|| function(255)).is_err());
        }
    }
}
