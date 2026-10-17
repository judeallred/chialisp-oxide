//! Host-only codec for owned SDK trees. Bounds are oracle limits, not policy.
//!
//! Check the entire input before allocating any recursive `Box` values: dropping
//! ignored heads and tails also traverses them. Prefixing needs a second depth
//! check because its result can be deeper than either input. These errors must
//! stay outside the adapter that turns a policy panic into a rejection.
use anyhow::{Result, anyhow, ensure};
use bumpalo::Bump;
use chialisp_oxide::OwnedTree;
use clvmr::{Allocator, NodePtr, allocator::SExp};

const MAX_DEPTH: usize = 128;
const MAX_NODES: usize = 16_384;
const MAX_ATOM_BYTES: usize = 1_048_576;

#[derive(Debug, Default)]
pub(super) struct Stats {
    depth: usize,
    nodes: usize,
    atom_bytes: usize,
}

/// Count expanded tree occurrences, including shared allocator nodes each time.
pub(super) fn preflight(a: &Allocator, root: NodePtr) -> Result<Stats> {
    let mut stats = Stats::default();
    let mut pending = vec![(root, 0)];
    while let Some((node, depth)) = pending.pop() {
        stats.nodes += 1;
        stats.depth = stats.depth.max(depth);
        ensure!(
            depth <= MAX_DEPTH,
            "native owned-tree depth exceeds {MAX_DEPTH}"
        );
        ensure!(
            stats.nodes <= MAX_NODES,
            "native owned-tree nodes exceed {MAX_NODES}"
        );
        match a.sexp(node) {
            SExp::Pair(first, rest) => {
                pending.extend([(rest, depth + 1), (first, depth + 1)]);
            }
            SExp::Atom => {
                stats.atom_bytes = stats.atom_bytes.saturating_add(a.atom(node).as_ref().len());
                ensure!(
                    stats.atom_bytes <= MAX_ATOM_BYTES,
                    "native owned-tree atom bytes exceed {MAX_ATOM_BYTES}"
                );
            }
        }
    }
    Ok(stats)
}

/// A proper left spine of length n places the complete final tail n levels down.
/// An improper left tree fails before constructing a result; its whole input
/// still needs the ordinary preflight because native unwinding drops that tree.
pub(super) fn preflight_prefix(a: &Allocator, list: NodePtr, tail: NodePtr) -> Result<()> {
    preflight(a, list)?;
    let tail_stats = preflight(a, tail)?;
    let mut rest = list;
    let mut pairs = 0;
    while let SExp::Pair(_, next) = a.sexp(rest) {
        pairs += 1;
        rest = next;
    }
    if a.atom(rest).as_ref().is_empty() {
        ensure!(
            pairs + tail_stats.depth <= MAX_DEPTH,
            "native prefix output depth exceeds {MAX_DEPTH}"
        );
    }
    Ok(())
}

/// Sorting moves complete entries between the two right spines. Their combined
/// length bounds recursion and, with the deepest untouched entry, every possible
/// output depth. This is a conservative host bound, never a compiled assertion.
pub(super) fn preflight_sort(a: &Allocator, sorted: NodePtr, insertions: NodePtr) -> Result<()> {
    preflight(a, sorted)?;
    preflight(a, insertions)?;
    let mut pairs = 0;
    let mut entry_depth = 0;
    for mut rest in [sorted, insertions] {
        while let SExp::Pair(first, next) = a.sexp(rest) {
            pairs += 1;
            entry_depth = entry_depth.max(preflight(a, first)?.depth);
            ensure!(
                pairs + entry_depth <= MAX_DEPTH,
                "native insertion-sort potential output depth exceeds {MAX_DEPTH}"
            );
            rest = next;
        }
    }
    Ok(())
}

pub(super) fn decode<'a>(arena: &'a Bump, a: &Allocator, root: NodePtr) -> Result<OwnedTree<'a>> {
    let stats = preflight(a, root)?;
    let mut pending = vec![Some(root)];
    let mut values = Vec::new();
    values.try_reserve(stats.nodes)?;
    while let Some(frame) = pending.pop() {
        match frame {
            Some(node) => match a.sexp(node) {
                SExp::Atom => {
                    let bytes = arena
                        .try_alloc_slice_copy(a.atom(node).as_ref())
                        .map_err(|_| anyhow!("native owned-tree atom allocation failed"))?;
                    values.push(OwnedTree::Atom(bytes));
                }
                SExp::Pair(first, rest) => pending.extend([None, Some(rest), Some(first)]),
            },
            None => {
                let rest = values.pop().expect("visited rest");
                let first = values.pop().expect("visited first");
                values.push(OwnedTree::Pair(Box::new(first), Box::new(rest)));
            }
        }
    }
    ensure!(values.len() == 1, "invalid owned-tree traversal");
    Ok(values.pop().expect("one complete tree"))
}

/// Consume the output iteratively, including after a resource/allocation error.
/// This avoids recursive destruction of a result that violates the host bound.
/// Policy recursion itself still needs its input/result preflight before entry.
pub(super) fn encode(a: &mut Allocator, root: OwnedTree<'_>) -> Result<NodePtr> {
    let mut pending = vec![Some((root, 0))];
    let mut values = vec![];
    let mut stats = Stats::default();
    let mut failure = None;
    while let Some(frame) = pending.pop() {
        match frame {
            Some((value, depth)) => {
                stats.nodes = stats.nodes.saturating_add(1);
                if failure.is_none() && (depth > MAX_DEPTH || stats.nodes > MAX_NODES) {
                    failure = Some(anyhow!(
                        "native owned-tree output depth or node limit exceeded"
                    ));
                }
                match value {
                    OwnedTree::Atom(bytes) => {
                        stats.atom_bytes = stats.atom_bytes.saturating_add(bytes.len());
                        if failure.is_none() && stats.atom_bytes > MAX_ATOM_BYTES {
                            failure =
                                Some(anyhow!("native owned-tree output atom byte limit exceeded"));
                        }
                        if failure.is_none() {
                            match a.new_atom(bytes) {
                                Ok(node) => values.push(node),
                                Err(error) => failure = Some(error.into()),
                            }
                        }
                    }
                    OwnedTree::Pair(first, rest) => {
                        pending.extend([None, Some((*rest, depth + 1)), Some((*first, depth + 1))]);
                    }
                }
            }
            None if failure.is_none() => {
                let rest = values.pop().expect("visited rest");
                let first = values.pop().expect("visited first");
                match a.new_pair(first, rest) {
                    Ok(node) => values.push(node),
                    Err(error) => failure = Some(error.into()),
                }
            }
            None => (),
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    ensure!(values.len() == 1, "invalid owned-tree traversal");
    Ok(values[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use clvmr::serde::node_to_bytes;

    fn spine(a: &mut Allocator, count: usize, left: bool) -> NodePtr {
        let mut tree = NodePtr::NIL;
        for _ in 0..count {
            tree = if left {
                a.new_pair(tree, NodePtr::NIL).unwrap()
            } else {
                a.new_pair(NodePtr::NIL, tree).unwrap()
            };
        }
        tree
    }

    #[test]
    fn all_branches_are_bounded_before_allocating_owned_values() {
        let mut a = Allocator::new();
        let arena = Bump::new();
        for left in [false, true] {
            let deep = spine(&mut a, 10_000, left);
            assert!(
                preflight(&a, deep)
                    .unwrap_err()
                    .to_string()
                    .contains("depth")
            );
            assert!(decode(&arena, &a, deep).is_err());
            assert_eq!(arena.allocated_bytes(), 0);
        }
        // A small DAG can expand to a large owned tree even at shallow depth.
        let mut wide = NodePtr::NIL;
        for _ in 0..15 {
            wide = a.new_pair(wide, wide).unwrap();
        }
        assert!(
            preflight(&a, wide)
                .unwrap_err()
                .to_string()
                .contains("nodes")
        );
        assert!(decode(&arena, &a, wide).is_err());
        assert_eq!(arena.allocated_bytes(), 0);
        let huge = a.new_atom(&vec![0; MAX_ATOM_BYTES + 1]).unwrap();
        assert!(
            decode(&arena, &a, huge)
                .err()
                .unwrap()
                .to_string()
                .contains("atom bytes")
        );
        assert_eq!(arena.allocated_bytes(), 0);
    }

    #[test]
    fn prefix_checks_result_depth_before_constructing_inputs() {
        let mut a = Allocator::new();
        let list = spine(&mut a, 64, false);
        let tail = spine(&mut a, 64, true);
        assert!(preflight_prefix(&a, list, tail).is_ok());
        let deeper_tail = a.new_pair(tail, NodePtr::NIL).unwrap();
        assert!(preflight(&a, deeper_tail).is_ok());
        assert!(
            preflight_prefix(&a, list, deeper_tail)
                .unwrap_err()
                .to_string()
                .contains("output depth")
        );
    }

    #[test]
    fn sorting_bounds_both_spines_and_untouched_entry_depth_before_boxes() {
        let mut a = Allocator::new();
        let entry = a.new_pair(NodePtr::NIL, NodePtr::NIL).unwrap();
        let mut sorted = NodePtr::NIL;
        let mut insertions = NodePtr::NIL;
        for _ in 0..64 {
            sorted = a.new_pair(entry, sorted).unwrap();
        }
        for _ in 0..63 {
            insertions = a.new_pair(entry, insertions).unwrap();
        }
        assert!(preflight_sort(&a, sorted, insertions).is_ok());
        let extra = a.new_pair(entry, insertions).unwrap();
        assert!(preflight(&a, sorted).is_ok() && preflight(&a, extra).is_ok());
        assert!(
            preflight_sort(&a, sorted, extra)
                .unwrap_err()
                .to_string()
                .contains("potential output depth")
        );
        // Entry payloads remain arbitrary trees; account for their whole depth.
        let deep_entry = spine(&mut a, 65, true);
        let deep_insertions = a.new_pair(deep_entry, NodePtr::NIL).unwrap();
        assert!(preflight(&a, deep_insertions).is_ok());
        assert!(preflight_sort(&a, sorted, deep_insertions).is_err());
        // A base case can return an improper sorted tail unchanged. Shape is a
        // policy concern: this host check must not silently reject small atoms.
        let padded = a.new_atom(&[0, 0]).unwrap();
        assert!(preflight_sort(&a, padded, NodePtr::NIL).is_ok());
    }

    #[test]
    fn atoms_improper_pairs_and_boundary_depth_round_trip_exactly() {
        let mut a = Allocator::new();
        let padded = a.new_atom(&[0, 0, 1]).unwrap();
        let deep = spine(&mut a, MAX_DEPTH - 1, true);
        let input = a.new_pair(deep, padded).unwrap();
        let original = node_to_bytes(&a, input).unwrap();
        let arena = Bump::new();
        let owned = decode(&arena, &a, input).unwrap();
        let mut b = Allocator::new();
        let encoded = encode(&mut b, owned).unwrap();
        assert_eq!(node_to_bytes(&b, encoded).unwrap(), original);
    }

    #[test]
    fn an_oversized_output_is_an_error_and_is_consumed_without_recursive_drop() {
        let mut value = OwnedTree::Atom(&[]);
        for _ in 0..10_000 {
            value = OwnedTree::Pair(Box::new(value), Box::new(OwnedTree::Atom(&[])));
        }
        let error = encode(&mut Allocator::new(), value).unwrap_err();
        assert!(error.to_string().contains("output depth or node limit"));
    }
}
