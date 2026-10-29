//! WIP public lifecycle journal shared by the CLI and loopback GUI adapters.
//! The generic journal records external assertions. The separate fixed runner
//! actually executes current native code and retained CLVM/aggregate validation
//! in the SDK simulator. The separate fresh adapter invokes explicit installed
//! Cargo/compiler tools and an independent reference compiler; it is now
//! connected through a private executed recipe to the journal/GUI. A separate
//! live adapter exposes narrow Testnet11 wallet/submission operations with a
//! public outbox; the existing HTTP interface remains a loopback simulator.
pub mod fresh;
pub mod http;
pub mod live;
mod model;
pub mod provider;
pub mod runner;
pub mod store;
mod types;

pub use model::{Journal, artifact_digest, digest};
pub use types::*;

#[cfg(test)]
mod tests;
