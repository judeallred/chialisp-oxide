//! Optional, bounded inlining of already validated closed functions.
//!
//! Arguments become strict lexical bindings, including unused arguments. A
//! callee's argument and local identities never refer to the caller's scope.
use std::collections::BTreeSet;

use crate::{Expr, Function, Parameter, Program, Type};
use anyhow::Result;

const MAX_NODES: usize = 20_000;
const MAX_DEPTH: usize = 64;
const MAX_WORK: usize = 500_000;
const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_TINY_NODES: usize = 16;
const MAX_TINY_PARAMETERS: usize = 4;
const MAX_ENTRY_NODES: usize = 256;
const MAX_BINDINGS: usize = 128;

#[derive(Default)]
struct Budget {
    work: usize,
    nodes: usize,
    types: usize,
    bytes: usize,
}
impl Budget {
    fn work(&mut self, amount: usize) -> Option<()> {
        self.work = self.work.checked_add(amount)?;
        (self.work <= MAX_WORK).then_some(())
    }
    fn node(&mut self, depth: usize) -> Option<()> {
        self.work(1)?;
        self.nodes = self.nodes.checked_add(1)?;
        (depth <= MAX_DEPTH && self.nodes <= MAX_NODES).then_some(())
    }
    fn text(&mut self, value: &str) -> Option<String> {
        self.bytes = self.bytes.checked_add(value.len())?;
        (self.bytes <= MAX_BYTES).then_some(())?;
        Some(value.to_owned())
    }
    fn ty(&mut self, value: &Type) -> Option<Type> {
        let mut pending = vec![value];
        while let Some(ty) = pending.pop() {
            self.work(1)?;
            self.types = self.types.checked_add(1)?;
            (self.types <= MAX_NODES).then_some(())?;
            if let Type::Tuple { items } = ty {
                self.work(items.len())?;
                (items.len() <= MAX_NODES.saturating_sub(pending.len())).then_some(())?;
                pending.extend(items);
            }
        }
        Some(value.clone())
    }
    fn parameters(&mut self, values: &[Parameter]) -> Option<Vec<Parameter>> {
        self.work(values.len())?;
        values
            .iter()
            .map(|p| {
                Some(Parameter {
                    name: self.text(&p.name)?,
                    ty: self.ty(&p.ty)?,
                })
            })
            .collect()
    }
}

/// Bound every child-list growth before extending a borrowed traversal.
fn children<'a>(expr: &'a Expr, pending: &mut Vec<&'a Expr>, budget: &mut Budget) -> Option<()> {
    let count = match expr {
        Expr::Tuple { items }
        | Expr::Sha256 { parts: items }
        | Expr::Call {
            arguments: items, ..
        } => items.len(),
        Expr::Let { .. } | Expr::Sequence { .. } | Expr::Binary { .. } | Expr::TreePair { .. } => 2,
        Expr::If { .. } => 3,
        Expr::Field { .. }
        | Expr::Not { .. }
        | Expr::AtomIsEmpty { .. }
        | Expr::TreeAtom { .. }
        | Expr::TreeAtomValue { .. }
        | Expr::TreeIsPair { .. }
        | Expr::TreeFirst { .. }
        | Expr::TreeRest { .. } => 1,
        Expr::Arg { .. }
        | Expr::Local { .. }
        | Expr::Unit
        | Expr::Bool { .. }
        | Expr::Int { .. }
        | Expr::Atom { .. }
        | Expr::Bytes { .. }
        | Expr::Reject { .. } => 0,
    };
    budget.work(count)?;
    (count <= MAX_NODES.saturating_sub(pending.len())).then_some(())?;
    match expr {
        Expr::Tuple { items }
        | Expr::Sha256 { parts: items }
        | Expr::Call {
            arguments: items, ..
        } => pending.extend(items),
        Expr::Let { value, body, .. } => pending.extend([value.as_ref(), body.as_ref()]),
        Expr::Sequence { first, then_value } => {
            pending.extend([first.as_ref(), then_value.as_ref()])
        }
        Expr::Binary { left, right, .. } => pending.extend([left.as_ref(), right.as_ref()]),
        Expr::TreePair { first, rest } => pending.extend([first.as_ref(), rest.as_ref()]),
        Expr::If {
            condition,
            then_value,
            else_value,
        } => pending.extend([condition.as_ref(), then_value.as_ref(), else_value.as_ref()]),
        Expr::Field { value, .. }
        | Expr::Not { value }
        | Expr::AtomIsEmpty { value }
        | Expr::TreeAtom { value }
        | Expr::TreeAtomValue { value }
        | Expr::TreeIsPair { value }
        | Expr::TreeFirst { value }
        | Expr::TreeRest { value } => pending.push(value),
        Expr::Arg { .. }
        | Expr::Local { .. }
        | Expr::Unit
        | Expr::Bool { .. }
        | Expr::Int { .. }
        | Expr::Atom { .. }
        | Expr::Bytes { .. }
        | Expr::Reject { .. } => (),
    }
    Some(())
}

struct Plan {
    tiny: Vec<bool>,
    entry: Option<usize>,
}
fn plan(program: &Program, budget: &mut Budget) -> Option<Plan> {
    let mut edges = Vec::with_capacity(program.functions.len());
    let mut sizes = Vec::with_capacity(program.functions.len());
    for function in &program.functions {
        let mut pending = vec![&function.body];
        let (mut count, mut calls) = (0_usize, 0_u64);
        while let Some(expr) = pending.pop() {
            budget.work(1)?;
            count = count.checked_add(1)?;
            (count <= MAX_NODES).then_some(())?;
            if let Expr::Call { function_id, .. } = expr {
                calls |= 1_u64.checked_shl((*function_id).try_into().ok()?)?;
            }
            children(expr, &mut pending, budget)?;
        }
        edges.push(calls);
        sizes.push(count);
    }
    let mut reachable = edges.clone();
    for via in 0..reachable.len() {
        for from in 0..reachable.len() {
            budget.work(1)?;
            if reachable[from] & (1_u64 << via) != 0 {
                reachable[from] |= reachable[via];
            }
        }
    }
    let recursive = |id: usize| reachable[id] & (1_u64 << id) != 0;
    let tiny = program
        .functions
        .iter()
        .enumerate()
        .map(|(id, f)| {
            !recursive(id)
                && edges[id] == 0
                && sizes[id] <= MAX_TINY_NODES
                && f.parameters.len() <= MAX_TINY_PARAMETERS
        })
        .collect();
    // This is a structural identity-argument entry, not an SDK/name rule. Its
    // standalone callee scope can become the standalone entry scope unchanged.
    let entry = if let Expr::Call {
        function_id,
        arguments,
    } = &program.body
    {
        let f = &program.functions[*function_id];
        (!recursive(*function_id)
            && sizes[*function_id] <= MAX_ENTRY_NODES
            && arguments.len() == program.parameters.len()
            && f.parameters
                .iter()
                .zip(&program.parameters)
                .all(|(a, b)| a.ty == b.ty)
            && arguments
                .iter()
                .enumerate()
                .all(|(i, a)| matches!(a, Expr::Arg { index } if *index == i)))
        .then_some(*function_id)
    } else {
        None
    };
    Some(Plan { tiny, entry })
}

struct Fresh {
    occupied: BTreeSet<usize>,
    next: usize,
}
impl Fresh {
    fn for_body(body: &Expr, budget: &mut Budget) -> Option<Self> {
        let mut occupied = BTreeSet::new();
        let mut pending = vec![body];
        while let Some(expr) = pending.pop() {
            budget.work(1)?;
            if let Expr::Let { local_id, .. } | Expr::Local { local_id } = expr {
                occupied.insert(*local_id);
            }
            children(expr, &mut pending, budget)?;
        }
        Some(Self { occupied, next: 0 })
    }
    fn take(&mut self, budget: &mut Budget) -> Option<usize> {
        while self.occupied.contains(&self.next) {
            budget.work(1)?;
            self.next = self.next.checked_add(1)?;
        }
        budget.work(1)?;
        let id = self.next;
        self.occupied.insert(id);
        self.next = self.next.checked_add(1)?;
        Some(id)
    }
}

#[derive(Default)]
struct Names {
    arguments: Option<Vec<usize>>,
    locals: Vec<(usize, usize)>,
}
struct Rewriter<'a> {
    program: &'a Program,
    plan: &'a Plan,
    budget: &'a mut Budget,
}
impl Rewriter<'_> {
    fn body(&mut self, expr: &Expr) -> Option<Expr> {
        let mut fresh = Fresh::for_body(expr, self.budget)?;
        self.expr(expr, &mut Names::default(), &mut fresh, 1)
    }
    fn expr(
        &mut self,
        expr: &Expr,
        names: &mut Names,
        fresh: &mut Fresh,
        depth: usize,
    ) -> Option<Expr> {
        self.budget.node(depth)?;
        Some(match expr {
            Expr::Arg { index } => match &names.arguments {
                Some(args) => Expr::Local {
                    local_id: *args.get(*index)?,
                },
                None => Expr::Arg { index: *index },
            },
            Expr::Local { local_id } => Expr::Local {
                local_id: match names
                    .locals
                    .iter()
                    .rev()
                    .find_map(|(old, new)| (*old == *local_id).then_some(*new))
                {
                    Some(id) => id,
                    None if names.arguments.is_none() => *local_id,
                    None => return None,
                },
            },
            Expr::Call {
                function_id,
                arguments,
            } => {
                self.budget.work(arguments.len())?;
                let arguments = arguments
                    .iter()
                    .enumerate()
                    .map(|(index, arg)| {
                        self.expr(
                            arg,
                            names,
                            fresh,
                            depth
                                + 1
                                + if self.plan.tiny[*function_id] {
                                    index
                                } else {
                                    0
                                },
                        )
                    })
                    .collect::<Option<Vec<_>>>()?;
                if self.plan.tiny[*function_id] {
                    let function = &self.program.functions[*function_id];
                    let parameters = function
                        .parameters
                        .iter()
                        .map(|_| fresh.take(self.budget))
                        .collect::<Option<Vec<_>>>()?;
                    let mut callee = Names {
                        arguments: Some(parameters.clone()),
                        locals: vec![],
                    };
                    let mut body =
                        self.expr(&function.body, &mut callee, fresh, depth + parameters.len())?;
                    for (offset, ((local_id, parameter), value)) in parameters
                        .into_iter()
                        .zip(&function.parameters)
                        .zip(arguments)
                        .enumerate()
                        .rev()
                    {
                        self.budget.node(depth + offset)?;
                        body = Expr::Let {
                            local_id,
                            ty: self.budget.ty(&parameter.ty)?,
                            value: Box::new(value),
                            body: Box::new(body),
                        };
                    }
                    body
                } else {
                    Expr::Call {
                        function_id: *function_id,
                        arguments,
                    }
                }
            }
            Expr::Let {
                local_id,
                ty,
                value,
                body,
            } => {
                let value = self.expr(value, names, fresh, depth + 1)?;
                let id = if names.arguments.is_some() {
                    fresh.take(self.budget)?
                } else {
                    *local_id
                };
                names.locals.push((*local_id, id));
                let body = self.expr(body, names, fresh, depth + 1)?;
                names.locals.pop();
                Expr::Let {
                    local_id: id,
                    ty: self.budget.ty(ty)?,
                    value: Box::new(value),
                    body: Box::new(body),
                }
            }
            Expr::If {
                condition,
                then_value,
                else_value,
            } => Expr::If {
                condition: Box::new(self.expr(condition, names, fresh, depth + 1)?),
                then_value: Box::new(self.expr(then_value, names, fresh, depth + 1)?),
                else_value: Box::new(self.expr(else_value, names, fresh, depth + 1)?),
            },
            Expr::Binary {
                kind,
                left,
                right,
                ty,
            } => Expr::Binary {
                kind: *kind,
                left: Box::new(self.expr(left, names, fresh, depth + 1)?),
                right: Box::new(self.expr(right, names, fresh, depth + 1)?),
                ty: self.budget.ty(ty)?,
            },
            Expr::Sequence { first, then_value } => Expr::Sequence {
                first: Box::new(self.expr(first, names, fresh, depth + 1)?),
                then_value: Box::new(self.expr(then_value, names, fresh, depth + 1)?),
            },
            Expr::TreePair { first, rest } => Expr::TreePair {
                first: Box::new(self.expr(first, names, fresh, depth + 1)?),
                rest: Box::new(self.expr(rest, names, fresh, depth + 1)?),
            },
            Expr::Tuple { items } => {
                self.budget.work(items.len())?;
                Expr::Tuple {
                    items: items
                        .iter()
                        .map(|item| self.expr(item, names, fresh, depth + 1))
                        .collect::<Option<_>>()?,
                }
            }
            Expr::Sha256 { parts } => {
                self.budget.work(parts.len())?;
                Expr::Sha256 {
                    parts: parts
                        .iter()
                        .map(|item| self.expr(item, names, fresh, depth + 1))
                        .collect::<Option<_>>()?,
                }
            }
            Expr::Field { value, index } => Expr::Field {
                value: Box::new(self.expr(value, names, fresh, depth + 1)?),
                index: *index,
            },
            Expr::Not { value } => Expr::Not {
                value: Box::new(self.expr(value, names, fresh, depth + 1)?),
            },
            Expr::AtomIsEmpty { value } => Expr::AtomIsEmpty {
                value: Box::new(self.expr(value, names, fresh, depth + 1)?),
            },
            Expr::TreeAtom { value } => Expr::TreeAtom {
                value: Box::new(self.expr(value, names, fresh, depth + 1)?),
            },
            Expr::TreeAtomValue { value } => Expr::TreeAtomValue {
                value: Box::new(self.expr(value, names, fresh, depth + 1)?),
            },
            Expr::TreeIsPair { value } => Expr::TreeIsPair {
                value: Box::new(self.expr(value, names, fresh, depth + 1)?),
            },
            Expr::TreeFirst { value } => Expr::TreeFirst {
                value: Box::new(self.expr(value, names, fresh, depth + 1)?),
            },
            Expr::TreeRest { value } => Expr::TreeRest {
                value: Box::new(self.expr(value, names, fresh, depth + 1)?),
            },
            Expr::Atom { hex } => Expr::Atom {
                hex: self.budget.text(hex)?,
            },
            Expr::Bytes { hex } => Expr::Bytes {
                hex: self.budget.text(hex)?,
            },
            Expr::Int { value, ty } => Expr::Int {
                value: self.budget.text(value)?,
                ty: self.budget.ty(ty)?,
            },
            Expr::Reject { message } => Expr::Reject {
                message: self.budget.text(message)?,
            },
            Expr::Bool { value } => Expr::Bool { value: *value },
            Expr::Unit => Expr::Unit,
        })
    }
}

fn leaf(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Arg { .. }
            | Expr::Local { .. }
            | Expr::Unit
            | Expr::Bool { .. }
            | Expr::Int { .. }
            | Expr::Atom { .. }
            | Expr::Bytes { .. }
    )
}

/// Mirror optimized runtime lexical-frame and pending-operand admission. This
/// does not claim that a smaller AST guarantees a smaller serialized program.
fn frames(
    expr: &Expr,
    live: usize,
    pending: usize,
    depth: usize,
    budget: &mut Budget,
) -> Option<()> {
    budget.work(1)?;
    (depth <= MAX_DEPTH && live <= MAX_BINDINGS && pending <= MAX_BINDINGS).then_some(())?;
    match expr {
        Expr::Let {
            local_id,
            value,
            body,
            ..
        } => {
            frames(value, live, pending, depth + 1, budget)?;
            let tail = matches!(body.as_ref(), Expr::Local { local_id: id } if id == local_id);
            frames(body, live + usize::from(!tail), pending, depth + 1, budget)
        }
        Expr::Sequence { first, then_value } => {
            frames(first, live, pending, depth + 1, budget)?;
            frames(
                then_value,
                live + usize::from(!leaf(first)),
                pending,
                depth + 1,
                budget,
            )
        }
        Expr::If {
            condition,
            then_value,
            else_value,
        } => {
            frames(condition, live, pending, depth + 1, budget)?;
            frames(then_value, live, pending, depth + 1, budget)?;
            frames(else_value, live, pending, depth + 1, budget)
        }
        Expr::Tuple { items }
        | Expr::Sha256 { parts: items }
        | Expr::Call {
            arguments: items, ..
        } => operands(items.iter(), live, pending, depth, budget),
        Expr::Binary { left, right, .. } => operands(
            [left.as_ref(), right.as_ref()].into_iter(),
            live,
            pending,
            depth,
            budget,
        ),
        Expr::TreePair { first, rest } => operands(
            [first.as_ref(), rest.as_ref()].into_iter(),
            live,
            pending,
            depth,
            budget,
        ),
        Expr::TreeAtomValue { value }
            if !leaf(value) && !matches!(value.as_ref(), Expr::TreeAtom { .. }) =>
        {
            frames(value, live, pending, depth + 1, budget)?;
            (live < MAX_BINDINGS && pending < MAX_BINDINGS).then_some(())
        }
        Expr::Field { value, .. }
        | Expr::Not { value }
        | Expr::AtomIsEmpty { value }
        | Expr::TreeAtom { value }
        | Expr::TreeAtomValue { value }
        | Expr::TreeIsPair { value }
        | Expr::TreeFirst { value }
        | Expr::TreeRest { value } => frames(value, live, pending, depth + 1, budget),
        Expr::Arg { .. }
        | Expr::Local { .. }
        | Expr::Unit
        | Expr::Bool { .. }
        | Expr::Int { .. }
        | Expr::Atom { .. }
        | Expr::Bytes { .. }
        | Expr::Reject { .. } => Some(()),
    }
}
fn operands<'a>(
    items: impl Iterator<Item = &'a Expr>,
    mut live: usize,
    mut pending: usize,
    depth: usize,
    budget: &mut Budget,
) -> Option<()> {
    for item in items {
        frames(item, live, pending, depth + 1, budget)?;
        (pending < MAX_BINDINGS).then_some(())?;
        pending += 1;
        if !leaf(item) {
            (live < MAX_BINDINGS).then_some(())?;
            live += 1;
        }
    }
    Some(())
}

fn prune(program: &mut Program, budget: &mut Budget) -> Option<()> {
    let mut used = 0_u64;
    let mut pending = vec![&program.body];
    while let Some(expr) = pending.pop() {
        budget.work(1)?;
        if let Expr::Call { function_id, .. } = expr {
            let bit = 1_u64 << *function_id;
            if used & bit == 0 {
                used |= bit;
                pending.push(&program.functions[*function_id].body);
            }
        }
        children(expr, &mut pending, budget)?;
    }
    let mut remap = vec![None; program.functions.len()];
    let mut next = 0;
    for (old, id) in remap.iter_mut().enumerate() {
        if used & (1_u64 << old) != 0 {
            *id = Some(next);
            next += 1;
        }
    }
    // One bounded borrowed scan collected identities; mutate only Call IDs.
    fn calls(
        expr: &mut Expr,
        remap: &[Option<usize>],
        budget: &mut Budget,
        depth: usize,
    ) -> Option<()> {
        budget.work(1)?;
        (depth <= MAX_DEPTH).then_some(())?;
        match expr {
            Expr::Call {
                function_id,
                arguments,
            } => {
                *function_id = remap[*function_id]?;
                for argument in arguments {
                    calls(argument, remap, budget, depth + 1)?;
                }
            }
            Expr::Tuple { items } | Expr::Sha256 { parts: items } => {
                for item in items {
                    calls(item, remap, budget, depth + 1)?;
                }
            }
            Expr::Let { value, body, .. } => {
                calls(value, remap, budget, depth + 1)?;
                calls(body, remap, budget, depth + 1)?;
            }
            Expr::Sequence { first, then_value } => {
                calls(first, remap, budget, depth + 1)?;
                calls(then_value, remap, budget, depth + 1)?;
            }
            Expr::Binary { left, right, .. } => {
                calls(left, remap, budget, depth + 1)?;
                calls(right, remap, budget, depth + 1)?;
            }
            Expr::TreePair { first, rest } => {
                calls(first, remap, budget, depth + 1)?;
                calls(rest, remap, budget, depth + 1)?;
            }
            Expr::If {
                condition,
                then_value,
                else_value,
            } => {
                calls(condition, remap, budget, depth + 1)?;
                calls(then_value, remap, budget, depth + 1)?;
                calls(else_value, remap, budget, depth + 1)?;
            }
            Expr::Field { value, .. }
            | Expr::Not { value }
            | Expr::AtomIsEmpty { value }
            | Expr::TreeAtom { value }
            | Expr::TreeAtomValue { value }
            | Expr::TreeIsPair { value }
            | Expr::TreeFirst { value }
            | Expr::TreeRest { value } => calls(value, remap, budget, depth + 1)?,
            Expr::Arg { .. }
            | Expr::Local { .. }
            | Expr::Unit
            | Expr::Bool { .. }
            | Expr::Int { .. }
            | Expr::Atom { .. }
            | Expr::Bytes { .. }
            | Expr::Reject { .. } => (),
        }
        Some(())
    }
    calls(&mut program.body, &remap, budget, 1)?;
    let old = std::mem::take(&mut program.functions);
    for (mut function, id) in old.into_iter().zip(remap.iter().copied()) {
        if let Some(id) = id {
            function.id = id;
            calls(&mut function.body, &remap, budget, 1)?;
            program.functions.push(function);
        }
    }
    Some(())
}

/// `None` means optional expansion cannot establish its compilation budgets.
/// Invalid original input is always an error, including unreachable functions.
pub(super) fn optimize(program: &Program) -> Result<Option<Program>> {
    program.validate()?;
    if program.functions.is_empty() || !super::budget::allows_recursive_refinement(program) {
        return Ok(None);
    }
    let mut budget = Budget::default();
    let Some(plan) = plan(program, &mut budget) else {
        return Ok(None);
    };
    let mut rewriter = Rewriter {
        program,
        plan: &plan,
        budget: &mut budget,
    };
    let attempted = (|| {
        let source = plan
            .entry
            .map_or(&program.body, |id| &program.functions[id].body);
        let body = rewriter.body(source)?;
        let mut functions = Vec::with_capacity(program.functions.len());
        for function in &program.functions {
            functions.push(Function {
                id: function.id,
                name: rewriter.budget.text(&function.name)?,
                parameters: rewriter.budget.parameters(&function.parameters)?,
                result: rewriter.budget.ty(&function.result)?,
                body: rewriter.body(&function.body)?,
            });
        }
        Some(Program {
            name: rewriter.budget.text(&program.name)?,
            parameters: rewriter.budget.parameters(&program.parameters)?,
            result: rewriter.budget.ty(&program.result)?,
            body,
            environment: program.environment,
            functions,
        })
    })();
    let Some(mut result) = attempted else {
        return Ok(None);
    };
    // Every rewritten body is checked before any table entry can disappear.
    if result.validate().is_err() {
        return Ok(None);
    }
    let safe = frames(&result.body, 0, 0, 1, &mut budget).and_then(|_| {
        for function in &result.functions {
            frames(&function.body, 0, 0, 1, &mut budget)?;
        }
        Some(())
    });
    if safe.is_none() || prune(&mut result, &mut budget).is_none() {
        return Ok(None);
    }
    // Runtime evaluation order is not an independent field in the current IR.
    // Public optimize callers may serialize and recompile this result, losing
    // compile's original-mode pin if every runtime node/table disappeared.
    if program.uses_runtime() && !result.uses_runtime() {
        return Ok(None);
    }
    result.validate()?;
    Ok(Some(result))
}
