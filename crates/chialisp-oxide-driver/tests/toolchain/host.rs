#![no_std]

// Native-valid, deliberately outside the current exported CLVM subset. Host
// routing must not interpret this marker as a request to publish target IR.
#[doc = "__oxide_puzzle_v1"]
pub fn entry(value: u64) -> u64 {
    value / 2
}
