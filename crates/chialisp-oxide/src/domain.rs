//! Shared domain values and pure condition constructors.

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct PuzzleHash(pub [u8; 32]);
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct CoinId(pub [u8; 32]);
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct PublicKey(pub [u8; 48]);
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Message32(pub [u8; 32]);
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Amount(pub u64);
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Timestamp(pub u64);

impl Amount {
    /// Construct an amount in mojos, Chia's indivisible monetary unit.
    pub const fn from_mojos(mojos: u64) -> Self {
        Self(mojos)
    }

    pub const fn mojos(self) -> u64 {
        self.0
    }

    pub const fn is_at_most(self, limit: Self) -> bool {
        self.0 <= limit.0
    }
}

impl Timestamp {
    /// Construct an absolute Unix timestamp measured in seconds.
    pub const fn from_unix_seconds(seconds: u64) -> Self {
        Self(seconds)
    }

    pub const fn unix_seconds(self) -> u64 {
        self.0
    }

    pub const fn is_at_or_after(self, threshold: Self) -> bool {
        self.0 >= threshold.0
    }
}

/// Wire discriminants; author puzzles with the named condition constructors.
pub mod opcode {
    pub const AGG_SIG_ME: u64 = 50;
    pub const CREATE_COIN: u64 = 51;
    pub const ASSERT_MY_AMOUNT: u64 = 73;
    pub const ASSERT_SECONDS_ABSOLUTE: u64 = 81;
}

pub type CreateCoin = (u64, PuzzleHash, Amount);
pub type AggSigMe = (u64, PublicKey, Message32);
pub type AssertMyAmount = (u64, Amount);
pub type AssertSecondsAbsolute = (u64, Timestamp);

pub fn create_coin(destination: PuzzleHash, amount: Amount) -> CreateCoin {
    (opcode::CREATE_COIN, destination, amount)
}
pub fn agg_sig_me(public_key: PublicKey, message: Message32) -> AggSigMe {
    (opcode::AGG_SIG_ME, public_key, message)
}
pub fn assert_my_amount(amount: Amount) -> AssertMyAmount {
    (opcode::ASSERT_MY_AMOUNT, amount)
}
pub fn assert_seconds_absolute(time: Timestamp) -> AssertSecondsAbsolute {
    (opcode::ASSERT_SECONDS_ABSOLUTE, time)
}
