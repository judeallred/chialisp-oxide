//! Host-only preflight for recursive hashing over the borrowed SDK tree.
//! Bounds are oracle resources, never public puzzle guards or policy rejection.
use anyhow::{Result, ensure};
use bumpalo::Bump;
use chialisp_oxide::Tree;
use clvmr::{Allocator, NodePtr, allocator::SExp};

const MAX_DEPTH: usize = 256;
const MAX_NODES: usize = 16_384;
const MAX_ATOM_BYTES: usize = 1_048_576;

fn preflight(allocator: &Allocator, root: NodePtr) -> Result<()> {
    let mut pending = vec![(root, 0usize)];
    let mut nodes = 0usize;
    let mut atom_bytes = 0usize;
    while let Some((node, depth)) = pending.pop() {
        nodes = nodes
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("native hash node count overflow"))?;
        ensure!(
            depth <= MAX_DEPTH,
            "native hash input depth exceeds {MAX_DEPTH}"
        );
        ensure!(
            nodes <= MAX_NODES,
            "native hash input nodes exceed {MAX_NODES}"
        );
        match allocator.sexp(node) {
            SExp::Pair(first, rest) => pending.extend([(rest, depth + 1), (first, depth + 1)]),
            SExp::Atom => {
                atom_bytes = atom_bytes
                    .checked_add(allocator.atom(node).as_ref().len())
                    .ok_or_else(|| anyhow::anyhow!("native hash atom byte count overflow"))?;
                ensure!(
                    atom_bytes <= MAX_ATOM_BYTES,
                    "native hash input atom bytes exceed {MAX_ATOM_BYTES}"
                );
            }
        }
    }
    Ok(())
}

/// Check expanded occurrences and both branches before any bump allocation.
/// The returned tree borrows only from the arena; atom bytes are never decoded
/// numerically. Keep failures from this function outside any policy panic catch.
pub(super) fn decode<'a>(
    arena: &'a Bump,
    allocator: &Allocator,
    root: NodePtr,
) -> Result<&'a Tree<'a>> {
    preflight(allocator, root)?;
    super::native_tree::decode(arena, allocator, root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_tree_depth_counts_left_and_right_before_allocating() {
        for left in [false, true] {
            let mut allocator = Allocator::new();
            let zero = allocator.new_atom(&[0]).unwrap();
            let mut root = zero;
            for _ in 0..MAX_DEPTH {
                root = if left {
                    allocator.new_pair(root, zero).unwrap()
                } else {
                    allocator.new_pair(zero, root).unwrap()
                };
            }
            let arena = Bump::new();
            decode(&arena, &allocator, root).unwrap();
            root = allocator.new_pair(root, zero).unwrap();
            let fresh = Bump::new();
            let before = fresh.allocated_bytes();
            let error = decode(&fresh, &allocator, root).unwrap_err().to_string();
            assert!(error.contains("input depth exceeds"), "{error}");
            assert_eq!(fresh.allocated_bytes(), before);
        }
    }

    #[test]
    fn shared_subtrees_count_every_expanded_occurrence() {
        let mut allocator = Allocator::new();
        let mut root = allocator.new_atom(&[0]).unwrap();
        for _ in 0..13 {
            root = allocator.new_pair(root, root).unwrap();
        }
        preflight(&allocator, root).unwrap();
        root = allocator.new_pair(root, root).unwrap();
        let fresh = Bump::new();
        let before = fresh.allocated_bytes();
        let error = decode(&fresh, &allocator, root).unwrap_err().to_string();
        assert!(error.contains("input nodes exceed"), "{error}");
        assert_eq!(fresh.allocated_bytes(), before);
    }

    #[test]
    fn atom_byte_budget_counts_ignored_and_repeated_payloads() {
        let mut allocator = Allocator::new();
        let large = allocator.new_atom(&vec![0; MAX_ATOM_BYTES]).unwrap();
        preflight(&allocator, large).unwrap();
        let root = allocator.new_pair(large, large).unwrap();
        let fresh = Bump::new();
        let before = fresh.allocated_bytes();
        let error = decode(&fresh, &allocator, root).unwrap_err().to_string();
        assert!(error.contains("input atom bytes exceed"), "{error}");
        assert_eq!(fresh.allocated_bytes(), before);
    }
}
