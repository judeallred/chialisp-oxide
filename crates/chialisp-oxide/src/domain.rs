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
    pub const SEND_MESSAGE: u64 = 66;
    pub const ASSERT_MY_AMOUNT: u64 = 73;
    pub const ASSERT_SECONDS_ABSOLUTE: u64 = 81;
    pub const ASSERT_BEFORE_SECONDS_ABSOLUTE: u64 = 85;
}

pub type CreateCoin = (u64, PuzzleHash, Amount);

/// Exact proper-list wire: (51 destination amount (hint)).
pub type CreateCoinWithHint = (u64, PuzzleHash, Amount, (PuzzleHash,));
/// Exact proper-list wire: (66 18 () receiver_puzzle_hash).
/// Unit is nil; arbitrary modes and arbitrary messages are outside this helper.
pub type SendPuzzleMessage = (u64, u64, (), PuzzleHash);

/// SENDER_PUZZLE (2) | RECEIVER_PUZZLE (16).
pub const PUZZLE_TO_PUZZLE_MESSAGE_MODE: u64 = 18;

pub fn create_coin_with_hint(
    destination: PuzzleHash,
    amount: Amount,
    hint: PuzzleHash,
) -> CreateCoinWithHint {
    (opcode::CREATE_COIN, destination, amount, (hint,))
}

pub fn send_puzzle_message(receiver: PuzzleHash) -> SendPuzzleMessage {
    (
        opcode::SEND_MESSAGE,
        PUZZLE_TO_PUZZLE_MESSAGE_MODE,
        (),
        receiver,
    )
}

pub type AggSigMe = (u64, PublicKey, Message32);
pub type AssertMyAmount = (u64, Amount);
pub type AssertSecondsAbsolute = (u64, Timestamp);
pub type AssertBeforeSecondsAbsolute = (u64, Timestamp);

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

/// Require the consensus timestamp to be strictly before this absolute deadline.
pub fn assert_before_seconds_absolute(deadline: Timestamp) -> AssertBeforeSecondsAbsolute {
    (opcode::ASSERT_BEFORE_SECONDS_ABSOLUTE, deadline)
}

#[cfg(test)]
mod tests {
    use super::{AssertBeforeSecondsAbsolute, Timestamp, assert_before_seconds_absolute};

    #[test]
    fn hinted_create_coin_keeps_zero_mojos_and_one_proper_hint() {
        let destination = super::PuzzleHash([0x11; 32]);
        let hint = super::PuzzleHash([0x22; 32]);
        let condition: super::CreateCoinWithHint =
            super::create_coin_with_hint(destination, super::Amount::from_mojos(0), hint);
        // A Rust one-tuple is a proper one-item memo list in the exact ABI.
        assert_eq!(condition.0, 51);
        assert_eq!(condition.1.0, [0x11; 32]);
        assert_eq!(condition.2.mojos(), 0);
        assert_eq!(condition.3.0.0, [0x22; 32]);
    }

    #[test]
    fn puzzle_message_has_fixed_mode_and_nil_unit_payload() {
        let receiver = super::PuzzleHash([0x33; 32]);
        let condition: super::SendPuzzleMessage = super::send_puzzle_message(receiver);
        let (opcode, mode, (), destination) = condition;
        assert_eq!(opcode, 66);
        assert_eq!(mode, 18);
        assert_eq!(destination.0, [0x33; 32]);
    }

    #[test]
    fn before_seconds_absolute_has_the_condition_wire_opcode_and_seconds() {
        let condition: AssertBeforeSecondsAbsolute =
            assert_before_seconds_absolute(Timestamp::from_unix_seconds(100));
        assert_eq!(condition.0, 85);
        assert_eq!(condition.1.unix_seconds(), 100);
    }
}
