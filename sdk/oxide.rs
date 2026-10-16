//! Source-only puzzle helpers for the currently supported compiler subset.
//!
//! Include this module in a standalone puzzle. Transparent wrappers retain Rust
//! nominal type checking but erase to their scalar CLVM wire representations.
//! Their constructors validate no cryptographic property; callers still supply
//! untrusted bytes and the ABI validates only encoding, length, and range.
#![allow(dead_code)]

#[repr(transparent)]
pub struct PuzzleHash(pub [u8; 32]);
#[repr(transparent)]
pub struct CoinId(pub [u8; 32]);
#[repr(transparent)]
pub struct PublicKey(pub [u8; 48]);
#[repr(transparent)]
pub struct Message32(pub [u8; 32]);
#[repr(transparent)]
pub struct Amount(pub u64);
#[repr(transparent)]
pub struct Timestamp(pub u64);

pub type CreateCoin = (u64, PuzzleHash, Amount);
pub type AggSigMe = (u64, PublicKey, Message32);
pub type AssertMyAmount = (u64, Amount);
pub type AssertSecondsAbsolute = (u64, Timestamp);

pub fn create_coin(destination: PuzzleHash, amount: Amount) -> CreateCoin {
    (51, destination, amount)
}
pub fn agg_sig_me(public_key: PublicKey, message: Message32) -> AggSigMe {
    (50, public_key, message)
}
pub fn assert_my_amount(amount: Amount) -> AssertMyAmount {
    (73, amount)
}
pub fn assert_seconds_absolute(time: Timestamp) -> AssertSecondsAbsolute {
    (81, time)
}
