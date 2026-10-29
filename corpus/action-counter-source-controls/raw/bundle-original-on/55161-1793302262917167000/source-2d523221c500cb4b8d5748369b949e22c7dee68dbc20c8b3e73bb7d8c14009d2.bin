//! Original raw-tree recursion example, not a coin or canonical puzzle port.
use chialisp_oxide::Tree;

/// Follow pair tails until an atom, preserving that atom's complete bytes.
/// A proper list ends at nil; this does not return its last element.
#[chialisp_oxide::puzzle(environment = "raw")]
pub fn puzzle<'a>(environment: &'a Tree<'a>) -> &'a Tree<'a> {
    match environment {
        Tree::Atom(_) => environment,
        Tree::Pair(_, rest) => puzzle(rest),
    }
}
