//! Types and pure helpers shared by native Rust tests and CLVM compilation.
//!
//! Nominal types prevent source-level domain swaps. Their public constructors
//! establish no cryptographic or protocol invariant. The CLVM ABI validates the
//! inner representation, not a runtime nominal tag or constructor predicate.
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

pub use chialisp_oxide_macros::puzzle;

mod domain;
pub use domain::*;

mod tree;
pub use tree::*;

mod cons;
pub use cons::Cons;

#[cfg(feature = "alloc")]
mod owned_tree;
#[cfg(feature = "alloc")]
pub use owned_tree::*;

mod hashing;
pub use hashing::{sha256, sha256_parts};

mod apply;
pub use apply::{ResourceExhausted, apply};
