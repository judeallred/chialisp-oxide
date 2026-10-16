//! WIP consumer fixture. Never use these teaching puzzles with real funds.

use oxide_sdk::{Amount, AssertMyAmount, CreateCoin, PuzzleHash, assert_my_amount, create_coin};

// Cargo executes the host build script before rustc expands this source.
include!(concat!(env!("OUT_DIR"), "/limits.rs"));

/// Permissionless fixed-destination payment; any remainder becomes a fee.
#[oxide_sdk::puzzle]
pub fn pay(amount: Amount) -> (AssertMyAmount, CreateCoin) {
    assert!(amount.0 <= PAYMENT_LIMIT);
    (
        assert_my_amount(Amount(1000)),
        create_coin(PuzzleHash([0x42; 32]), amount),
    )
}

/// Pure arithmetic example, not a condition-producing coin puzzle.
#[oxide_sdk::puzzle]
pub fn guarded_add(left: u64, right: u64) -> u64 {
    assert!(left <= PAYMENT_LIMIT);
    assert!(right <= PAYMENT_LIMIT);
    left + right
}

/// Ordinary native Rust deliberately outside the accepted CLVM subset.
/// This function is not marked or reachable from either puzzle entrypoint.
pub fn native_description(amounts: &[u64]) -> String {
    let rendered: Vec<String> = amounts
        .iter()
        .map(|amount| format!("{amount} mojo"))
        .collect();
    rendered.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_script_and_feature_select_the_limit() {
        #[cfg(feature = "strict-cap")]
        assert_eq!(PAYMENT_LIMIT, 100);
        #[cfg(not(feature = "strict-cap"))]
        assert_eq!(PAYMENT_LIMIT, 1000);
    }

    #[test]
    fn payment_returns_the_exact_conditions_at_both_boundaries() {
        for amount in [0, PAYMENT_LIMIT] {
            let (assertion, payment) = pay(Amount(amount));
            assert_eq!(assertion.0, 73);
            assert_eq!(assertion.1.0, 1000);
            assert_eq!(payment.0, 51);
            assert_eq!(payment.1.0, [0x42; 32]);
            assert_eq!(payment.2.0, amount);
        }
    }

    #[test]
    fn payment_rejects_an_amount_above_the_generated_limit() {
        assert!(std::panic::catch_unwind(|| pay(Amount(PAYMENT_LIMIT + 1))).is_err());
    }

    #[test]
    fn arithmetic_entry_adds_allowed_values_and_rejects_either_bad_argument() {
        assert_eq!(guarded_add(0, PAYMENT_LIMIT), PAYMENT_LIMIT);
        assert_eq!(guarded_add(PAYMENT_LIMIT, PAYMENT_LIMIT), PAYMENT_LIMIT * 2);
        assert!(std::panic::catch_unwind(|| guarded_add(PAYMENT_LIMIT + 1, 0)).is_err());
        assert!(std::panic::catch_unwind(|| guarded_add(0, PAYMENT_LIMIT + 1)).is_err());
    }

    #[test]
    fn unannotated_native_code_uses_std_and_allocation_normally() {
        assert_eq!(native_description(&[42, 100]), "42 mojo, 100 mojo");
        assert_eq!(native_description(&[]), "");
    }
}
