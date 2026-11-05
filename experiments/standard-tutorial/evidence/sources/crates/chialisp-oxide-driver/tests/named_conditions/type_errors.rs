use chialisp_oxide::*;

#[chialisp_oxide::puzzle]
pub fn absolute_height_rejects_age(value: BlockAge) -> AssertHeightAbsolute {
    assert_height_absolute(value)
}

#[chialisp_oxide::puzzle]
pub fn relative_height_rejects_height(value: BlockHeight) -> AssertHeightRelative {
    assert_height_relative(value)
}

#[chialisp_oxide::puzzle]
pub fn before_absolute_rejects_age(value: BlockAge) -> AssertBeforeHeightAbsolute {
    assert_before_height_absolute(value)
}

#[chialisp_oxide::puzzle]
pub fn before_relative_rejects_height(value: BlockHeight) -> AssertBeforeHeightRelative {
    assert_before_height_relative(value)
}

#[chialisp_oxide::puzzle]
pub fn announcement_rejects_coin_id(value: CoinId) -> AssertCoinAnnouncement {
    assert_coin_announcement(value)
}

#[chialisp_oxide::puzzle]
pub fn own_id_rejects_announcement(value: CoinAnnouncementId) -> AssertMyCoinId {
    assert_my_coin_id(value)
}

#[chialisp_oxide::puzzle]
pub fn message_rejects_coin_id(value: CoinId) -> CreateCoinAnnouncement {
    create_coin_announcement(value)
}

#[chialisp_oxide::puzzle]
pub fn origin_rejects_puzzle_hash(value: PuzzleHash, message: Message32) -> CoinAnnouncementId {
    coin_announcement_id(value, message)
}
