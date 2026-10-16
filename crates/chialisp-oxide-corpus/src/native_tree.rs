//! Host-only codec for the actual borrowed SDK tree. No numeric interpretation.
//! Both traversals use explicit stacks so tree depth does not consume call stack.
use anyhow::{Result, ensure};
use bumpalo::Bump;
use chialisp_oxide::Tree;
use clvmr::{Allocator, NodePtr, allocator::SExp};

pub fn decode<'a>(arena: &'a Bump, a: &Allocator, root: NodePtr) -> Result<&'a Tree<'a>> {
    let mut pending = vec![Some(root)];
    let mut values: Vec<&'a Tree<'a>> = vec![];
    while let Some(frame) = pending.pop() {
        match frame {
            Some(node) => match a.sexp(node) {
                SExp::Atom => {
                    let bytes = arena.alloc_slice_copy(a.atom(node).as_ref());
                    values.push(arena.alloc(Tree::Atom(bytes)));
                }
                SExp::Pair(first, rest) => {
                    pending.extend([None, Some(rest), Some(first)]);
                }
            },
            None => {
                let rest = values.pop().expect("visited rest");
                let first = values.pop().expect("visited first");
                values.push(arena.alloc(Tree::Pair(first, rest)));
            }
        }
    }
    ensure!(values.len() == 1, "invalid tree traversal");
    Ok(values[0])
}

pub fn encode(a: &mut Allocator, root: &Tree<'_>) -> Result<NodePtr> {
    let mut pending = vec![Some(root)];
    let mut values = vec![];
    while let Some(frame) = pending.pop() {
        match frame {
            Some(Tree::Atom(bytes)) => values.push(a.new_atom(bytes)?),
            Some(Tree::Pair(first, rest)) => {
                pending.extend([None, Some(*rest), Some(*first)]);
            }
            None => {
                let rest = values.pop().expect("visited rest");
                let first = values.pop().expect("visited first");
                values.push(a.new_pair(first, rest)?);
            }
        }
    }
    ensure!(values.len() == 1, "invalid tree traversal");
    Ok(values[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use clvmr::serde::node_to_bytes;

    #[test]
    fn exact_atom_contents_and_improper_structure_round_trip_without_recursion() {
        let mut a = Allocator::new();
        let mut root = a.new_atom(&[0, 0, 1]).unwrap();
        // Deeper than the compiler expression-depth budget: runtime data is not IR.
        for i in 0..10_000_u32 {
            let atom = a.new_atom(&i.to_be_bytes()).unwrap();
            root = a.new_pair(atom, root).unwrap();
        }
        let original = node_to_bytes(&a, root).unwrap();
        let arena = Bump::new();
        let tree = decode(&arena, &a, root).unwrap();
        let mut b = Allocator::new();
        let encoded = encode(&mut b, tree).unwrap();
        assert_eq!(node_to_bytes(&b, encoded).unwrap(), original);
    }
}
