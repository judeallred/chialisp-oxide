//! Standalone-harness compatibility path to the real SDK domain definitions.
//!
//! Normal Cargo libraries depend on `chialisp-oxide`. This ordinary module path
//! shares the same source without generating newtype declarations in a macro.
#![allow(dead_code)]

#[path = "../crates/chialisp-oxide/src/domain.rs"]
mod domain;
pub use domain::*;
