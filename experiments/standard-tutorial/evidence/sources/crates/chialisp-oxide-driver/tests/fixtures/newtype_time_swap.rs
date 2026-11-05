#[path = "../../../../sdk/chialisp_oxide.rs"]
mod chialisp_oxide;
pub fn puzzle(amount: chialisp_oxide::Amount) -> chialisp_oxide::AssertSecondsAbsolute {
    chialisp_oxide::assert_seconds_absolute(amount)
}
