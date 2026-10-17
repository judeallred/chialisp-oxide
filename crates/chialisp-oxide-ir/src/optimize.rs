//! Conservative optimizations over the validated, pure IR.
use crate::{Expr, Program, Type};
use anyhow::Result;

/// Remove redundant tree-pair guards while preserving successful outputs and
/// rejection. This does not preserve VM costs or diagnostic error strings.
///
/// A guard is redundant only when its successful branch must perform a checked
/// first/rest projection of the same tree argument. The projection retains the
/// rejection of atom inputs. No checks are removed from the environment ABI.
/// The source program is unchanged; the returned program is validated again.
pub fn optimize(program: &Program) -> Result<Program> {
    program.validate()?;
    // Successful-evaluation pair facts do not establish that a projection is
    // reached before a call diverges. Preserve all guards in runtime programs.
    if program.uses_runtime() {
        return Ok(program.clone());
    }
    let tree_arguments =
        program
            .parameters
            .iter()
            .enumerate()
            .fold(0_u64, |mask, (index, parameter)| {
                if parameter.ty == Type::Tree {
                    mask | (1_u64 << index)
                } else {
                    mask
                }
            });
    let mut result = program.clone();
    result.body = expression(result.body, tree_arguments).0;
    result.validate()?;
    Ok(result)
}

// A set bit means every successful evaluation of this expression requires that
// tree argument to be a pair. Failure-only expressions get no facts; this is
// deliberately conservative. All facts are about already decoded arguments.
fn boxed(value: Expr, tree_arguments: u64) -> (Box<Expr>, u64) {
    let (value, facts) = expression(value, tree_arguments);
    (Box::new(value), facts)
}

fn guard(mut condition: &Expr) -> Option<(usize, bool)> {
    let mut pair_is_true = true;
    while let Expr::Not { value } = condition {
        pair_is_true = !pair_is_true;
        condition = value;
    }
    if let Expr::TreeIsPair { value } = condition
        && let Expr::Arg { index } = **value
    {
        Some((index, pair_is_true))
    } else {
        None
    }
}

fn projection(value: Expr, tree_arguments: u64, first: bool) -> (Expr, u64) {
    let direct = if let Expr::Arg { index } = value {
        (1_u64 << index) & tree_arguments
    } else {
        0
    };
    let (value, facts) = boxed(value, tree_arguments);
    let expr = if first {
        Expr::TreeFirst { value }
    } else {
        Expr::TreeRest { value }
    };
    (expr, facts | direct)
}

fn expression(expr: Expr, tree_arguments: u64) -> (Expr, u64) {
    match expr {
        Expr::If {
            condition,
            then_value,
            else_value,
        } => {
            let (condition, condition_facts) = boxed(*condition, tree_arguments);
            let (then_value, then_facts) = boxed(*then_value, tree_arguments);
            let (else_value, else_facts) = boxed(*else_value, tree_arguments);
            if let Some((index, pair_is_true)) = guard(&condition) {
                let bit = 1_u64 << index; // Validated argument indices are at most 63.
                if pair_is_true
                    && matches!(*else_value, Expr::Reject { .. })
                    && then_facts & bit != 0
                {
                    return (*then_value, then_facts);
                }
                if !pair_is_true
                    && matches!(*then_value, Expr::Reject { .. })
                    && else_facts & bit != 0
                {
                    return (*else_value, else_facts);
                }
            }
            (
                Expr::If {
                    condition,
                    then_value,
                    else_value,
                },
                condition_facts | (then_facts & else_facts),
            )
        }
        Expr::TreeFirst { value } => projection(*value, tree_arguments, true),
        Expr::TreeRest { value } => projection(*value, tree_arguments, false),
        Expr::TreePair { first, rest } => {
            let (first, a) = boxed(*first, tree_arguments);
            let (rest, b) = boxed(*rest, tree_arguments);
            (Expr::TreePair { first, rest }, a | b)
        }
        Expr::Sequence { first, then_value } => {
            let (first, a) = boxed(*first, tree_arguments);
            let (then_value, b) = boxed(*then_value, tree_arguments);
            (Expr::Sequence { first, then_value }, a | b)
        }
        Expr::Binary {
            kind,
            left,
            right,
            ty,
        } => {
            let (left, a) = boxed(*left, tree_arguments);
            let (right, b) = boxed(*right, tree_arguments);
            (
                Expr::Binary {
                    kind,
                    left,
                    right,
                    ty,
                },
                a | b,
            )
        }
        Expr::Tuple { items } => {
            let mut facts = 0;
            let items = items
                .into_iter()
                .map(|item| {
                    let (item, required) = expression(item, tree_arguments);
                    facts |= required;
                    item
                })
                .collect();
            (Expr::Tuple { items }, facts)
        }
        Expr::Field { value, index } => {
            let (value, facts) = boxed(*value, tree_arguments);
            (Expr::Field { value, index }, facts)
        }
        Expr::TreeAtom { value } => {
            let (value, facts) = boxed(*value, tree_arguments);
            (Expr::TreeAtom { value }, facts)
        }
        Expr::TreeAtomValue { value } => {
            let (value, facts) = boxed(*value, tree_arguments);
            (Expr::TreeAtomValue { value }, facts)
        }
        Expr::TreeIsPair { value } => {
            let (value, facts) = boxed(*value, tree_arguments);
            (Expr::TreeIsPair { value }, facts)
        }
        Expr::Not { value } => {
            let (value, facts) = boxed(*value, tree_arguments);
            (Expr::Not { value }, facts)
        }
        Expr::AtomIsEmpty { value } => {
            let (value, facts) = boxed(*value, tree_arguments);
            (Expr::AtomIsEmpty { value }, facts)
        }
        expr @ (Expr::Atom { .. }
        | Expr::Bytes { .. }
        | Expr::Int { .. }
        | Expr::Bool { .. }
        | Expr::Arg { .. }
        | Expr::Call { .. }
        | Expr::Let { .. }
        | Expr::Local { .. }
        | Expr::Sha256 { .. }
        | Expr::Unit
        | Expr::Reject { .. }) => (expr, 0),
    }
}
