//! Original teaching fixture: the puzzle hash of a module curried with one parameter.
//!
//! Coins commit to puzzle hashes, so a parent that creates a child with a
//! curried inner puzzle needs this hash without materializing the program.
//! The SDK helpers port the pinned `curry-and-treehash.clib` arithmetic over the
//! reviewed SHA-256 bridge; this fixture takes the module's and parameter's
//! tree hashes as fixed 32-byte arguments and returns the curried puzzle hash.
use chialisp_oxide::curry::curried_puzzle_hash;

#[chialisp_oxide::puzzle]
pub fn puzzle(module_hash: [u8; 32], parameter_hash: [u8; 32]) -> [u8; 32] {
    curried_puzzle_hash(&module_hash, &parameter_hash)
}
