// Native-only compiler test: prevent an injected pinned core from deduplicating
// the renamed extern. This is not supported exported puzzle source.
#![feature(no_core)]
#![no_core]

extern crate replacement as byte_core;

pub fn entry(left: &[u8], right: &[u8]) -> bool {
    byte_core::cmp::PartialEq::ne(left, right)
}
