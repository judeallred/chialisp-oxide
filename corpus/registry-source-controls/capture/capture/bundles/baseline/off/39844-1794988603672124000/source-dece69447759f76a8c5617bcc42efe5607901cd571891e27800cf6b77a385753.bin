//! Exact raw-tree port of Chia's canonical p2_conditions program factory.
use chialisp_oxide::{Tree, quote};

/// Build a quoted program from the first environment element.
/// The rest may be any tree; the payload need not be valid spend conditions.
#[chialisp_oxide::puzzle(environment = "raw")]
pub fn puzzle<'a>(environment: &'a Tree<'a>) -> Tree<'a> {
    match environment {
        Tree::Pair(conditions, _) => quote(conditions),
        Tree::Atom(_) => panic!(), // The original's first projection rejects atoms.
    }
}
