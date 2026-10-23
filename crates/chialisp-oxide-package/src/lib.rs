//! Offline package identity and local immutable storage for CLVM artifacts.
//!
//! WIP prototype. Identity consistency does not establish source equivalence,
//! compiler preservation, authorization, an audit, or suitability for real funds.
pub mod artifact;
pub mod bundle;
pub mod codegen;
mod construction;
pub mod dependency;
mod digest;
mod error;
pub(crate) mod fs;
pub mod inspect;
pub(crate) mod json;
mod limits;
pub mod p2_puzzle_hash;
pub mod package;
pub mod receipt;
pub mod standard;
pub mod store;
mod string_enum;
pub use digest::{Digest32, digest};
pub use error::{Error, ErrorKind, Result};
pub use limits::Limits;
pub use package::{PackageManifest, VerifiedPackage, verify_package, verify_package_file};
