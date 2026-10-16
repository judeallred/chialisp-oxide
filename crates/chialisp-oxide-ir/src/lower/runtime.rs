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

#[derive(Clone)]
struct Scope<'a> {
    parameters: &'a [Parameter],
    slots: Vec<Binding>,
    locals: Vec<(usize, Type)>,
}
impl<'a> Scope<'a> {
    fn new(parameters: &'a [Parameter]) -> Self {
        Self {
            parameters,
            slots: (0..parameters.len()).map(Binding::Argument).collect(),
            locals: vec![],
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
}

type Continuation<'a, 'p, 'c> =
    dyn FnMut(&mut Runtime<'a, 'p>, &Scope<'_>, &[Binding]) -> Result<LirId> + 'c;

impl<'a, 'p> Runtime<'a, 'p> {
    fn charge(&mut self, amount: usize) -> Result<()> {
        self.work = self.work.saturating_add(amount);
        ensure!(
            self.work <= 500_000,
            "runtime lowering work budget exceeded"
        );
        Ok(())
    }

    fn extend_scope<'s>(&mut self, scope: &Scope<'s>, binding: Binding) -> Result<Scope<'s>> {
        // Check before recursive operand lowering or cloning the frame. A
        // shallow AST can otherwise accumulate thousands of live temporaries
        // through nested, wide tuples before the serialized-size check runs.
        ensure!(
            scope.slots.len().saturating_sub(scope.parameters.len()) < 128,
            "runtime frame binding budget exceeded"
        );
        self.charge(1 + scope.slots.len())?;
        let mut types = scope.locals.iter().map(|(_, ty)| ty).collect::<Vec<_>>();
        while let Some(ty) = types.pop() {
            self.charge(1)?;
            if let Type::Tuple { items } = ty {
                types.extend(items);
            }
        }
        Ok(scope.extended(binding))
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
        bindings: &mut Vec<Binding>,
        finish: &mut Continuation<'a, 'p, '_>,
    ) -> Result<LirId> {
        self.charge(1)?;
        if let Some((first, remaining)) = expressions.split_first() {
            let value = self.expression(first, scope)?;
            let binding = Binding::Temporary(self.next_temporary);
            self.next_temporary += 1;
            let next = self.extend_scope(scope, binding)?;
            bindings.push(binding);
            let body = self.values(remaining, &next, bindings, finish)?;
            bindings.pop();
            Ok(self.continuation(value, body))
        } else {
            finish(self, scope, bindings)
        }
    }

    fn expression(&mut self, expression: &Expr, scope: &Scope<'_>) -> Result<LirId> {
        self.charge(1)?;
        Ok(match expression {
            Expr::Atom { hex } | Expr::Bytes { hex } => self.lower.bytes(hex::decode(hex)?),
            Expr::Unit => self.lower.nil(),
            Expr::Bool { value } => self.lower.int(i64::from(*value)),
            Expr::Int { value, .. } => self.lower.number(value.parse()?),
            Expr::Arg { index } => self.slot(scope, Binding::Argument(*index))?,
            Expr::Local { local_id } => self.slot(scope, Binding::Local(*local_id))?,
            Expr::Reject { .. } => self.lower.reject(),
            Expr::TreeAtom { value } => self.expression(value, scope)?,
            Expr::Let {
                local_id,
                ty,
                value,
                body,
            } => {
                let value = self.expression(value, scope)?;
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
                        .map(|binding| this.slot(scope, *binding))
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
                        .map(|binding| this.slot(scope, *binding))
                        .collect::<Result<Vec<_>>>()?;
                    Ok(this.lower.list(&values))
                })?
            }
            Expr::TreePair { first, rest } => self.values(
                &[first.as_ref(), rest.as_ref()],
                scope,
                &mut vec![],
                &mut |this, scope, bindings| {
                    let first = this.slot(scope, bindings[0])?;
                    let rest = this.slot(scope, bindings[1])?;
                    Ok(this.lower.alloc(Lir::Cons(first, rest)))
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
                    let left = this.slot(scope, bindings[0])?;
                    let right = this.slot(scope, bindings[1])?;
                    this.lower.binary(*kind, left, right, ty)
                },
            )?,
            Expr::Field { value, index } => {
                let ty =
                    self.lower
                        .program
                        .type_of_scoped(value, scope.parameters, &scope.locals)?;
                self.values(
                    &[value.as_ref()],
                    scope,
                    &mut vec![],
                    &mut |this, scope, bindings| {
                        let value = this.slot(scope, bindings[0])?;
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
            Expr::Not { value }
            | Expr::TreeIsPair { value }
            | Expr::TreeFirst { value }
            | Expr::TreeRest { value }
            | Expr::TreeAtomValue { value } => self.values(
                &[value.as_ref()],
                scope,
                &mut vec![],
                &mut |this, scope, bindings| {
                    let value = this.slot(scope, bindings[0])?;
                    Ok(match expression {
                        Expr::Not { .. } => this.lower.alloc(Lir::Not(value)),
                        Expr::TreeIsPair { .. } => this.lower.alloc(Lir::Listp(value, true)),
                        Expr::TreeFirst { .. } => this.lower.alloc(Lir::First(value)),
                        Expr::TreeRest { .. } => this.lower.alloc(Lir::Rest(value)),
                        Expr::TreeAtomValue { .. } => this.lower.scalar(value, value),
                        _ => unreachable!("matched unary expression"),
                    })
                },
            )?,
        })
    }
}

pub(super) fn bootstrap(lower: &mut Lower<'_>) -> Result<LirId> {
    let program = lower.program;
    let mut runtime = Runtime {
        lower,
        next_temporary: 0,
        work: 0,
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
