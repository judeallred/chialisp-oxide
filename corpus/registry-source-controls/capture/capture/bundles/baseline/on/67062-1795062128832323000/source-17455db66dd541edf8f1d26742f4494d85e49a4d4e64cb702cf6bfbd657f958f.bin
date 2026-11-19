//! Published sha256tree helper over the complete raw environment.
use chialisp_oxide::{Tree, sha256_parts};

const ATOM_DOMAIN: [u8; 1] = [1];
const PAIR_DOMAIN: [u8; 1] = [2];

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn puzzle<'a>(environment: &'a Tree<'a>) -> [u8; 32] {
    sha256tree(environment)
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
