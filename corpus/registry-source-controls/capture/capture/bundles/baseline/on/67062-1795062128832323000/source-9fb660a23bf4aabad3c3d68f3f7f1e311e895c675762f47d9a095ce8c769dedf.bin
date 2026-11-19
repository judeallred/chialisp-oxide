//! Exact raw tutorial policy with delegated and hidden-puzzle paths.
//!
//! Commit SYNTHETIC_PUBLIC_KEY before funding. Returning AGG_SIG_ME requires a
//! later consensus signature; the hidden puzzle must enforce its own policy.
use chialisp_oxide::{OwnedTree, Tree, apply, raw_clvm as raw, sha256_parts};

const ATOM_DOMAIN: [u8; 1] = [1];
const PAIR_DOMAIN: [u8; 1] = [2];
const AGG_SIG_ME_BYTES: [u8; 1] = [50];

fn tree_hash<'a>(tree: &Tree<'a>) -> [u8; 32] {
    match *tree {
        Tree::Atom(bytes) => sha256_parts(&ATOM_DOMAIN, bytes, &[]),
        Tree::Pair(first, rest) => {
            let first_hash = tree_hash(first);
            let rest_hash = tree_hash(rest);
            sha256_parts(&PAIR_DOMAIN, &first_hash, &rest_hash)
        }
    }
}

fn owned<'a>(tree: &Tree<'a>) -> OwnedTree<'a> {
    match *tree {
        Tree::Atom(bytes) => OwnedTree::Atom(bytes),
        Tree::Pair(first, rest) => OwnedTree::Pair(Box::new(owned(first)), Box::new(owned(rest))),
    }
}

/// Check the hidden opening using the unchanged tutorial's signed atom rule.
/// The native codec calls this same source helper before preflighting only an
/// actually selected applied result; it is not an independent crypto oracle.
pub fn hidden_key_matches<'a>(
    synthetic_public_key: &Tree<'a>,
    original_public_key: &Tree<'a>,
    delegated_puzzle: &Tree<'a>,
) -> bool {
    let hidden_hash = tree_hash(delegated_puzzle);
    let original_bytes = match *original_public_key {
        Tree::Atom(bytes) => bytes,
        Tree::Pair(_, _) => panic!(),
    };
    let offset = sha256_parts(original_bytes, &hidden_hash, &[]);
    // Exact signed CLVM atom, reduced modulo group order by the VM primitive.
    let offset_key = raw::pubkey_for_exp(&Tree::Atom(&offset));
    let calculated = raw::point_add(original_public_key, &offset_key);
    let equal = raw::atom_equal(synthetic_public_key, &calculated);
    !matches!(equal, Tree::Atom([]))
}

fn finish<'a>(
    synthetic_public_key: &Tree<'a>,
    original_public_key: &Tree<'a>,
    delegated_puzzle: &Tree<'a>,
    solution: &Tree<'a>,
) -> OwnedTree<'a> {
    match *original_public_key {
        Tree::Atom([]) => {
            // Classic c evaluates the result tail first on the delegated path.
            let conditions = apply(delegated_puzzle, solution);
            let message = tree_hash(delegated_puzzle);
            let message_tree = Tree::Atom(&message);
            let captured_message = apply(&Tree::Atom(&[1]), &message_tree);
            OwnedTree::Pair(
                Box::new(OwnedTree::Pair(
                    Box::new(OwnedTree::Atom(&AGG_SIG_ME_BYTES)),
                    Box::new(OwnedTree::Pair(
                        Box::new(owned(synthetic_public_key)),
                        Box::new(OwnedTree::Pair(
                            Box::new(owned(&captured_message)),
                            Box::new(OwnedTree::Atom(&[])),
                        )),
                    )),
                )),
                Box::new(owned(&conditions)),
            )
        }
        _ => {
            // defun-inline is a macro in the pinned classic compiler. Its
            // assert checks the key relation before selecting the applied tail.
            assert!(hidden_key_matches(
                synthetic_public_key,
                original_public_key,
                delegated_puzzle
            ));
            let conditions = apply(delegated_puzzle, solution);
            owned(&conditions)
        }
    }
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn puzzle<'a>(environment: &Tree<'a>) -> OwnedTree<'a> {
    // Four positional fields, as in the original. Extra/improper tails after
    // solution are ignored; a missing selected field fails.
    match *environment {
        Tree::Pair(synthetic, rest) => match *rest {
            Tree::Pair(original, rest) => match *rest {
                Tree::Pair(delegated, rest) => match *rest {
                    Tree::Pair(solution, _) => finish(synthetic, original, delegated, solution),
                    Tree::Atom(_) => panic!(),
                },
                Tree::Atom(_) => panic!(),
            },
            Tree::Atom(_) => panic!(),
        },
        Tree::Atom(_) => panic!(),
    }
}
