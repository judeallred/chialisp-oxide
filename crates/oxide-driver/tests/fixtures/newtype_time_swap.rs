#[path = "../../../../sdk/oxide.rs"]
mod oxide;
pub fn puzzle(amount: oxide::Amount) -> oxide::AssertSecondsAbsolute {
    oxide::assert_seconds_absolute(amount)
}
