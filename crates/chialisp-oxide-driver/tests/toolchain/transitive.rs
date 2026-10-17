#![no_std]
#![no_implicit_prelude]

pub extern crate replacement_dependency;

#[doc = "__oxide_puzzle_v1"]
pub fn entry(left: &[u8], right: &[u8]) -> bool {
    left != right
}
