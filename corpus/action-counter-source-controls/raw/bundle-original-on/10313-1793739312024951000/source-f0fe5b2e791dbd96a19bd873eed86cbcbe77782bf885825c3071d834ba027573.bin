//! Translation of the pinned Condition Morphing tutorial's outer puzzle.
//!
//! This transforms emitted conditions; it does not create value or authorize
//! the selected inner program. Consensus still checks conservation separately.
use chialisp_oxide::{OwnedTree, Tree, apply, raw_clvm as raw};

const CREATE_COIN_BYTES: [u8; 1] = [51];
const DOUBLE_FACTOR_BYTES: [u8; 1] = [2];

fn owned<'a>(tree: &Tree<'a>) -> OwnedTree<'a> {
    match *tree {
        Tree::Atom(bytes) => OwnedTree::Atom(bytes),
        Tree::Pair(first, rest) => OwnedTree::Pair(Box::new(owned(first)), Box::new(owned(rest))),
    }
}

fn morph_condition<'a>(condition: &Tree<'a>) -> OwnedTree<'a> {
    let opcode = raw::first(condition);
    let is_payment = raw::atom_equal(&opcode, &Tree::Atom(&CREATE_COIN_BYTES));
    match is_payment {
        Tree::Atom([]) => owned(condition),
        _ => {
            let fields = raw::rest(condition);
            let puzzle_hash = raw::first(&fields);
            let amount_and_tail = raw::rest(&fields);
            let amount = raw::first(&amount_and_tail);
            let doubled = raw::mul(&amount, &Tree::Atom(&DOUBLE_FACTOR_BYTES));
            OwnedTree::Pair(
                Box::new(OwnedTree::Atom(&CREATE_COIN_BYTES)),
                Box::new(OwnedTree::Pair(
                    Box::new(owned(&puzzle_hash)),
                    Box::new(OwnedTree::Pair(
                        Box::new(owned(&doubled)),
                        Box::new(OwnedTree::Atom(&[])),
                    )),
                )),
            )
        }
    }
}

fn morph_conditions<'a>(conditions: &Tree<'a>) -> OwnedTree<'a> {
    match *conditions {
        Tree::Pair(condition, rest) => OwnedTree::Pair(
            Box::new(morph_condition(condition)),
            Box::new(morph_conditions(rest)),
        ),
        // The canonical program tests listp, so every atom ends the traversal.
        Tree::Atom(_) => OwnedTree::Atom(&[]),
    }
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn puzzle<'a>(environment: &Tree<'a>) -> OwnedTree<'static> {
    let inner_puzzle = raw::first(environment);
    let inner_solution = raw::rest(environment);
    let conditions = apply(&inner_puzzle, &inner_solution);
    morph_conditions(&conditions)
}
