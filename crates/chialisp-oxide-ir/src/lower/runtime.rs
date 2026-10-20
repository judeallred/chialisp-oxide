//! Closed runtime functions and strict, once-only evaluation.
//!
//! Every frame is `(compiler_function_table . slots)`. Eligible optimized unary calls
//! store their sole argument directly in the slots tail; other calls use a
//! proper argument list. A binding applies a quoted continuation with one
//! new slot; slot identities, rather than cached paths, survive that extension.
//! Calls create fresh slots containing only their arguments. No input tree can
//! select a callee or supply executable code through this calling convention.
use super::{Lower, limits::Limit};
use crate::{Expr, Parameter, Program, Type};
use anyhow::{Result, anyhow, ensure};
use num_bigint::BigUint;
use rue_lir::{Lir, LirId, first_path, rest_path};
use std::{collections::BTreeMap, error::Error, fmt};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Binding {
    Argument(usize, usize),
    Local(usize, usize),
    Temporary(usize),
}

#[derive(Clone, Copy)]
enum Value {
    Slot(Binding),
    Literal(LirId),
    Projection(usize),
}

// A place describes only a read of already captured immutable data. Its root
// is a physical binding identity, never a lexical name or cached VM path.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Place {
    Slot(Binding),
    Projection(usize),
}

#[derive(Clone, Copy)]
struct Projection {
    parent: Place,
    first: bool,
    depth: usize,
    reads: usize,
}

#[derive(Debug)]
pub(super) struct ProjectionBudget;
impl fmt::Display for ProjectionBudget {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("optional guarded projection analysis budget exceeded")
    }
}
impl Error for ProjectionBudget {}

const MAX_PROJECTION_WORK: usize = 50_000;

#[derive(Debug)]
pub(super) struct AtomBudget;
impl fmt::Display for AtomBudget {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("optional atom fact analysis budget exceeded")
    }
}
impl Error for AtomBudget {}

// Complete borrowed preflight, including unused function bodies, before the
// optional analysis. Deeper valid input retains the preceding lowering route.
fn projection_preflight(program: &Program) -> bool {
    let mut pending = Vec::with_capacity(1 + program.functions.len());
    pending.push((&program.body, 1usize));
    pending.extend(program.functions.iter().map(|function| (&function.body, 1)));
    let mut visited = 0usize;
    while let Some((expression, depth)) = pending.pop() {
        if depth > 64 || visited >= 20_000 {
            return false;
        }
        visited += 1;
        let mut child = |expression| {
            if depth >= 64 || visited + pending.len() >= 20_000 {
                false
            } else {
                pending.push((expression, depth + 1));
                true
            }
        };
        let bounded = match expression {
            Expr::Tuple { items }
            | Expr::Sha256 { parts: items }
            | Expr::Call {
                arguments: items, ..
            } => items.iter().all(&mut child),
            Expr::Binary { left, right, .. }
            | Expr::Apply {
                program: left,
                environment: right,
            } => child(left.as_ref()) && child(right.as_ref()),
            Expr::TreePair { first, rest }
            | Expr::Pair { first, rest }
            | Expr::Sequence {
                first,
                then_value: rest,
            } => child(first.as_ref()) && child(rest.as_ref()),
            Expr::Let { value, body, .. } => child(value.as_ref()) && child(body.as_ref()),
            Expr::If {
                condition,
                then_value,
                else_value,
            } => {
                child(condition.as_ref())
                    && child(then_value.as_ref())
                    && child(else_value.as_ref())
            }
            Expr::AtomIsEmpty { value }
            | Expr::Not { value }
            | Expr::Field { value, .. }
            | Expr::TreeAtom { value }
            | Expr::TreeAtomValue { value }
            | Expr::TreeIsPair { value }
            | Expr::TreeFirst { value }
            | Expr::TreeRest { value }
            | Expr::PairFirst { value }
            | Expr::PairRest { value } => child(value.as_ref()),
            Expr::Arg { .. }
            | Expr::Local { .. }
            | Expr::Atom { .. }
            | Expr::Bytes { .. }
            | Expr::Unit
            | Expr::Bool { .. }
            | Expr::Int { .. }
            | Expr::Reject { .. } => true,
        };
        if !bounded {
            return false;
        }
    }
    true
}

// A v5 program may contain an ordinary Tree helper whose own body never
// handles typed pairs. Keep this first lane closed to leaf/self-recursive
// helpers: a call to another table entry is not borrowed admission evidence.
// The whole program is validated and preflighted before this scan. Descriptor
// visits/copies share the existing optional projection budget across bodies.
fn helper_charge(work: &mut usize, amount: usize) -> Result<()> {
    *work = work.saturating_add(amount);
    ensure!(*work <= MAX_PROJECTION_WORK, ProjectionBudget);
    Ok(())
}

fn pair_free_type(ty: &Type, work: &mut usize) -> Result<bool> {
    helper_charge(work, 1)?;
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        helper_charge(work, 1)?;
        match ty {
            Type::Pair { .. } => return Ok(false),
            Type::Tuple { items } => {
                helper_charge(work, items.len())?;
                pending.extend(items);
            }
            _ => (),
        }
    }
    Ok(true)
}

fn pair_free_helper(function: &crate::Function, work: &mut usize) -> Result<bool> {
    if !pair_free_type(&function.result, work)? {
        return Ok(false);
    }
    for parameter in &function.parameters {
        if !pair_free_type(&parameter.ty, work)? {
            return Ok(false);
        }
    }
    helper_charge(work, 1)?;
    let mut pending = vec![&function.body];
    while let Some(expression) = pending.pop() {
        helper_charge(work, 1)?;
        match expression {
            Expr::Pair { .. } | Expr::PairFirst { .. } | Expr::PairRest { .. } => {
                return Ok(false);
            }
            Expr::Call { function_id, .. } if *function_id != function.id => {
                return Ok(false);
            }
            Expr::Let { ty, .. } | Expr::Binary { ty, .. } | Expr::Int { ty, .. }
                if !pair_free_type(ty, work)? =>
            {
                return Ok(false);
            }
            _ => (),
        }
        let children = match expression {
            Expr::Tuple { items }
            | Expr::Sha256 { parts: items }
            | Expr::Call {
                arguments: items, ..
            } => items.len(),
            Expr::Let { .. }
            | Expr::Sequence { .. }
            | Expr::TreePair { .. }
            | Expr::Apply { .. }
            | Expr::Binary { .. } => 2,
            Expr::If { .. } => 3,
            Expr::AtomIsEmpty { .. }
            | Expr::Not { .. }
            | Expr::Field { .. }
            | Expr::TreeAtom { .. }
            | Expr::TreeAtomValue { .. }
            | Expr::TreeIsPair { .. }
            | Expr::TreeFirst { .. }
            | Expr::TreeRest { .. } => 1,
            Expr::Arg { .. }
            | Expr::Local { .. }
            | Expr::Atom { .. }
            | Expr::Bytes { .. }
            | Expr::Unit
            | Expr::Bool { .. }
            | Expr::Int { .. }
            | Expr::Reject { .. } => 0,
            Expr::Pair { .. } | Expr::PairFirst { .. } | Expr::PairRest { .. } => {
                unreachable!("typed pairs were excluded before helper traversal")
            }
        };
        helper_charge(work, children)?;
        crate::validate::children(expression, &mut pending);
    }
    Ok(true)
}

#[derive(Clone)]
struct Scope<'a> {
    frame: usize,
    parameters: &'a [Parameter],
    unary_tail: bool,
    slots: Vec<Binding>,
    locals: Vec<(usize, Type)>,
    // Total aliases retain a resolved value identity, never a local name that
    // could be rebound in another lexical scope. Slot paths resolve only when
    // consumed, after any intervening physical frames have been added.
    aliases: Vec<(usize, Value)>,
    pairs: Vec<Place>,
    atoms: Vec<Place>,
}
impl<'a> Scope<'a> {
    fn new(parameters: &'a [Parameter], frame: usize, compact_unary: bool) -> Self {
        Self {
            frame,
            parameters,
            unary_tail: compact_unary && parameters.len() == 1,
            slots: (0..parameters.len())
                .map(|index| Binding::Argument(frame, index))
                .collect(),
            locals: vec![],
            aliases: vec![],
            pairs: vec![],
            atoms: vec![],
        }
    }
    fn extended(&self, binding: Binding) -> Self {
        let mut next = self.clone();
        next.slots.insert(0, binding);
        next
    }
}

struct Runtime<'a, 'p> {
    lower: &'a mut Lower<'p>,
    next_temporary: usize,
    work: usize,
    optimize: bool,
    compact_unary: bool,
    pending_operands: usize,
    expression_depth: usize,
    guarded_projections: bool,
    projection_work: usize,
    projections: Vec<Projection>,
    projection_ids: BTreeMap<(Place, bool), usize>,
    atom_facts: bool,
    atom_work: usize,
}

type Continuation<'a, 'p, 'c> =
    dyn FnMut(&mut Runtime<'a, 'p>, &Scope<'_>, &[Value]) -> Result<LirId> + 'c;

impl<'a, 'p> Runtime<'a, 'p> {
    fn atom_charge(&mut self, amount: usize) -> Result<()> {
        self.atom_work = self.atom_work.saturating_add(amount);
        ensure!(self.atom_work <= 50_000, AtomBudget);
        Ok(())
    }

    fn projection_charge(&mut self, amount: usize) -> Result<()> {
        self.projection_work = self.projection_work.saturating_add(amount);
        ensure!(
            self.projection_work <= MAX_PROJECTION_WORK,
            ProjectionBudget
        );
        Ok(())
    }

    fn charge(&mut self, amount: usize) -> Result<()> {
        self.work = self.work.saturating_add(amount);
        ensure!(self.work <= 500_000, Limit::RuntimeWork);
        Ok(())
    }

    fn charge_type_clone(&mut self, ty: &Type) -> Result<()> {
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            self.charge(1)?;
            if let Type::Tuple { items } = ty {
                self.charge(items.len())?;
                pending.extend(items);
            } else if let Type::Pair { first, rest } = ty {
                self.charge(2)?;
                pending.extend([first.as_ref(), rest.as_ref()]);
            }
        }
        Ok(())
    }

    fn check_extension(&mut self, scope: &Scope<'_>) -> Result<()> {
        // Check before recursive operand lowering or cloning the frame. A
        // shallow AST can otherwise accumulate thousands of live temporaries
        // through nested, wide tuples before the serialized-size check runs.
        ensure!(
            scope.slots.len().saturating_sub(scope.parameters.len()) + scope.aliases.len() < 128,
            Limit::FrameBindings
        );
        self.charge(1 + scope.slots.len() + scope.aliases.len())?;
        self.projection_charge(scope.pairs.len())?;
        self.atom_charge(scope.atoms.len())?;
        for (_, ty) in &scope.locals {
            self.charge_type_clone(ty)?;
        }
        Ok(())
    }

    fn extend_scope<'s>(&mut self, scope: &Scope<'s>, binding: Binding) -> Result<Scope<'s>> {
        self.check_extension(scope)?;
        Ok(scope.extended(binding))
    }

    fn alias_scope<'s>(
        &mut self,
        scope: &Scope<'s>,
        local_id: usize,
        ty: &Type,
        value: Value,
    ) -> Result<Scope<'s>> {
        self.check_extension(scope)?;
        self.charge_type_clone(ty)?;
        let mut next = scope.clone();
        next.locals.push((local_id, ty.clone()));
        next.aliases.push((local_id, value));
        Ok(next)
    }

    fn local(scope: &Scope<'_>, local_id: usize) -> Value {
        scope
            .aliases
            .iter()
            .rev()
            .find_map(|(id, value)| (*id == local_id).then_some(*value))
            .unwrap_or_else(|| {
                Value::Slot(
                    *scope
                        .slots
                        .iter()
                        .find(|binding| matches!(binding, Binding::Local(id, _) if *id == local_id))
                        .expect("validated local has a captured binding"),
                )
            })
    }
    fn table(&mut self) -> LirId {
        self.lower.alloc(Lir::Path(2u8.into()))
    }

    fn arguments(&mut self, values: &[LirId]) -> LirId {
        if self.compact_unary && values.len() == 1 {
            values[0]
        } else {
            self.lower.list(values)
        }
    }

    fn slot_path(scope: &Scope<'_>, binding: Binding) -> Result<BigUint> {
        let index = scope
            .slots
            .iter()
            .position(|slot| *slot == binding)
            .ok_or_else(|| anyhow!("runtime binding outside its lexical scope"))?;
        let mut path = BigUint::from(3u8);
        for _ in 0..index {
            path = rest_path(&path);
        }
        // Only the original unary argument is the improper tail. Prepended
        // strict bindings remain ordinary cons cells. Resolve this physical
        // identity after all extensions, just like projection aliases.
        if scope.unary_tail && binding == Binding::Argument(scope.frame, 0) {
            ensure!(
                index + 1 == scope.slots.len(),
                "unary argument is not the frame tail"
            );
            Ok(path)
        } else {
            Ok(first_path(&path))
        }
    }

    fn slot(&mut self, scope: &Scope<'_>, binding: Binding) -> Result<LirId> {
        Ok(self
            .lower
            .alloc(Lir::Path(Self::slot_path(scope, binding)?)))
    }

    fn value(&mut self, scope: &Scope<'_>, value: Value) -> Result<LirId> {
        match value {
            Value::Slot(binding) => self.slot(scope, binding),
            Value::Literal(value) => Ok(value),
            Value::Projection(id) => {
                // Forwarding trades a cached value for repeated root paths.
                // Keep that trade small; high reuse or distant roots retry the
                // preceding emitter instead of duplicating arbitrarily costly
                // reads. These conservative bounds are not a cost theorem.
                ensure!(self.projections[id].reads < 4, ProjectionBudget);
                self.projections[id].reads += 1;
                let depth = self.projections[id].depth;
                let mut steps = Vec::new();
                let mut place = Place::Projection(id);
                let root = loop {
                    self.projection_charge(1)?;
                    match place {
                        Place::Slot(binding) => break binding,
                        Place::Projection(id) => {
                            let projection = self.projections[id];
                            steps.push(projection.first);
                            place = projection.parent;
                        }
                    }
                };
                self.projection_charge(scope.slots.len())?;
                let index = scope
                    .slots
                    .iter()
                    .position(|slot| *slot == root)
                    .ok_or_else(|| anyhow!("runtime projection root outside its lexical scope"))?;
                ensure!(index + depth <= 8, ProjectionBudget);
                let mut path = Self::slot_path(scope, root)?;
                for first in steps.into_iter().rev() {
                    path = if first {
                        first_path(&path)
                    } else {
                        rest_path(&path)
                    };
                }
                Ok(self.lower.alloc(Lir::Path(path)))
            }
        }
    }

    fn project(&mut self, parent: Place, first: bool) -> Result<Place> {
        self.projection_charge(1)?;
        if let Some(id) = self.projection_ids.get(&(parent, first)) {
            return Ok(Place::Projection(*id));
        }
        let depth = match parent {
            Place::Slot(_) => 1,
            Place::Projection(id) => self.projections[id].depth + 1,
        };
        ensure!(
            depth <= 64 && self.projections.len() < 4096,
            ProjectionBudget
        );
        let id = self.projections.len();
        self.projections.push(Projection {
            parent,
            first,
            depth,
            reads: 0,
        });
        self.projection_ids.insert((parent, first), id);
        Ok(Place::Projection(id))
    }

    fn total_place(&mut self, expression: &Expr, scope: &Scope<'_>) -> Result<Option<Place>> {
        let mut expression = expression;
        let mut steps = Vec::new();
        loop {
            self.projection_charge(1)?;
            match expression {
                Expr::TreeFirst { value } | Expr::TreeRest { value } => {
                    ensure!(steps.len() < 64, ProjectionBudget);
                    steps.push(matches!(expression, Expr::TreeFirst { .. }));
                    expression = value;
                }
                _ => break,
            }
        }
        let value = match expression {
            Expr::Arg { index } => Value::Slot(Binding::Argument(scope.frame, *index)),
            Expr::Local { local_id } => Self::local(scope, *local_id),
            _ => return Ok(None),
        };
        let mut place = match value {
            Value::Slot(binding) => Place::Slot(binding),
            Value::Projection(id) => Place::Projection(id),
            Value::Literal(_) => return Ok(None),
        };
        for first in steps.into_iter().rev() {
            self.projection_charge(scope.pairs.len())?;
            if !scope.pairs.contains(&place) {
                return Ok(None);
            }
            place = self.project(place, first)?;
        }
        Ok(Some(place))
    }

    fn pair_branch(
        &mut self,
        condition: &Expr,
        scope: &Scope<'_>,
    ) -> Result<Option<(Place, bool)>> {
        let mut condition = condition;
        let mut positive = true;
        while let Expr::Not { value } = condition {
            self.projection_charge(1)?;
            positive = !positive;
            condition = value;
        }
        if let Expr::TreeIsPair { value } = condition {
            return Ok(self
                .total_place(value, scope)?
                .map(|place| (place, positive)));
        }
        Ok(None)
    }

    fn pair_scope<'s>(&mut self, scope: &Scope<'s>, place: Place) -> Result<Scope<'s>> {
        ensure!(scope.pairs.len() < 128, ProjectionBudget);
        if self.atom_facts {
            ensure!(scope.pairs.len() + scope.atoms.len() < 128, AtomBudget);
            self.atom_charge(scope.atoms.len())?;
        }
        self.projection_charge(1 + scope.pairs.len() + scope.slots.len() + scope.aliases.len())?;
        for (_, ty) in &scope.locals {
            self.charge_type_clone(ty)?;
        }
        let mut next = scope.clone();
        if !next.pairs.contains(&place) {
            next.pairs.push(place);
        }
        Ok(next)
    }

    fn atom_scope<'s>(&mut self, scope: &Scope<'s>, place: Place) -> Result<Scope<'s>> {
        ensure!(scope.pairs.len() + scope.atoms.len() < 128, AtomBudget);
        self.atom_charge(
            1 + scope.atoms.len() + scope.pairs.len() + scope.slots.len() + scope.aliases.len(),
        )?;
        for (_, ty) in &scope.locals {
            self.charge_type_clone(ty)?;
        }
        let mut next = scope.clone();
        if !next.atoms.contains(&place) {
            next.atoms.push(place);
        }
        Ok(next)
    }

    fn is_leaf(expression: &Expr) -> bool {
        matches!(
            expression,
            Expr::Arg { .. }
                | Expr::Local { .. }
                | Expr::Atom { .. }
                | Expr::Bytes { .. }
                | Expr::Unit
                | Expr::Bool { .. }
                | Expr::Int { .. }
        )
    }

    fn leaf(&mut self, expression: &Expr, scope: &Scope<'_>) -> Result<Value> {
        Ok(match expression {
            Expr::Arg { index } => Value::Slot(Binding::Argument(scope.frame, *index)),
            Expr::Local { local_id } => Self::local(scope, *local_id),
            _ => Value::Literal(self.expression(expression, scope)?),
        })
    }

    fn operand(&mut self) -> Result<()> {
        if self.optimize {
            // Forwarded values do not extend physical frames. Bound pending
            // descriptors independently, before growing their vectors, so wide
            // nested total operands cannot evade the old compilation bound.
            ensure!(self.pending_operands < 128, Limit::PendingOperands);
            self.pending_operands += 1;
            self.charge(1)?;
        }
        Ok(())
    }

    fn list_slot(&mut self, root: u8, index: usize) -> LirId {
        let mut path = BigUint::from(root);
        for _ in 0..index {
            path = rest_path(&path);
        }
        self.lower.alloc(Lir::Path(first_path(&path)))
    }

    fn continuation(&mut self, value: LirId, body: LirId) -> LirId {
        // The only possibly fallible computation in this new frame is `value`.
        // It is evaluated once before Run enters the continuation, even when
        // that continuation ignores slot zero. Other operands are frame paths.
        let previous = self.lower.alloc(Lir::Path(3u8.into()));
        let slots = self.lower.alloc(Lir::Cons(value, previous));
        let table = self.table();
        let frame = self.lower.alloc(Lir::Cons(table, slots));
        let quoted = self.lower.alloc(Lir::Quote(body));
        self.lower.alloc(Lir::Run(quoted, frame))
    }

    fn values(
        &mut self,
        expressions: &[&Expr],
        scope: &Scope<'_>,
        bindings: &mut Vec<Value>,
        finish: &mut Continuation<'a, 'p, '_>,
    ) -> Result<LirId> {
        self.charge(1)?;
        let count = bindings.len();
        let pending = self.pending_operands;
        let result = (|| {
            let mut remaining = expressions;
            if self.optimize {
                // Consume total leaves iteratively, never one Rust call per
                // skipped frame. Slot identities are resolved only in finish's
                // final scope, after every effectful operand has been bound.
                while let Some((first, rest)) = remaining.split_first() {
                    if !Self::is_leaf(first) {
                        break;
                    }
                    self.operand()?;
                    bindings.push(self.leaf(first, scope)?);
                    remaining = rest;
                }
            }
            if let Some((first, remaining)) = remaining.split_first() {
                let value = self.expression(first, scope)?;
                let binding = Binding::Temporary(self.next_temporary);
                self.next_temporary += 1;
                let next = self.extend_scope(scope, binding)?;
                self.operand()?;
                bindings.push(Value::Slot(binding));
                let body = self.values(remaining, &next, bindings, finish)?;
                Ok(self.continuation(value, body))
            } else {
                finish(self, scope, bindings)
            }
        })();
        bindings.truncate(count);
        self.pending_operands = pending;
        result
    }

    fn expression(&mut self, expression: &Expr, scope: &Scope<'_>) -> Result<LirId> {
        self.charge(1)?;
        if self.optimize {
            // Direct unary lowering must not trade frame bounds for an
            // unbounded native compiler stack. Check before descending.
            ensure!(self.expression_depth < 192, Limit::RuntimeDepth);
            self.expression_depth += 1;
        }
        let result = self.expression_inner(expression, scope);
        if self.optimize {
            self.expression_depth -= 1;
        }
        result
    }

    fn expression_inner(&mut self, expression: &Expr, scope: &Scope<'_>) -> Result<LirId> {
        Ok(match expression {
            Expr::Atom { hex } | Expr::Bytes { hex } => self.lower.bytes(hex::decode(hex)?),
            Expr::Unit => self.lower.nil(),
            Expr::Bool { value } => self.lower.int(i64::from(*value)),
            Expr::Int { value, .. } => self.lower.number(value.parse()?),
            Expr::Arg { index } => self.slot(scope, Binding::Argument(scope.frame, *index))?,
            Expr::Local { local_id } => self.value(scope, Self::local(scope, *local_id))?,
            Expr::Reject { .. } => self.lower.reject(),
            Expr::TreeAtom { value } => self.expression(value, scope)?,
            Expr::Let {
                local_id,
                ty,
                value,
                body,
            } => {
                if self.optimize
                    && matches!(body.as_ref(), Expr::Local { local_id: returned } if returned == local_id)
                {
                    // Returning the freshly bound value needs no continuation.
                    // The initializer still runs exactly once in its outer scope,
                    // including when it rejects or diverges.
                    return self.expression(value, scope);
                }
                if self.optimize && Self::is_leaf(value) {
                    // A decoded argument, already-bound local or literal has
                    // no initializer effect to sequence in a new VM frame.
                    // Capture aliases now, while their original binding is in
                    // scope, and rebase that identity at the final consumer.
                    let value = self.leaf(value, scope)?;
                    let next = self.alias_scope(scope, *local_id, ty, value)?;
                    return self.expression(body, &next);
                }
                if self.guarded_projections
                    && matches!(
                        value.as_ref(),
                        Expr::TreeFirst { .. } | Expr::TreeRest { .. }
                    )
                    && let Some(place) = self.total_place(value, scope)?
                {
                    let Place::Projection(id) = place else {
                        unreachable!("a projection initializer has a projection place")
                    };
                    let next = self.alias_scope(scope, *local_id, ty, Value::Projection(id))?;
                    return self.expression(body, &next);
                }
                let value = self.expression(value, scope)?;
                self.charge_type_clone(ty)?;
                let binding = Binding::Local(*local_id, self.next_temporary);
                self.next_temporary += 1;
                let mut next = self.extend_scope(scope, binding)?;
                next.locals.push((*local_id, ty.clone()));
                let body = self.expression(body, &next)?;
                self.continuation(value, body)
            }
            Expr::If {
                condition,
                then_value,
                else_value,
            } => {
                let fact = if self.guarded_projections {
                    self.pair_branch(condition, scope)?
                } else {
                    None
                };
                let condition = self.expression(condition, scope)?;
                let (yes, no) = if let Some((place, positive)) = fact {
                    let pair = self.pair_scope(scope, place)?;
                    let atom = if self.atom_facts {
                        Some(self.atom_scope(scope, place)?)
                    } else {
                        None
                    };
                    let atom = atom.as_ref().unwrap_or(scope);
                    (
                        self.expression(then_value, if positive { &pair } else { atom })?,
                        self.expression(else_value, if positive { atom } else { &pair })?,
                    )
                } else {
                    (
                        self.expression(then_value, scope)?,
                        self.expression(else_value, scope)?,
                    )
                };
                self.lower.branch(condition, yes, no)
            }
            Expr::Sequence { first, then_value } => {
                if self.optimize && Self::is_leaf(first) {
                    return self.expression(then_value, scope);
                }
                let value = self.expression(first, scope)?;
                let binding = Binding::Temporary(self.next_temporary);
                self.next_temporary += 1;
                let next = self.extend_scope(scope, binding)?;
                let body = self.expression(then_value, &next)?;
                self.continuation(value, body)
            }
            Expr::Call {
                function_id,
                arguments,
            } => {
                let values = arguments.iter().collect::<Vec<_>>();
                self.values(&values, scope, &mut vec![], &mut |this, scope, bindings| {
                    let callee = this.list_slot(2, *function_id);
                    let values = bindings
                        .iter()
                        .map(|binding| this.value(scope, *binding))
                        .collect::<Result<Vec<_>>>()?;
                    let arguments = this.arguments(&values);
                    let table = this.table();
                    let frame = this.lower.alloc(Lir::Cons(table, arguments));
                    Ok(this.lower.alloc(Lir::Run(callee, frame)))
                })?
            }
            Expr::Tuple { items } => {
                let items = items.iter().collect::<Vec<_>>();
                self.values(&items, scope, &mut vec![], &mut |this, scope, bindings| {
                    let values = bindings
                        .iter()
                        .map(|binding| this.value(scope, *binding))
                        .collect::<Result<Vec<_>>>()?;
                    Ok(this.lower.list(&values))
                })?
            }
            Expr::TreePair { first, rest } | Expr::Pair { first, rest } => self.values(
                &[first.as_ref(), rest.as_ref()],
                scope,
                &mut vec![],
                &mut |this, scope, bindings| {
                    let first = this.value(scope, bindings[0])?;
                    let rest = this.value(scope, bindings[1])?;
                    Ok(this.lower.alloc(Lir::Cons(first, rest)))
                },
            )?,
            Expr::Sha256 { parts } => self.values(
                &parts.iter().collect::<Vec<_>>(),
                scope,
                &mut vec![],
                &mut |this, scope, bindings| {
                    let parts = bindings
                        .iter()
                        .map(|binding| this.value(scope, *binding))
                        .collect::<Result<Vec<_>>>()?;
                    Ok(this.lower.alloc(Lir::Sha256(parts)))
                },
            )?,
            Expr::Apply {
                program,
                environment,
            } => self.values(
                &[program.as_ref(), environment.as_ref()],
                scope,
                &mut vec![],
                &mut |this, scope, bindings| {
                    let program = this.value(scope, bindings[0])?;
                    let environment = this.value(scope, bindings[1])?;
                    Ok(this.lower.alloc(Lir::Run(program, environment)))
                },
            )?,
            Expr::Binary {
                kind,
                left,
                right,
                ty,
            } => self.values(
                &[left.as_ref(), right.as_ref()],
                scope,
                &mut vec![],
                &mut |this, scope, bindings| {
                    let left = this.value(scope, bindings[0])?;
                    let right = this.value(scope, bindings[1])?;
                    this.lower.binary(*kind, left, right, ty)
                },
            )?,
            Expr::Field { value, index } => {
                let ty =
                    self.lower
                        .program
                        .type_of_scoped(value, scope.parameters, &scope.locals)?;
                if self.optimize {
                    let value = self.expression(value, scope)?;
                    return Ok(self.lower.field(value, *index, ty));
                }
                self.values(
                    &[value.as_ref()],
                    scope,
                    &mut vec![],
                    &mut |this, scope, bindings| {
                        let value = this.value(scope, bindings[0])?;
                        if matches!(ty, Some(Type::Tuple { .. })) {
                            let Lir::Path(mut path) = this.lower.arena[value].clone() else {
                                unreachable!("a bound value is a frame path")
                            };
                            for _ in 0..*index {
                                path = rest_path(&path);
                            }
                            Ok(this.lower.alloc(Lir::Path(first_path(&path))))
                        } else {
                            Ok(this.lower.field(value, *index, ty.clone()))
                        }
                    },
                )?
            }
            Expr::AtomIsEmpty { value }
            | Expr::Not { value }
            | Expr::TreeIsPair { value }
            | Expr::TreeFirst { value }
            | Expr::TreeRest { value }
            | Expr::PairFirst { value }
            | Expr::PairRest { value }
            | Expr::TreeAtomValue { value } => {
                if self.atom_facts
                    && matches!(expression, Expr::TreeAtomValue { .. })
                    && let Some(place) = self.total_place(value, scope)?
                {
                    self.atom_charge(scope.atoms.len())?;
                    if scope.atoms.contains(&place) {
                        // The original branch already checked this exact,
                        // immutable captured value. Resolve its physical
                        // place now; no effectful operand is discarded.
                        return self.value(
                            scope,
                            match place {
                                Place::Slot(binding) => Value::Slot(binding),
                                Place::Projection(id) => Value::Projection(id),
                            },
                        );
                    }
                }
                if self.optimize
                    && matches!(expression, Expr::TreeAtomValue { .. })
                    && let Expr::TreeAtom { value } = value.as_ref()
                {
                    // A validated atom constructor cannot produce a pair.
                    // Its payload still evaluates once, including calls
                    // and unused SHA computations; only the redundant
                    // shape guard and its temporary frame disappear.
                    return self.expression(value, scope);
                }
                // These unary operations consume the operand once. AtomValue
                // uses it in both a guard and result, so only total leaves may
                // bypass the once-only binding for that operation.
                if self.optimize
                    && (!matches!(expression, Expr::TreeAtomValue { .. }) || Self::is_leaf(value))
                {
                    let value = self.expression(value, scope)?;
                    return Ok(self.unary(expression, value));
                }
                self.values(
                    &[value.as_ref()],
                    scope,
                    &mut vec![],
                    &mut |this, scope, bindings| {
                        let value = this.value(scope, bindings[0])?;
                        Ok(this.unary(expression, value))
                    },
                )?
            }
        })
    }

    fn unary(&mut self, expression: &Expr, value: LirId) -> LirId {
        match expression {
            Expr::AtomIsEmpty { .. } => {
                let nil = self.lower.nil();
                self.lower.alloc(Lir::Eq(value, nil))
            }
            Expr::Not { .. } => self.lower.alloc(Lir::Not(value)),
            Expr::TreeIsPair { .. } => self.lower.alloc(Lir::Listp(value, true)),
            Expr::TreeFirst { .. } | Expr::PairFirst { .. } => self.lower.alloc(Lir::First(value)),
            Expr::TreeRest { .. } | Expr::PairRest { .. } => self.lower.alloc(Lir::Rest(value)),
            Expr::TreeAtomValue { .. } => self.lower.scalar(value, value),
            _ => unreachable!("matched unary expression"),
        }
    }
}

pub(super) fn bootstrap(
    lower: &mut Lower<'_>,
    optimize: bool,
    guarded_projections: bool,
    atom_facts: bool,
    compact_unary: bool,
) -> Result<LirId> {
    let program = lower.program;
    // Typed-pair bodies and the v5 entry retain the preceding strict emitter.
    // Only structurally pair-free leaf/self-recursive table helpers may request
    // the existing Tree facts. Every v5 call keeps proper argument frames;
    // helper-local facts do not admit compact unary layouts or Pair rewrites.
    let preflighted = guarded_projections && projection_preflight(program);
    let pair_capability = preflighted && program.uses_pairs();
    let guarded_projections = preflighted;
    let mut runtime = Runtime {
        lower,
        next_temporary: 0,
        work: 0,
        optimize,
        compact_unary: optimize && compact_unary && guarded_projections && !pair_capability,
        pending_operands: 0,
        expression_depth: 0,
        guarded_projections,
        projection_work: 0,
        projections: vec![],
        projection_ids: BTreeMap::new(),
        atom_facts: atom_facts && guarded_projections,
        atom_work: 0,
    };
    let mut functions = Vec::with_capacity(program.functions.len());
    for (frame, function) in program.functions.iter().enumerate() {
        // Pair-capable tables use the same branch-local facts in every body;
        // explicit pair constructors keep strict operand frames and typed pair
        // projections forward only under a recorded fact. The pair-free scan
        // still charges and classifies helpers so its budget stays observable.
        let helper_facts = if preflighted && pair_capability {
            pair_free_helper(function, &mut runtime.projection_work)?;
            guarded_projections
        } else {
            guarded_projections
        };
        runtime.guarded_projections = helper_facts;
        runtime.atom_facts = atom_facts && helper_facts;
        let body = runtime.expression(
            &function.body,
            &Scope::new(&function.parameters, frame, runtime.compact_unary),
        )?;
        functions.push(runtime.lower.alloc(Lir::Quote(body)));
    }
    let table = runtime.lower.list(&functions);
    // Facts/options are body-local; never inherit the last helper's admission.
    runtime.guarded_projections = guarded_projections;
    runtime.atom_facts = atom_facts && guarded_projections;
    let body = runtime.expression(
        &program.body,
        &Scope::new(
            &program.parameters,
            program.functions.len(),
            runtime.compact_unary,
        ),
    )?;
    let body = runtime.lower.alloc(Lir::Quote(body));
    let arguments = runtime.arguments(&runtime.lower.args.clone());
    let frame = runtime.lower.alloc(Lir::Cons(table, arguments));
    Ok(runtime.lower.alloc(Lir::Run(body, frame)))
}

#[cfg(test)]
mod guarded_projection_limits {
    use super::*;
    use crate::{Environment, Function};

    fn argument() -> Expr {
        Expr::Arg { index: 0 }
    }
    fn guarded() -> Expr {
        Expr::If {
            condition: Box::new(Expr::TreeIsPair {
                value: Box::new(argument()),
            }),
            then_value: Box::new(argument()),
            else_value: Box::new(argument()),
        }
    }
    fn source() -> Program {
        let mut body = Expr::Tuple {
            items: vec![
                Expr::Tuple {
                    items: vec![guarded(); 8]
                };
                8
            ],
        };
        for local_id in 0..48 {
            body = Expr::Let {
                local_id,
                ty: Type::Tree,
                value: Box::new(argument()),
                body: Box::new(body),
            };
        }
        Program {
            name: "optional-projection-budget".into(),
            environment: Environment::Raw,
            parameters: vec![Parameter {
                name: "tree".into(),
                ty: Type::Tree,
            }],
            result: Type::Tree,
            body: argument(),
            functions: (0..16)
                .map(|id| Function {
                    id,
                    name: format!("unused_{id}"),
                    parameters: vec![Parameter {
                        name: "tree".into(),
                        ty: Type::Tree,
                    }],
                    result: Type::Tuple {
                        items: vec![
                            Type::Tuple {
                                items: vec![Type::Tree; 8]
                            };
                            8
                        ],
                    },
                    body: body.clone(),
                })
                .collect(),
        }
    }

    #[test]
    fn optional_analysis_budget_retries_a_fresh_exact_old_on_emitter() {
        let source = source();
        source.validate().unwrap();
        assert!(projection_preflight(&source));
        let error = super::super::emit_attempt(&source, true, true, true).unwrap_err();
        assert!(error.is::<ProjectionBudget>(), "{error:?}");
        let previous = super::super::emit_attempt(&source, true, true, false).unwrap();
        assert_eq!(super::super::emit(&source, true, true).unwrap(), previous);
        // The unoptimized route remains independent of optional metadata.
        let previous = super::super::emit_attempt(&source, false, true, false).unwrap_err();
        let current = super::super::emit(&source, false, true).unwrap_err();
        assert_eq!(current.to_string(), previous.to_string());
    }

    #[test]
    fn complete_preflight_and_fallback_keep_dead_validation_and_old_limits() {
        let mut source = source();
        let mut value = argument();
        for _ in 0..64 {
            value = Expr::TreeFirst {
                value: Box::new(value),
            };
        }
        source.functions[0].body = value;
        source.functions[0].result = Type::Tree;
        source.validate().unwrap();
        assert!(!projection_preflight(&source));
        let previous = super::super::emit_attempt(&source, true, true, false);
        let current = super::super::emit(&source, true, true);
        match (previous, current) {
            (Ok(left), Ok(right)) => assert_eq!(left, right),
            (Err(left), Err(right)) => assert_eq!(left.to_string(), right.to_string()),
            other => panic!("optional preflight changed prior admission: {other:?}"),
        }
        source.functions[0].body = Expr::Arg { index: 100 };
        let error = super::super::emit(&source, true, true).unwrap_err();
        assert!(error.to_string().contains("invalid argument 100"));
        assert!(!error.is::<ProjectionBudget>() && !error.is::<Limit>());
    }
}

#[cfg(test)]
mod projection_cost_controls {
    use super::*;
    use crate::Environment;
    use clvmr::{
        Allocator, ChiaDialect, MEMPOOL_MODE, run_program,
        serde::{node_from_bytes, node_to_bytes},
    };

    fn repeated(local_id: usize, uses: usize) -> (Expr, Type, Vec<u8>) {
        let mut values = Vec::new();
        let mut types = Vec::new();
        let mut expected = Vec::new();
        if uses <= 8 {
            for _ in 0..uses {
                values.push(Expr::Local { local_id });
                types.push(Type::Tree);
                expected.extend([0xff, 0x82, 0, 0]);
            }
        } else {
            let group = uses.div_ceil(8);
            for start in (0..uses).step_by(group) {
                let (value, ty, bytes) = repeated(local_id, group.min(uses - start));
                values.push(value);
                types.push(ty);
                expected.push(0xff);
                expected.extend(bytes);
            }
        }
        expected.push(0x80);
        (
            Expr::Tuple { items: values },
            Type::Tuple { items: types },
            expected,
        )
    }
    fn source(depth: usize, uses: usize) -> Program {
        let (mut body, result, _) = repeated(depth - 1, uses);
        for local_id in (0..depth).rev() {
            let parent = if local_id == 0 {
                Expr::Arg { index: 0 }
            } else {
                Expr::Local {
                    local_id: local_id - 1,
                }
            };
            body = Expr::If {
                condition: Box::new(Expr::TreeIsPair {
                    value: Box::new(parent.clone()),
                }),
                then_value: Box::new(Expr::Let {
                    local_id,
                    ty: Type::Tree,
                    value: Box::new(Expr::TreeRest {
                        value: Box::new(parent),
                    }),
                    body: Box::new(body),
                }),
                else_value: Box::new(Expr::Reject {
                    message: "pair required".into(),
                }),
            };
        }
        Program {
            name: "projection-repeated-use".into(),
            environment: Environment::Raw,
            parameters: vec![Parameter {
                name: "tree".into(),
                ty: Type::Tree,
            }],
            result,
            body,
            functions: vec![],
        }
    }
    fn execute_output(code: &[u8], depth: usize, expected: Vec<u8>) -> u64 {
        let mut environment = vec![0x82, 0, 0];
        for _ in 0..depth {
            environment.splice(0..0, [0xff, 0x80]);
        }
        let mut allocator = Allocator::new();
        let code = node_from_bytes(&mut allocator, code).unwrap();
        let environment = node_from_bytes(&mut allocator, &environment).unwrap();
        let result = run_program(
            &mut allocator,
            &ChiaDialect::new(MEMPOOL_MODE),
            code,
            environment,
            100_000_000,
        )
        .unwrap();
        assert_eq!(node_to_bytes(&allocator, result.1).unwrap(), expected);
        result.0
    }
    fn execute(code: &[u8], depth: usize, uses: usize) -> u64 {
        execute_output(code, depth, repeated(0, uses).2)
    }

    fn tail(source: &mut Program, depth: usize) -> &mut Expr {
        let mut expression = &mut source.body;
        for _ in 0..depth {
            let Expr::If { then_value, .. } = expression else {
                unreachable!()
            };
            let Expr::Let { body, .. } = then_value.as_mut() else {
                unreachable!()
            };
            expression = body;
        }
        expression
    }

    #[test]
    fn repeated_call_arguments_and_distant_roots_keep_old_admission_and_costs() {
        for depth in [1, 8] {
            for uses in [2, 8, 32] {
                let mut source = source(depth, uses);
                *tail(&mut source, depth) = Expr::Call {
                    function_id: 0,
                    arguments: vec![
                        Expr::Local {
                            local_id: depth - 1
                        };
                        uses
                    ],
                };
                let arguments = (0..uses)
                    .map(|index| Expr::Arg { index })
                    .collect::<Vec<_>>();
                source.result = Type::Tuple {
                    items: vec![Type::Tree; uses],
                };
                source.functions.push(crate::Function {
                    id: 0,
                    name: "recursive_tuple".into(),
                    parameters: (0..uses)
                        .map(|index| Parameter {
                            name: format!("value_{index}"),
                            ty: Type::Tree,
                        })
                        .collect(),
                    result: source.result.clone(),
                    body: Expr::If {
                        condition: Box::new(Expr::TreeIsPair {
                            value: Box::new(Expr::Arg { index: 0 }),
                        }),
                        then_value: Box::new(Expr::Call {
                            function_id: 0,
                            arguments: arguments.clone(),
                        }),
                        else_value: Box::new(Expr::Tuple { items: arguments }),
                    },
                });
                let plan = crate::optimize::plan(&source).unwrap();
                let previous =
                    super::super::emit_attempt(&plan.program, true, true, false).unwrap();
                let current = super::super::emit(&plan.program, true, true).unwrap();
                let mut expected = [0xff, 0x82, 0, 0].repeat(uses);
                expected.push(0x80);
                let old_cost = execute_output(&previous, depth, expected.clone());
                let new_cost = execute_output(&current, depth, expected);
                println!(
                    "projection call depth={depth} uses={uses}: bytes {}->{}, VM {old_cost}->{new_cost}",
                    previous.len(),
                    current.len()
                );
                assert!(current.len() <= previous.len() && new_cost <= old_cost);
            }
        }
        let mut source = source(1, 2);
        let mut body = tail(&mut source, 1).clone();
        for _ in 0..10 {
            body = Expr::Sequence {
                first: Box::new(Expr::Call {
                    function_id: 0,
                    arguments: vec![Expr::Arg { index: 0 }],
                }),
                then_value: Box::new(body),
            };
        }
        *tail(&mut source, 1) = body;
        source.functions.push(crate::Function {
            id: 0,
            name: "recursive_identity".into(),
            parameters: source.parameters.clone(),
            result: Type::Tree,
            body: Expr::If {
                condition: Box::new(Expr::TreeIsPair {
                    value: Box::new(Expr::Arg { index: 0 }),
                }),
                then_value: Box::new(Expr::Arg { index: 0 }),
                else_value: Box::new(Expr::Call {
                    function_id: 0,
                    arguments: vec![Expr::Arg { index: 0 }],
                }),
            },
        });
        let plan = crate::optimize::plan(&source).unwrap();
        let error = super::super::emit_attempt(&plan.program, true, true, true).unwrap_err();
        assert!(error.is::<ProjectionBudget>(), "{error:?}");
        let previous = super::super::emit_attempt(&plan.program, true, true, false).unwrap();
        let current = super::super::emit(&plan.program, true, true).unwrap();
        assert_eq!(current, previous);
        assert_eq!(execute(&current, 1, 2), execute(&previous, 1, 2));
    }
    #[test]
    fn deep_repeated_reads_retain_a_bounded_cost_policy() {
        let mut compared = 0;
        let mut regressions = Vec::new();
        for depth in [1, 2, 4, 8, 12, 16] {
            for uses in [2, 8, 32, 128, 512] {
                let source = source(depth, uses);
                source.validate().unwrap();
                let plan = crate::optimize::plan(&source).unwrap();
                let previous = super::super::emit_attempt(&plan.program, true, true, false);
                let current = super::super::emit(&plan.program, true, true);
                if let Ok(previous) = previous {
                    let current = current.unwrap();
                    let old_cost = execute(&previous, depth, uses);
                    let new_cost = execute(&current, depth, uses);
                    println!(
                        "projection depth={depth} uses={uses}: bytes {}->{}, VM {old_cost}->{new_cost}",
                        previous.len(),
                        current.len()
                    );
                    compared += 1;
                    if current.len() > previous.len() || new_cost > old_cost {
                        regressions.push((
                            depth,
                            uses,
                            previous.len(),
                            current.len(),
                            old_cost,
                            new_cost,
                        ));
                    }
                }
            }
        }
        assert!(compared >= 24);
        assert!(
            regressions.is_empty(),
            "deep repeated reads regress: {regressions:?}"
        );
    }
}

#[cfg(test)]
mod atom_fact_limits {
    use super::*;
    use crate::{Environment, Function};

    #[test]
    fn a_proven_atom_cast_removes_only_its_guard_and_capture() {
        let source = Program {
            name: "known-atom-positive".into(),
            environment: Environment::Raw,
            parameters: vec![Parameter {
                name: "tree".into(),
                ty: Type::Tree,
            }],
            result: Type::Atom,
            body: Expr::If {
                condition: Box::new(Expr::TreeIsPair {
                    value: Box::new(Expr::Arg { index: 0 }),
                }),
                then_value: Box::new(Expr::Reject {
                    message: "pair".into(),
                }),
                else_value: Box::new(Expr::TreeAtomValue {
                    value: Box::new(Expr::Arg { index: 0 }),
                }),
            },
            functions: vec![],
        };
        source.validate().unwrap();
        let previous = super::super::emit_attempt(&source, true, true, true).unwrap();
        let current = super::super::emit_atom_attempt(&source, true, true, true, true).unwrap();
        assert!(
            current.len() < previous.len(),
            "{} -> {} bytes",
            previous.len(),
            current.len()
        );
        for code in [previous, current] {
            for environment in [vec![0x80], vec![0], vec![0x82, 0, 0], vec![0x82, 0, 128]] {
                let mut allocator = clvmr::Allocator::new();
                let code = clvmr::serde::node_from_bytes(&mut allocator, &code).unwrap();
                let input = clvmr::serde::node_from_bytes(&mut allocator, &environment).unwrap();
                let output = clvmr::run_program(
                    &mut allocator,
                    &clvmr::ChiaDialect::new(clvmr::MEMPOOL_MODE),
                    code,
                    input,
                    100_000,
                )
                .unwrap();
                assert_eq!(
                    clvmr::serde::node_to_bytes(&allocator, output.1).unwrap(),
                    environment
                );
            }
        }
    }

    #[test]
    fn atom_budget_retries_the_fresh_current_pair_only_emitter() {
        let mut body = Expr::Arg { index: 0 };
        for index in (0..16).rev() {
            body = Expr::If {
                condition: Box::new(Expr::TreeIsPair {
                    value: Box::new(Expr::Arg { index }),
                }),
                then_value: Box::new(Expr::Arg { index: 0 }),
                else_value: Box::new(body),
            };
        }
        let parameters = (0..64)
            .map(|index| Parameter {
                name: format!("tree_{index}"),
                ty: Type::Tree,
            })
            .collect::<Vec<_>>();
        let source = Program {
            name: "atom-budget-before-copy".into(),
            environment: Environment::Raw,
            parameters: parameters[..1].to_vec(),
            result: Type::Tree,
            body: Expr::Arg { index: 0 },
            functions: (0..40)
                .map(|id| Function {
                    id,
                    name: format!("unused_{id}"),
                    parameters: parameters.clone(),
                    result: Type::Tree,
                    body: body.clone(),
                })
                .collect(),
        };
        source.validate().unwrap();
        assert!(projection_preflight(&source));
        let error = super::super::emit_atom_attempt(&source, true, true, true, true).unwrap_err();
        assert!(error.is::<AtomBudget>(), "{error:?}");
        let previous = super::super::emit_attempt(&source, true, true, true).unwrap();
        assert_eq!(super::super::emit(&source, true, true).unwrap(), previous);
        let previous_off = super::super::emit_attempt(&source, false, true, false).unwrap();
        assert_eq!(
            super::super::emit(&source, false, true).unwrap(),
            previous_off
        );
    }
}

#[cfg(test)]
mod pair_optimization_boundary {
    use super::*;
    use crate::{Environment, Function};

    fn argument() -> Expr {
        Expr::Arg { index: 0 }
    }

    fn source() -> Program {
        let parameter = Parameter {
            name: "tree".into(),
            ty: Type::Tree,
        };
        Program {
            name: "pair-optimization-boundary".into(),
            environment: Environment::Raw,
            parameters: vec![parameter.clone()],
            result: Type::Tree,
            body: Expr::Call {
                function_id: 0,
                arguments: vec![argument()],
            },
            functions: vec![
                Function {
                    id: 0,
                    name: "branch_and_capture".into(),
                    parameters: vec![parameter.clone()],
                    result: Type::Tree,
                    body: Expr::If {
                        condition: Box::new(Expr::TreeIsPair {
                            value: Box::new(argument()),
                        }),
                        then_value: Box::new(Expr::Let {
                            local_id: 0,
                            ty: Type::Tree,
                            value: Box::new(Expr::TreeFirst {
                                value: Box::new(argument()),
                            }),
                            body: Box::new(Expr::Sequence {
                                first: Box::new(Expr::Call {
                                    function_id: 1,
                                    arguments: vec![argument()],
                                }),
                                then_value: Box::new(Expr::TreePair {
                                    first: Box::new(Expr::Local { local_id: 0 }),
                                    rest: Box::new(argument()),
                                }),
                            }),
                        }),
                        else_value: Box::new(Expr::TreeAtom {
                            value: Box::new(Expr::TreeAtomValue {
                                value: Box::new(argument()),
                            }),
                        }),
                    },
                },
                Function {
                    id: 1,
                    name: "strict_identity".into(),
                    parameters: vec![parameter],
                    result: Type::Tree,
                    body: argument(),
                },
            ],
        }
    }

    fn execute(code: &[u8], input: &str, expected: &str) {
        let mut allocator = clvmr::Allocator::new();
        let code = clvmr::serde::node_from_bytes(&mut allocator, code).unwrap();
        let input =
            clvmr::serde::node_from_bytes(&mut allocator, &hex::decode(input).unwrap()).unwrap();
        let output = clvmr::run_program(
            &mut allocator,
            &clvmr::ChiaDialect::new(clvmr::MEMPOOL_MODE),
            code,
            input,
            100_000,
        )
        .unwrap();
        assert_eq!(
            clvmr::serde::node_to_bytes(&allocator, output.1).unwrap(),
            hex::decode(expected).unwrap()
        );
    }

    #[test]
    fn pair_result_and_unused_pair_signature_use_facts_without_compact_frames() {
        let ordinary = source();
        ordinary.validate().unwrap();
        assert!(!ordinary.uses_pairs());
        let emit = |program: &Program, guarded, atoms, compact| {
            super::super::emit_layout_attempt(program, true, true, guarded, atoms, compact).unwrap()
        };
        let strict = emit(&ordinary, false, false, false);
        let projected = emit(&ordinary, true, false, false);
        let atom = emit(&ordinary, true, true, false);
        let compact = emit(&ordinary, true, true, true);
        // Each requested mechanism must have a real emitted-program effect.
        // The first alias survives a strict call; the atom cast is in the
        // opposite branch; entry and both callees have one logical argument.
        assert_ne!(projected, strict);
        assert_ne!(atom, projected);
        assert_ne!(compact, atom);
        for code in [&strict, &projected, &atom, &compact] {
            for (input, expected) in [
                ("80", "80"),
                ("00", "00"),
                ("820080", "820080"),
                ("ff01820080", "ff01ff01820080"),
            ] {
                execute(code, input, expected);
            }
        }

        let pair_type = Type::Pair {
            first: Box::new(Type::Tree),
            rest: Box::new(Type::Tree),
        };
        let mut pair_result = ordinary.clone();
        pair_result.result = pair_type.clone();
        pair_result.body = Expr::Pair {
            first: Box::new(pair_result.body),
            rest: Box::new(argument()),
        };
        let mut unused_signature = ordinary;
        unused_signature.functions.push(Function {
            id: 2,
            name: "unused_pair_identity".into(),
            parameters: vec![Parameter {
                name: "pair".into(),
                ty: pair_type.clone(),
            }],
            result: pair_type,
            body: argument(),
        });
        for (program, pair_result) in [(pair_result, true), (unused_signature, false)] {
            program.validate().unwrap();
            assert!(program.uses_pairs());
            let previous = emit(&program, false, false, false);
            let facts = emit(&program, true, true, false);
            let requested = emit(&program, true, true, true);
            // Branch-local facts now apply inside pair-capable tables; compact
            // unary layouts still do not, so every v5 call keeps proper frames.
            assert_ne!(facts, previous);
            assert_eq!(requested, facts);
            for code in [&previous, &facts, &requested] {
                for (input, ordinary, pair) in [
                    ("80", "80", "ff8080"),
                    ("00", "00", "ff0000"),
                    ("820080", "820080", "ff820080820080"),
                    ("ff01820080", "ff01ff01820080", "ffff01ff01820080ff01820080"),
                ] {
                    execute(code, input, if pair_result { pair } else { ordinary });
                }
            }
        }
    }
}

#[cfg(test)]
mod pair_free_helper_facts {
    use super::*;
    use crate::{Environment, Function};

    fn argument() -> Expr {
        Expr::Arg { index: 0 }
    }
    fn tree_parameter() -> Parameter {
        Parameter {
            name: "tree".into(),
            ty: Type::Tree,
        }
    }
    fn pair_type() -> Type {
        Type::Pair {
            first: Box::new(Type::Tree),
            rest: Box::new(Type::Tree),
        }
    }
    fn pair(first: Expr, rest: Expr) -> Expr {
        Expr::Pair {
            first: Box::new(first),
            rest: Box::new(rest),
        }
    }
    fn helper() -> Function {
        Function {
            id: 0,
            name: "ordinary_tree_helper".into(),
            parameters: vec![tree_parameter()],
            result: Type::Tree,
            body: Expr::If {
                condition: Box::new(Expr::TreeIsPair {
                    value: Box::new(argument()),
                }),
                then_value: Box::new(Expr::Let {
                    local_id: 0,
                    ty: Type::Tree,
                    value: Box::new(Expr::TreeFirst {
                        value: Box::new(argument()),
                    }),
                    body: Box::new(Expr::TreePair {
                        first: Box::new(Expr::Local { local_id: 0 }),
                        rest: Box::new(argument()),
                    }),
                }),
                else_value: Box::new(Expr::TreeAtom {
                    value: Box::new(Expr::TreeAtomValue {
                        value: Box::new(argument()),
                    }),
                }),
            },
        }
    }
    fn program() -> Program {
        Program {
            name: "pair-free-helper-facts".into(),
            environment: Environment::Raw,
            parameters: vec![tree_parameter()],
            result: pair_type(),
            body: pair(
                Expr::Call {
                    function_id: 0,
                    arguments: vec![argument()],
                },
                argument(),
            ),
            functions: vec![helper()],
        }
    }

    #[test]
    fn helper_selection_checks_all_signatures_body_types_nodes_and_call_ids() {
        let ordinary = helper();
        assert!(pair_free_helper(&ordinary, &mut 0).unwrap());
        let mut recursive = ordinary.clone();
        recursive.body = Expr::Call {
            function_id: 0,
            arguments: vec![argument()],
        };
        assert!(pair_free_helper(&recursive, &mut 0).unwrap());
        let mut cross_call = recursive.clone();
        cross_call.body = Expr::Call {
            function_id: 1,
            arguments: vec![argument()],
        };
        assert!(!pair_free_helper(&cross_call, &mut 0).unwrap());
        let mut signature = ordinary.clone();
        signature.parameters[0].ty = Type::Tuple {
            items: vec![pair_type()],
        };
        signature.result = signature.parameters[0].ty.clone();
        signature.body = argument();
        assert!(!pair_free_helper(&signature, &mut 0).unwrap());
        let mut result = ordinary.clone();
        result.result = pair_type();
        result.body = pair(argument(), argument());
        assert!(!pair_free_helper(&result, &mut 0).unwrap());
        let pair = pair(argument(), argument());
        for first in [
            pair.clone(),
            Expr::PairFirst {
                value: Box::new(pair.clone()),
            },
            Expr::PairRest {
                value: Box::new(pair.clone()),
            },
        ] {
            let mut node = ordinary.clone();
            node.body = Expr::Sequence {
                first: Box::new(first),
                then_value: Box::new(argument()),
            };
            assert!(!pair_free_helper(&node, &mut 0).unwrap());
        }
        let mut binder = ordinary;
        binder.body = Expr::Let {
            local_id: 0,
            ty: Type::Tuple {
                items: vec![pair_type()],
            },
            value: Box::new(Expr::Tuple { items: vec![pair] }),
            body: Box::new(argument()),
        };
        assert!(!pair_free_helper(&binder, &mut 0).unwrap());
        let error = pair_free_helper(&recursive, &mut (MAX_PROJECTION_WORK - 1)).unwrap_err();
        assert!(error.is::<ProjectionBudget>(), "{error:?}");
    }

    #[test]
    fn nested_type_graphs_and_body_descriptors_share_one_optional_scan_budget() {
        fn balanced(depth: usize) -> Expr {
            if depth == 0 {
                Expr::TreeAtom {
                    value: Box::new(Expr::Atom { hex: String::new() }),
                }
            } else {
                Expr::TreePair {
                    first: Box::new(balanced(depth - 1)),
                    rest: Box::new(balanced(depth - 1)),
                }
            }
        }
        let mut source = program();
        source.body = pair(argument(), argument());
        source.functions[0].parameters = (0..64)
            .map(|index| Parameter {
                name: format!("product_{index}"),
                ty: Type::Tuple {
                    items: vec![Type::Tree; 256],
                },
            })
            .collect();
        source.functions[0].body = balanced(12);
        source.validate().unwrap();
        assert!(projection_preflight(&source));
        let error = pair_free_helper(&source.functions[0], &mut 0).unwrap_err();
        assert!(error.is::<ProjectionBudget>(), "{error:?}");
    }

    #[test]
    fn eligible_helper_facts_change_v5_code_without_compact_frames_or_entry_facts() {
        let source = program();
        source.validate().unwrap();
        let emit = |guarded, atoms, compact| {
            super::super::emit_layout_attempt(&source, true, true, guarded, atoms, compact).unwrap()
        };
        let preceding = emit(false, false, false);
        let projected = emit(true, false, false);
        let atom = emit(true, true, false);
        let all_requested = emit(true, true, true);
        assert_ne!(projected, preceding);
        assert_ne!(atom, projected);
        assert_eq!(all_requested, atom, "all v5 calls must keep proper frames");
        let mut unused_pair = source;
        unused_pair.functions.push(Function {
            id: 1,
            name: "unused_pair_identity".into(),
            parameters: vec![Parameter {
                name: "pair".into(),
                ty: pair_type(),
            }],
            result: pair_type(),
            body: argument(),
        });
        unused_pair.validate().unwrap();
        let previous =
            super::super::emit_layout_attempt(&unused_pair, true, true, false, false, false)
                .unwrap();
        let requested =
            super::super::emit_layout_attempt(&unused_pair, true, true, true, true, true).unwrap();
        assert_ne!(
            requested, previous,
            "unused Pair metadata must not disable the separate eligible helper"
        );
    }

    #[test]
    fn the_last_helpers_fact_options_never_leak_into_the_v5_entry() {
        let mut source = program();
        source.body = Expr::If {
            condition: Box::new(Expr::TreeIsPair {
                value: Box::new(argument()),
            }),
            then_value: Box::new(Expr::Let {
                local_id: 0,
                ty: Type::Tree,
                value: Box::new(Expr::TreeFirst {
                    value: Box::new(argument()),
                }),
                body: Box::new(pair(Expr::Local { local_id: 0 }, argument())),
            }),
            else_value: Box::new(pair(argument(), argument())),
        };
        source.functions.push(Function {
            id: 1,
            name: "last_unused_identity".into(),
            parameters: vec![tree_parameter()],
            result: Type::Tree,
            body: argument(),
        });
        let mut ineligible_last = source.clone();
        let last = ineligible_last.functions.last_mut().unwrap();
        last.parameters[0].ty = pair_type();
        last.result = pair_type();
        // Both unused identities lower to the same proper-frame path. Only
        // their eligibility differs; the v5 entry must ignore the last option.
        source.validate().unwrap();
        ineligible_last.validate().unwrap();
        let emit = |source: &Program| {
            super::super::emit_layout_attempt(source, true, true, true, true, true).unwrap()
        };
        assert_eq!(emit(&source), emit(&ineligible_last));
    }

    #[test]
    fn helper_scan_budget_retries_the_fresh_whole_preceding_v5_emitter() {
        let mut body = argument();
        for index in (0..16).rev() {
            body = Expr::If {
                condition: Box::new(Expr::TreeIsPair {
                    value: Box::new(Expr::Arg { index }),
                }),
                then_value: Box::new(argument()),
                else_value: Box::new(body),
            };
        }
        let parameters = (0..64)
            .map(|index| Parameter {
                name: format!("tree_{index}"),
                ty: Type::Tree,
            })
            .collect::<Vec<_>>();
        let mut source = program();
        source.body = pair(argument(), argument());
        source.functions = (0..40)
            .map(|id| Function {
                id,
                name: format!("unused_{id}"),
                parameters: parameters.clone(),
                result: Type::Tree,
                body: body.clone(),
            })
            .collect();
        source.validate().unwrap();
        assert!(projection_preflight(&source));
        let error = super::super::emit_attempt(&source, true, true, true).unwrap_err();
        assert!(error.is::<ProjectionBudget>(), "{error:?}");
        let preceding = super::super::emit_attempt(&source, true, true, false).unwrap();
        assert_eq!(super::super::emit(&source, true, true).unwrap(), preceding);
        let preceding_off = super::super::emit_attempt(&source, false, true, false).unwrap();
        assert_eq!(
            super::super::emit(&source, false, true).unwrap(),
            preceding_off
        );
    }
}
