//! Standalone-harness compatibility path to the real SDK domain definitions.
//!
//! Normal Cargo libraries depend on `oxide-sdk`. This ordinary module path
//! shares the same source without generating newtype declarations in a macro.
#![allow(dead_code)]

#[path = "../crates/oxide-sdk/src/domain.rs"]
mod domain;
pub use domain::*;
