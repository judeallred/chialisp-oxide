//! Real Cargo source controls for the strict atom inequality export.
//!
//! Negative controls deliberately change the source behavior; operand swapping
//! is a positive equivalence control. These functions have the same strict ABI.
//! This crate and its native helper perform no compiler or backend calls.

pub mod constant_false;
pub mod constant_true;
pub mod operator_eq;
pub mod self_inequality;
pub mod swap_operands;

#[cfg(test)]
mod tests {
    use super::*;

    fn observe(left: &[u8], right: &[u8]) -> [bool; 5] {
        [
            operator_eq::puzzle(left, right),
            self_inequality::puzzle(left, right),
            constant_true::puzzle(left, right),
            constant_false::puzzle(left, right),
            swap_operands::puzzle(left, right),
        ]
    }

    #[test]
    fn equal_and_different_payloads_distinguish_the_source_controls() {
        let equal = [true, false, true, false, false];
        let different = [false, false, true, false, true];
        for (left, right, expected) in [
            (&b""[..], &b""[..], equal),
            (&b""[..], &b"\x00"[..], different),
            (&b"\x00"[..], &b"\x00"[..], equal),
            (&b"\x00"[..], &b"\x00\x00"[..], different),
            (&b"\x01"[..], &b"\x00\x01"[..], different),
            (&b"\xff"[..], &b"\xff\xff"[..], different),
            (&b"a\0b"[..], &b"ab"[..], different),
            (&b"\xff\xfe"[..], &b"\xff\xfe"[..], equal),
            (&b"\xc3\xa9"[..], &b"e\xcc\x81"[..], different),
        ] {
            assert_eq!(observe(left, right), expected);
        }
    }

    #[test]
    fn eight_kibibyte_inputs_preserve_equal_length_and_edge_differences() {
        let left = vec![0x42; 8192];
        let mut right = left.clone();
        assert_eq!(observe(&left, &right), [true, false, true, false, false]);
        right[8191] = 0x43;
        assert_eq!(observe(&left, &right), [false, false, true, false, true]);
        right[8191] = 0x42;
        right[0] = 0x43;
        assert_eq!(observe(&left, &right), [false, false, true, false, true]);
        assert_eq!(
            observe(&left, &right[..8191]),
            [false, false, true, false, true]
        );
    }
}
