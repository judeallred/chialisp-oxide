//! Offline package identity and local immutable storage for CLVM artifacts.
//!
//! WIP prototype. Identity consistency does not establish source equivalence,
//! compiler preservation, authorization, an audit, or suitability for real funds.
pub mod artifact;
pub mod bundle;
mod digest;
mod error;
pub(crate) mod fs;
pub(crate) mod json;
mod limits;
pub mod package;
pub mod store;
pub use digest::{Digest32, digest};
pub use error::{Error, ErrorKind, Result};
pub use limits::Limits;
pub use package::{PackageManifest, VerifiedPackage, verify_package, verify_package_file};
