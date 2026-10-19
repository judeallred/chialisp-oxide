//! Conservative optimizations over the validated, pure IR.
use crate::{Expr, Program, Type};
use anyhow::Result;
mod budget;
mod constants;
mod facts;
mod inline;
mod intervals;

pub(crate) struct Plan {
    pub program: Program,
    pub before_inlining: Option<Program>,
}

pub(crate) fn plan(program: &Program) -> Result<Plan> {
    program.validate()?;
    // Initial pair admission deliberately has no frontend rewrite semantics.
    // Keep strict runtime mode; Rue LIR/codegen and runtime frame forwarding
    // are independently controlled by the compiler's optimization setting.
    if program.uses_pairs() {
        return Ok(Plan {
            program: pair_copy(program)?,
            before_inlining: None,
        });
    }
    let previous = before_inlining(program)?;
    Ok(if let Some(inlined) = inline::optimize(&previous)? {
        Plan {
            program: inlined,
            before_inlining: Some(previous),
        }
    } else {
        Plan {
            program: previous,
            before_inlining: None,
        }
    })
}

#[derive(Debug)]
struct PairCopyLimit;
impl std::fmt::Display for PairCopyLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("typed pair optimization copy budget exceeded (4 MiB)")
    }
}
impl std::error::Error for PairCopyLimit {}

/// Whole validation bounds all expression/type nodes and depths. Charge every
/// owned string before cloning the unchanged pair-capable program as an
/// optimization result; bypassing unsupported rewrites must not bypass their
/// literal-copy budget. This scan allocates only bounded borrowed descriptors.
fn pair_copy(program: &Program) -> Result<Program> {
    let mut bytes = 0usize;
    let mut charge = |text: &str| -> Result<()> {
        bytes = bytes.saturating_add(text.len());
        anyhow::ensure!(bytes <= 4 * 1024 * 1024, PairCopyLimit);
        Ok(())
    };
    charge(&program.name)?;
    for parameter in &program.parameters {
        charge(&parameter.name)?;
    }
    let mut pending = vec![&program.body];
    for function in &program.functions {
        charge(&function.name)?;
        for parameter in &function.parameters {
            charge(&parameter.name)?;
        }
        pending.push(&function.body);
    }
    while let Some(expression) = pending.pop() {
        match expression {
            Expr::Atom { hex } | Expr::Bytes { hex } => charge(hex)?,
            Expr::Int { value, .. } => charge(value)?,
            Expr::Reject { message } => charge(message)?,
            _ => (),
        }
        crate::validate::children(expression, &mut pending);
    }
    Ok(program.clone())
}

fn refine(program: &Program) -> Result<Option<Program>> {
    // These optional visitors have larger native frames than the original
    // lowering. Scan iteratively first; deep valid IR follows the prior route
    // and its existing resource diagnostics instead of exhausting our stack.
    if !budget::allows_recursive_refinement(program) {
        return Ok(None);
    }
    let reduced = constants::optimize(program)?;
    if !budget::allows_recursive_refinement(&reduced) {
        return Ok(None);
    }
    let refined = facts::optimize(&reduced)?;
    if !budget::allows_recursive_refinement(&refined) {
        return Ok(None);
    }
    let result = constants::optimize(&refined)?;
    let result = intervals::optimize(&result)?.unwrap_or(result);
    Ok(budget::allows_recursive_refinement(&result).then_some(result))
}

/// Simplify checked constants, forward execution-success facts and proven-total
/// discarded arithmetic, then remove eligible legacy pair guards and inline
/// bounded closed helpers. Preserve
/// values, rejection and strict effects; VM costs, resource thresholds and
/// diagnostic strings are not preserved.
///
/// A guard is redundant only when its successful branch must perform a checked
/// first/rest projection of the same tree argument. The projection retains the
/// rejection of atom inputs. No checks are removed from the environment ABI.
/// The source program is unchanged; the returned program is validated again.
pub fn optimize(program: &Program) -> Result<Program> {
    let result = plan(program)?.program;
    // A standalone Program carries its evaluation convention only through
    // runtime nodes/the function table. Serialization must not erase strict
    // ordering when refinement removes the last such node. A total result has
    // no failure/divergence ordering to preserve after the unchanged complete
    // ABI decoder. Private compilation pins the original mode separately and
    // can still use its fully reduced plan.
    if program.uses_runtime() && !result.uses_runtime() && !total_result(&result.body) {
        return Ok(program.clone());
    }
    Ok(result)
}

/// A closed whitelist for evaluation-mode removal, after full validation.
/// Children must also be total: projecting a tuple does not discard a failing
/// sibling. Integer comparisons are total on decoded fixed-width values;
/// checked arithmetic, tree projections, casts and effects stay outside this
/// whitelist even when some of their inputs might make them safe.
fn total_result(body: &Expr) -> bool {
    // Validation bounds the entire result to 20,000 nodes before this borrowed
    // iterative scan. No recursive visitor or copied literal payloads are used.
    let mut pending = vec![body];
    while let Some(expr) = pending.pop() {
        match expr {
            Expr::Arg { .. }
            | Expr::Unit
            | Expr::Bool { .. }
            | Expr::Int { .. }
            | Expr::Atom { .. }
            | Expr::Bytes { .. } => (),
            Expr::Tuple { items } => pending.extend(items),
            Expr::TreePair { first, rest } => pending.extend([first.as_ref(), rest.as_ref()]),
            Expr::If {
                condition,
                then_value,
                else_value,
            } => pending.extend([condition.as_ref(), then_value.as_ref(), else_value.as_ref()]),
            Expr::Not { value }
            | Expr::AtomIsEmpty { value }
            | Expr::TreeIsPair { value }
            | Expr::TreeAtom { value }
            | Expr::Field { value, .. } => pending.push(value),
            Expr::Binary {
                kind:
                    crate::BinaryOp::Eq
                    | crate::BinaryOp::Ne
                    | crate::BinaryOp::Lt
                    | crate::BinaryOp::Le
                    | crate::BinaryOp::Gt
                    | crate::BinaryOp::Ge,
                left,
                right,
                ..
            } => {
                pending.extend([left.as_ref(), right.as_ref()]);
            }
            _ => return false,
        }
    }
    true
}

fn before_inlining(program: &Program) -> Result<Program> {
    program.validate()?;
    let runtime_required = program.uses_runtime();
    // Refinement can make match-discriminant bindings unused. Recheck their
    // totality before deleting them; an unused call/hash/projection still runs.
    let refined = refine(program)?;
    let program = refined.as_ref().unwrap_or(program);
    // Successful-evaluation pair facts do not establish that a projection is
    // reached before a call diverges. Preserve all guards in runtime programs.
    if runtime_required || program.uses_runtime() {
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
        | Expr::Pair { .. }
        | Expr::PairFirst { .. }
        | Expr::PairRest { .. }
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
