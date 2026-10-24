//! Port of the pinned last_inner helper, not the surrounding last macro/puzzle.
use chialisp_oxide::Tree;

/// Return the final element of a nonempty proper list, preserving its full tree.
/// Empty lists and nonempty atom tails reject, including 0x00 and padded zero.
#[chialisp_oxide::puzzle(environment = "raw")]
pub fn puzzle<'a>(environment: &'a Tree<'a>) -> &'a Tree<'a> {
    match environment {
        Tree::Pair(next, remainder) => match remainder {
            Tree::Atom([]) => next,
            _ => puzzle(remainder),
        },
        Tree::Atom(_) => panic!(),
    }
}
