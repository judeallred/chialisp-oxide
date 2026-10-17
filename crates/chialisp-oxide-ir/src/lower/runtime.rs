//! Closed runtime functions and strict, once-only evaluation.
//!
//! Every frame is `(compiler_function_table . slots)`. Entry arguments occupy
//! initial slots. A binding applies a quoted continuation to a frame with one
//! new slot; slot identities, rather than cached paths, survive that extension.
//! Calls create fresh slots containing only their arguments. No input tree can
//! select a callee or supply executable code through this calling convention.
use super::Lower;
use crate::{Expr, Parameter, Type};
use anyhow::{Result, anyhow, ensure};
use num_bigint::BigUint;
use rue_lir::{Lir, LirId, first_path, rest_path};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Binding {
    Argument(usize),
    Local(usize),
    Temporary(usize),
}

#[derive(Clone, Copy)]
enum Value {
    Slot(Binding),
    Literal(LirId),
}

#[derive(Clone)]
struct Scope<'a> {
    parameters: &'a [Parameter],
    slots: Vec<Binding>,
    locals: Vec<(usize, Type)>,
    // Total aliases retain a resolved value identity, never a local name that
    // could be rebound in another lexical scope. Slot paths resolve only when
    // consumed, after any intervening physical frames have been added.
    aliases: Vec<(usize, Value)>,
}
impl<'a> Scope<'a> {
    fn new(parameters: &'a [Parameter]) -> Self {
        Self {
            parameters,
            slots: (0..parameters.len()).map(Binding::Argument).collect(),
            locals: vec![],
            aliases: vec![],
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
    pending_operands: usize,
    expression_depth: usize,
}

type Continuation<'a, 'p, 'c> =
    dyn FnMut(&mut Runtime<'a, 'p>, &Scope<'_>, &[Value]) -> Result<LirId> + 'c;

impl<'a, 'p> Runtime<'a, 'p> {
    fn charge(&mut self, amount: usize) -> Result<()> {
        self.work = self.work.saturating_add(amount);
        ensure!(
            self.work <= 500_000,
            "runtime lowering work budget exceeded"
        );
        Ok(())
    }

    fn charge_type_clone(&mut self, ty: &Type) -> Result<()> {
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            self.charge(1)?;
            if let Type::Tuple { items } = ty {
                self.charge(items.len())?;
                pending.extend(items);
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
            "runtime frame binding budget exceeded"
        );
        self.charge(1 + scope.slots.len() + scope.aliases.len())?;
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
            .unwrap_or(Value::Slot(Binding::Local(local_id)))
    }
    fn table(&mut self) -> LirId {
        self.lower.alloc(Lir::Path(2u8.into()))
    }

    fn slot(&mut self, scope: &Scope<'_>, binding: Binding) -> Result<LirId> {
        let index = scope
            .slots
            .iter()
            .position(|slot| *slot == binding)
            .ok_or_else(|| anyhow!("runtime binding outside its lexical scope"))?;
        Ok(self.list_slot(3, index))
    }

    fn value(&mut self, scope: &Scope<'_>, value: Value) -> Result<LirId> {
        match value {
            Value::Slot(binding) => self.slot(scope, binding),
            Value::Literal(value) => Ok(value),
        }
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
            Expr::Arg { index } => Value::Slot(Binding::Argument(*index)),
            Expr::Local { local_id } => Self::local(scope, *local_id),
            _ => Value::Literal(self.expression(expression, scope)?),
        })
    }

    fn operand(&mut self) -> Result<()> {
        if self.optimize {
            // Forwarded values do not extend physical frames. Bound pending
            // descriptors independently, before growing their vectors, so wide
            // nested total operands cannot evade the old compilation bound.
            ensure!(
                self.pending_operands < 128,
                "runtime frame operand budget exceeded"
            );
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
            ensure!(
                self.expression_depth < 192,
                "runtime expression depth budget exceeded"
            );
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
            Expr::Arg { index } => self.slot(scope, Binding::Argument(*index))?,
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
                let value = self.expression(value, scope)?;
                self.charge_type_clone(ty)?;
                let mut next = self.extend_scope(scope, Binding::Local(*local_id))?;
                next.locals.push((*local_id, ty.clone()));
                let body = self.expression(body, &next)?;
                self.continuation(value, body)
            }
            Expr::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.expression(condition, scope)?;
                let yes = self.expression(then_value, scope)?;
                let no = self.expression(else_value, scope)?;
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
                    let arguments = this.lower.list(&values);
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
            Expr::TreePair { first, rest } => self.values(
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
            | Expr::TreeAtomValue { value } => {
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
            Expr::TreeFirst { .. } => self.lower.alloc(Lir::First(value)),
            Expr::TreeRest { .. } => self.lower.alloc(Lir::Rest(value)),
            Expr::TreeAtomValue { .. } => self.lower.scalar(value, value),
            _ => unreachable!("matched unary expression"),
        }
    }
}

pub(super) fn bootstrap(lower: &mut Lower<'_>, optimize: bool) -> Result<LirId> {
    let program = lower.program;
    let mut runtime = Runtime {
        lower,
        next_temporary: 0,
        work: 0,
        optimize,
        pending_operands: 0,
        expression_depth: 0,
    };
    let mut functions = Vec::with_capacity(program.functions.len());
    for function in &program.functions {
        let body = runtime.expression(&function.body, &Scope::new(&function.parameters))?;
        functions.push(runtime.lower.alloc(Lir::Quote(body)));
    }
    let table = runtime.lower.list(&functions);
    let body = runtime.expression(&program.body, &Scope::new(&program.parameters))?;
    let body = runtime.lower.alloc(Lir::Quote(body));
    let arguments = runtime.lower.list(&runtime.lower.args.clone());
    let frame = runtime.lower.alloc(Lir::Cons(table, arguments));
    Ok(runtime.lower.alloc(Lir::Run(body, frame)))
}
