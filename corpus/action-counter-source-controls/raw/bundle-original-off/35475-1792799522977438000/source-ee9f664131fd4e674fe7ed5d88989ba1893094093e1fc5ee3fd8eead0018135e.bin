//! Original teaching fixture: run an untrusted inner puzzle on its solution.
//!
//! `apply` is CLVM's `a` operator. The inner puzzle and its solution both come
//! from the untrusted environment; whatever the inner program returns is this
//! puzzle's result, and an inner failure rejects the whole spend. Nothing here
//! checks who supplied the inner puzzle: that is the delegated-puzzle pattern's
//! authorization gap, which signature and hash commitments must close elsewhere.
use chialisp_oxide::{Tree, apply};

#[chialisp_oxide::puzzle]
pub fn puzzle<'a>(inner_puzzle: &'a Tree<'a>, inner_solution: &'a Tree<'a>) -> Tree<'static> {
    apply(inner_puzzle, inner_solution)
}
