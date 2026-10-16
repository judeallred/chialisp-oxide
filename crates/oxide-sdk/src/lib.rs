//! Types and pure helpers shared by native Rust tests and CLVM compilation.
//!
//! Nominal types prevent source-level domain swaps. Their public constructors
//! establish no cryptographic or protocol invariant. The CLVM ABI validates the
//! inner representation, not a runtime nominal tag or constructor predicate.
#![no_std]

pub use oxide_macros::puzzle;

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
