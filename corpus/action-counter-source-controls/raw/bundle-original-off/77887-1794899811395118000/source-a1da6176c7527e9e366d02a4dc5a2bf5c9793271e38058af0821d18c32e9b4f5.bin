//! Translation of the pinned fixed lambda map. No general Rust closure or
//! arbitrary higher-order function support is asserted by this specialization.
use chialisp_oxide::{OwnedTree, Tree, raw_clvm as raw};

const ONE: [u8; 1] = [1];

fn increment<'a>(value: &Tree<'a>) -> Tree<'static> {
    raw::add(value, &Tree::Atom(&ONE))
}

fn map_increment<'a>(values: &Tree<'a>) -> OwnedTree<'static> {
    match *values {
        Tree::Atom([]) => OwnedTree::Atom(&[]),
        Tree::Pair(first, rest) => {
            let incremented = increment(first);
            match incremented {
                Tree::Atom(bytes) => OwnedTree::Pair(
                    Box::new(OwnedTree::Atom(bytes)),
                    Box::new(map_increment(rest)),
                ),
                Tree::Pair(_, _) => panic!(),
            }
        }
        Tree::Atom(_) => panic!(),
    }
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn puzzle<'a>(environment: &Tree<'a>) -> OwnedTree<'static> {
    let values = raw::first(environment);
    map_increment(&values)
}
