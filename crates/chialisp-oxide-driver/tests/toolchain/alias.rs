#![no_std]
#![no_implicit_prelude]

extern crate replacement as byte_core;

#[doc = "__oxide_puzzle_v1"]
pub fn entry(left: &[u8], right: &[u8]) -> bool {
    byte_core::cmp::PartialEq::ne(left, right)
}
