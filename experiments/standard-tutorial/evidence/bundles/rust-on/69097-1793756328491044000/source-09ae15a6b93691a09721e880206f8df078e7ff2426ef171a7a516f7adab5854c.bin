//! Exact raw argument contract of the pinned arbitrary-conditions tutorial.
//! Returns the selected tree unchanged; it establishes no authorization.
use chialisp_oxide::Tree;

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn puzzle<'a>(environment: &'a Tree<'a>) -> &'a Tree<'a> {
    match environment {
        Tree::Pair(conditions, _) => conditions,
        Tree::Atom(_) => panic!(),
    }
}
