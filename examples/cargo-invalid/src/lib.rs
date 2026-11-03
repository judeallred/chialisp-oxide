//! Intentionally failing compiler fixture; never use with real funds.

// Valid native Rust, deliberately unsupported by the current CLVM lowering.
#[cfg(not(feature = "type-error"))]
#[chialisp_oxide::puzzle]
pub fn unsupported_cast(value: u64) -> u128 {
    value as u128
}

// With this feature, ordinary Rust checking must reject the nominal mismatch
// before any IR or CLVM artifact can be emitted.
#[cfg(feature = "type-error")]
#[chialisp_oxide::puzzle]
pub fn wrong_domain(
    coin: chialisp_oxide::CoinId,
    amount: chialisp_oxide::Amount,
) -> chialisp_oxide::CreateCoin {
    chialisp_oxide::create_coin(coin, amount)
}

#[cfg(all(test, not(feature = "type-error")))]
mod tests {
    #[test]
    fn widening_cast_is_valid_native_rust() {
        assert_eq!(super::unsupported_cast(u64::MAX), u128::from(u64::MAX));
    }
}
