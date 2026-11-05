//! Translation of one closed named-function map, not Rust closure admission.
use chialisp_oxide::{OwnedTree, Tree, raw_clvm as raw};

const ONE: [u8; 1] = [1];

fn add_one<'a>(value: &Tree<'a>) -> Tree<'static> {
    raw::add(value, &Tree::Atom(&ONE))
}

fn map_add_one<'a>(values: &Tree<'a>) -> OwnedTree<'static> {
    match *values {
        Tree::Atom([]) => OwnedTree::Atom(&[]),
        Tree::Pair(first, rest) => {
            let incremented = add_one(first);
            match incremented {
                Tree::Atom(bytes) => OwnedTree::Pair(
                    Box::new(OwnedTree::Atom(bytes)),
                    Box::new(map_add_one(rest)),
                ),
                Tree::Pair(_, _) => panic!(),
            }
        }
        // Original `if L` treats every non-nil atom as true. Its subsequent
        // first/rest projection fails; it does not truncate like listp.
        Tree::Atom(_) => panic!(),
    }
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn puzzle<'a>(environment: &Tree<'a>) -> OwnedTree<'static> {
    let values = raw::first(environment);
    map_add_one(&values)
}
