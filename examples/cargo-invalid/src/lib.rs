//! Intentionally failing compiler fixture; never use with real funds.

// Valid native Rust, deliberately unsupported by the current CLVM lowering.
#[cfg(not(feature = "type-error"))]
#[oxide_sdk::puzzle]
pub fn unsupported_divide(numerator: u64, denominator: u64) -> u64 {
    numerator / denominator
}

// With this feature, ordinary Rust checking must reject the nominal mismatch
// before any IR or CLVM artifact can be emitted.
#[cfg(feature = "type-error")]
#[oxide_sdk::puzzle]
pub fn wrong_domain(coin: oxide_sdk::CoinId, amount: oxide_sdk::Amount) -> oxide_sdk::CreateCoin {
    oxide_sdk::create_coin(coin, amount)
}

#[cfg(all(test, not(feature = "type-error")))]
mod tests {
    #[test]
    fn division_is_valid_native_rust() {
        assert_eq!(super::unsupported_divide(6, 2), 3);
    }
}
