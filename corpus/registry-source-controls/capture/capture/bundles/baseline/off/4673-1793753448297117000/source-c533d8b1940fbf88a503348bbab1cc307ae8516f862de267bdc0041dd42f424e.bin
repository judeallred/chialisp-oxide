//! Original teaching fixture: run an inner puzzle only if it matches a commitment.
//!
//! The committed hash is the first argument, so a parent curries it into this
//! puzzle before creating the coin; the solution then supplies the inner puzzle
//! and its solution. The tree hash of the supplied inner puzzle must equal the
//! commitment before `apply` runs it. This closes the delegated-puzzle gap for
//! program identity only: it does not authorize who may spend.
use chialisp_oxide::{Tree, apply, sha256_parts};

const ATOM_DOMAIN: [u8; 1] = [1];
const PAIR_DOMAIN: [u8; 1] = [2];

#[chialisp_oxide::puzzle]
pub fn puzzle<'a>(
    inner_puzzle_hash: [u8; 32],
    inner_puzzle: &'a Tree<'a>,
    inner_solution: &'a Tree<'a>,
) -> Tree<'static> {
    assert!(sha256tree(inner_puzzle) == inner_puzzle_hash);
    apply(inner_puzzle, inner_solution)
}

fn sha256tree<'a>(tree: &'a Tree<'a>) -> [u8; 32] {
    match tree {
        Tree::Atom(payload) => sha256_parts(&ATOM_DOMAIN, payload, &[]),
        Tree::Pair(first, rest) => {
            let first_hash = sha256tree(first);
            let rest_hash = sha256tree(rest);
            sha256_parts(&PAIR_DOMAIN, &first_hash, &rest_hash)
        }
    }
}
