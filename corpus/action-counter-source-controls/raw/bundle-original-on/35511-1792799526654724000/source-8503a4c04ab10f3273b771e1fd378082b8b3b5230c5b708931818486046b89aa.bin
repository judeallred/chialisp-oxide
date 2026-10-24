//! WIP raw-domain translation of canonical p2_delegated_conditions.
//!
//! This raw module accepts an attacker-chosen public-key tree. A separately
//! committed key and real signature/consensus checks are needed for a coin policy.
use chialisp_oxide::{Cons, Tree, opcode, sha256_parts};

const ATOM_DOMAIN: [u8; 1] = [1];
const PAIR_DOMAIN: [u8; 1] = [2];

type RawSignature<'a> = (u64, &'a Tree<'a>, [u8; 32]);
pub type DelegatedConditions<'a> = Cons<RawSignature<'a>, &'a Tree<'a>>;

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn puzzle<'a>(environment: &'a Tree<'a>) -> DelegatedConditions<'a> {
    match environment {
        Tree::Pair(public_key, rest) => match rest {
            Tree::Pair(conditions, _) => Cons {
                first: (opcode::AGG_SIG_ME, public_key, tree_hash(conditions)),
                rest: conditions,
            },
            Tree::Atom(_) => panic!(),
        },
        Tree::Atom(_) => panic!(),
    }
}

fn tree_hash<'a>(tree: &'a Tree<'a>) -> [u8; 32] {
    match tree {
        Tree::Atom(payload) => sha256_parts(&ATOM_DOMAIN, payload, &[]),
        Tree::Pair(first, rest) => {
            let first_hash = tree_hash(first);
            let rest_hash = tree_hash(rest);
            sha256_parts(&PAIR_DOMAIN, &first_hash, &rest_hash)
        }
    }
}
