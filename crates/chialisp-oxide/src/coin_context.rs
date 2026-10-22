//! Block-height, coin-age, own-coin-ID and coin-announcement conditions.
//!
//! Kept outside `domain.rs` so the standalone harness can include the domain
//! types without the hashing module. Nominal wrappers make unit and identifier
//! swaps Rust type errors; they establish no consensus fact by themselves.

use crate::{CoinId, Message32, sha256_parts};

/// Wire discriminants for the conditions below.
const CREATE_COIN_ANNOUNCEMENT: u64 = 60;
const ASSERT_COIN_ANNOUNCEMENT: u64 = 61;
const ASSERT_MY_COIN_ID: u64 = 70;
const ASSERT_HEIGHT_RELATIVE: u64 = 82;
const ASSERT_HEIGHT_ABSOLUTE: u64 = 83;
const ASSERT_BEFORE_HEIGHT_RELATIVE: u64 = 86;
const ASSERT_BEFORE_HEIGHT_ABSOLUTE: u64 = 87;

/// An absolute block number, distinct from a duration measured in blocks.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct BlockHeight(pub u32);
/// A relative coin age measured in blocks, distinct from an absolute height.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct BlockAge(pub u32);
/// A coin-origin-bound announcement digest; the wrapper alone authenticates nothing.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct CoinAnnouncementId(pub [u8; 32]);

impl BlockHeight {
    pub const fn from_block_number(number: u32) -> Self {
        Self(number)
    }

    pub const fn block_number(self) -> u32 {
        self.0
    }
}

impl BlockAge {
    pub const fn from_blocks(blocks: u32) -> Self {
        Self(blocks)
    }

    pub const fn blocks(self) -> u32 {
        self.0
    }
}

pub type CreateCoinAnnouncement = (u64, Message32);
pub type AssertCoinAnnouncement = (u64, CoinAnnouncementId);
pub type AssertMyCoinId = (u64, CoinId);
pub type AssertHeightRelative = (u64, BlockAge);
pub type AssertHeightAbsolute = (u64, BlockHeight);
pub type AssertBeforeHeightRelative = (u64, BlockAge);
pub type AssertBeforeHeightAbsolute = (u64, BlockHeight);

/// Emit an announcement with an exact 32-byte message from this spend's coin.
/// This is a fixed-message convenience profile, not the complete consensus byte domain.
pub fn create_coin_announcement(message: Message32) -> CreateCoinAnnouncement {
    (CREATE_COIN_ANNOUNCEMENT, message)
}

/// Require the matching coin announcement during consensus validation.
/// The expected origin and message must be bound by the surrounding policy.
pub fn assert_coin_announcement(announcement: CoinAnnouncementId) -> AssertCoinAnnouncement {
    (ASSERT_COIN_ANNOUNCEMENT, announcement)
}

/// Hash exactly coin_id || message, without a tree-hash prefix or serialization.
/// Computing this value alone does not authenticate the supplied origin coin.
pub fn coin_announcement_id(origin: CoinId, message: Message32) -> CoinAnnouncementId {
    CoinAnnouncementId(sha256_parts(&origin.0, &message.0, &[]))
}

/// Bind a caller-supplied coin identifier to the coin being spent.
pub fn assert_my_coin_id(coin: CoinId) -> AssertMyCoinId {
    (ASSERT_MY_COIN_ID, coin)
}

/// Require at least this relative coin age under the consensus height rules.
pub fn assert_height_relative(age: BlockAge) -> AssertHeightRelative {
    (ASSERT_HEIGHT_RELATIVE, age)
}

/// Require at least this absolute height under the consensus height rules.
pub fn assert_height_absolute(height: BlockHeight) -> AssertHeightAbsolute {
    (ASSERT_HEIGHT_ABSOLUTE, height)
}

/// Require a relative age strictly below the expiry threshold.
pub fn assert_before_height_relative(expiry: BlockAge) -> AssertBeforeHeightRelative {
    (ASSERT_BEFORE_HEIGHT_RELATIVE, expiry)
}

/// Require an absolute height strictly below the deadline.
pub fn assert_before_height_absolute(deadline: BlockHeight) -> AssertBeforeHeightAbsolute {
    (ASSERT_BEFORE_HEIGHT_ABSOLUTE, deadline)
}
