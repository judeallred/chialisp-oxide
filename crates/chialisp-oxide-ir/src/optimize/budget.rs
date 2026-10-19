//! Optional recursive refinements have a smaller host-stack budget than IR.
//!
//! The caller validates the complete Program first. Exceeding this preflight
//! skips optional passes; it does not reject an otherwise valid input or relax
//! the existing lowering/code-generation limits.
use crate::{Expr, Program};

const MAX_DEPTH: usize = 64;
const MAX_FUNCTIONS: usize = 64;
const MAX_NODES: usize = 20_000;

/// Inspect borrowed expressions only, including unused function bodies.
/// The root expression of each independent body has depth one.
pub(super) fn allows_recursive_refinement(program: &Program) -> bool {
    if program.functions.len() > MAX_FUNCTIONS {
        return false;
    }
    let mut pending = Vec::with_capacity(1 + program.functions.len());
    pending.push((&program.body, 1_usize));
    for function in &program.functions {
        pending.push((&function.body, 1));
    }
    let mut visited = 0_usize;
    while let Some((expression, depth)) = pending.pop() {
        if depth > MAX_DEPTH || visited >= MAX_NODES {
            return false;
        }
        visited += 1;
        let children = match expression {
            Expr::Tuple { items }
            | Expr::Sha256 { parts: items }
            | Expr::Call {
                arguments: items, ..
            } => items.len(),
            Expr::Let { .. }
            | Expr::Sequence { .. }
            | Expr::TreePair { .. }
            | Expr::Pair { .. }
            | Expr::Binary { .. } => 2,
            Expr::If { .. } => 3,
            Expr::AtomIsEmpty { .. }
            | Expr::TreeAtom { .. }
            | Expr::TreeAtomValue { .. }
            | Expr::TreeIsPair { .. }
            | Expr::TreeFirst { .. }
            | Expr::PairFirst { .. }
            | Expr::PairRest { .. }
            | Expr::TreeRest { .. }
            | Expr::Not { .. }
            | Expr::Field { .. } => 1,
            Expr::Atom { .. }
            | Expr::Unit
            | Expr::Bool { .. }
            | Expr::Int { .. }
            | Expr::Bytes { .. }
            | Expr::Arg { .. }
            | Expr::Local { .. }
            | Expr::Reject { .. } => 0,
        };
        // Bound future visits and allocation before extending the worklist.
        // Arithmetic is bounded independently of the caller's validation.
        if children > MAX_NODES.saturating_sub(visited + pending.len())
            || (children != 0 && depth == MAX_DEPTH)
        {
            return false;
        }
        let depth = depth + 1;
        match expression {
            Expr::Tuple { items }
            | Expr::Sha256 { parts: items }
            | Expr::Call {
                arguments: items, ..
            } => pending.extend(items.iter().map(|item| (item, depth))),
            Expr::Let { value, body, .. }
            | Expr::Sequence {
                first: value,
                then_value: body,
            } => pending.extend([(value.as_ref(), depth), (body.as_ref(), depth)]),
            Expr::TreePair { first, rest } | Expr::Pair { first, rest } => {
                pending.extend([(first.as_ref(), depth), (rest.as_ref(), depth)]);
            }
            Expr::Binary { left, right, .. } => {
                pending.extend([(left.as_ref(), depth), (right.as_ref(), depth)]);
            }
            Expr::If {
                condition,
                then_value,
                else_value,
            } => pending.extend([
                (condition.as_ref(), depth),
                (then_value.as_ref(), depth),
                (else_value.as_ref(), depth),
            ]),
            Expr::AtomIsEmpty { value }
            | Expr::TreeAtom { value }
            | Expr::TreeAtomValue { value }
            | Expr::TreeIsPair { value }
            | Expr::TreeFirst { value }
            | Expr::PairFirst { value }
            | Expr::PairRest { value }
            | Expr::TreeRest { value }
            | Expr::Not { value }
            | Expr::Field { value, .. } => pending.push((value.as_ref(), depth)),
            Expr::Atom { .. }
            | Expr::Unit
            | Expr::Bool { .. }
            | Expr::Int { .. }
            | Expr::Bytes { .. }
            | Expr::Arg { .. }
            | Expr::Local { .. }
            | Expr::Reject { .. } => {}
        }
    }
    true
}
