//! Original teaching factory: the standard curried program of a module and one parameter.
//!
//! The factory returns `(a (q . module) (c (q . parameter) 1))` as an owned
//! tree through the SDK's `curry` constructor. The corpus compares the produced
//! program with an independent standard-form construction and applies it in a
//! second VM execution; producing a program never counts as a complete spend.
use chialisp_oxide::{OwnedTree, Tree, curry::curry};

/// Copy a borrowed tree into owned pair nodes, preserving every atom byte.
fn owned<'a>(tree: &Tree<'a>) -> OwnedTree<'a> {
    match *tree {
        Tree::Atom(bytes) => OwnedTree::Atom(bytes),
        Tree::Pair(first, rest) => OwnedTree::Pair(Box::new(owned(first)), Box::new(owned(rest))),
    }
}

#[chialisp_oxide::puzzle]
pub fn puzzle<'a>(module: &Tree<'a>, parameter: &Tree<'a>) -> OwnedTree<'a> {
    curry(owned(module), owned(parameter))
}
