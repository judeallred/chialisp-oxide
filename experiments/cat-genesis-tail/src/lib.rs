//! Compatibility import of the shared paired-corpus TAIL policy.
//! WIP prototype; not a complete CAT spend or production-ready program.
#[path = "../../../puzzles/cat-genesis-by-coin-id/puzzle.rs"]
mod policy;
pub use policy::puzzle as genesis_by_coin_id;
