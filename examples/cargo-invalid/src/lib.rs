//! Intentionally failing compiler fixture; never use with real funds.

// Valid native Rust, deliberately outside the primitive-integer cast profile.
// bool-to-integer conversion is distinct from i/u8..128 integer casts.
#[cfg(not(feature = "type-error"))]
#[chialisp_oxide::puzzle]
pub fn unsupported_bool_cast(value: u64) -> u8 {
    (value != 0) as u8
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
    fn bool_cast_is_valid_native_rust() {
        assert_eq!(super::unsupported_bool_cast(0), 0);
        assert_eq!(super::unsupported_bool_cast(1), 1);
        assert_eq!(super::unsupported_bool_cast(u64::MAX), 1);
    }
}
