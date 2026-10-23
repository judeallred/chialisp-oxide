//! Raw-tree translation of pinned canonical genesis_by_coin_id TAIL.
//! WIP prototype; not ready for real funds. This is not a complete CAT spend.
use chialisp_oxide::Tree;

fn first<'a>(value: &'a Tree<'a>) -> &'a Tree<'a> {
    match value {
        Tree::Pair(first, _) => first,
        Tree::Atom(_) => panic!(),
    }
}

fn rest<'a>(value: &'a Tree<'a>) -> &'a Tree<'a> {
    match value {
        Tree::Pair(_, rest) => rest,
        Tree::Atom(_) => panic!(),
    }
}

/// Complete pre-curry environment, using only the original's needed paths.
/// Curry GENESIS_ID as one arbitrary atom before using it as a TAIL.
/// No full tuple, proper-tail, amount, integer or 32-byte-ID check is added.
#[chialisp_oxide::puzzle(environment = "raw")]
pub fn puzzle<'a>(environment: &'a Tree<'a>) -> Tree<'static> {
    let delta = first(rest(rest(rest(rest(environment)))));
    let _discarded_delta = delta;

    // Published code compares paths (45, 2). CLVM evaluates these operands
    // right-to-left: project GENESIS_ID first, then the parent from Truths.
    // Only after both projections return does equality require atom operands.
    let genesis = first(environment);
    let truths = first(rest(environment));
    let parent = first(rest(rest(truths)));
    let parent_bytes = match parent {
        Tree::Atom(bytes) => *bytes,
        Tree::Pair(_, _) => panic!(),
    };
    let genesis_bytes = match genesis {
        Tree::Atom(bytes) => *bytes,
        Tree::Pair(_, _) => panic!(),
    };
    assert!(parent_bytes == genesis_bytes);
    Tree::Atom(&[])
}
