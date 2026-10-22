//! Ordinary Cargo exports exercising named condition helpers, not a spending policy.
use chialisp_oxide::{
    AssertBeforeHeightAbsolute, AssertBeforeHeightRelative, AssertCoinAnnouncement,
    AssertHeightAbsolute, AssertHeightRelative, AssertMyCoinId, BlockAge, BlockHeight,
    CoinAnnouncementId, CoinId, CreateCoinAnnouncement, Message32, assert_before_height_absolute,
    assert_before_height_relative, assert_coin_announcement, assert_height_absolute,
    assert_height_relative, assert_my_coin_id, coin_announcement_id, create_coin_announcement,
};

pub type HeightConditions = (
    AssertHeightRelative,
    AssertHeightAbsolute,
    AssertBeforeHeightRelative,
    AssertBeforeHeightAbsolute,
);

#[chialisp_oxide::puzzle]
pub fn height_window(
    minimum_age: BlockAge,
    earliest_height: BlockHeight,
    maximum_age: BlockAge,
    latest_height: BlockHeight,
) -> HeightConditions {
    (
        assert_height_relative(minimum_age),
        assert_height_absolute(earliest_height),
        assert_before_height_relative(maximum_age),
        assert_before_height_absolute(latest_height),
    )
}

pub type AnnouncementConditions = (
    CreateCoinAnnouncement,
    AssertCoinAnnouncement,
    AssertMyCoinId,
);

#[chialisp_oxide::puzzle]
pub fn announcement_conditions(origin: CoinId, message: Message32) -> AnnouncementConditions {
    (
        create_coin_announcement(message),
        assert_coin_announcement(coin_announcement_id(origin, message)),
        assert_my_coin_id(origin),
    )
}

/// A nominal ID admits every bytes32 value; it does not prove an announcement exists.
#[chialisp_oxide::puzzle]
pub fn supplied_announcement(id: CoinAnnouncementId) -> (AssertCoinAnnouncement,) {
    (assert_coin_announcement(id),)
}

#[chialisp_oxide::puzzle]
pub fn discarded_announcement(origin: CoinId, message: Message32) -> u8 {
    let _ = coin_announcement_id(origin, message);
    7
}

#[chialisp_oxide::puzzle]
pub fn without_announcement(_: CoinId, _: Message32) -> u8 {
    7
}

#[cfg(test)]
#[path = "native.rs"]
mod native;
