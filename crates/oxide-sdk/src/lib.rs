//! Types and pure helpers shared by native Rust tests and CLVM compilation.
//!
//! Nominal types prevent source-level domain swaps. Their public constructors
//! establish no cryptographic or protocol invariant. The CLVM ABI validates the
//! inner representation, not a runtime nominal tag or constructor predicate.
#![no_std]

pub use oxide_macros::puzzle;

mod domain;
pub use domain::*;
