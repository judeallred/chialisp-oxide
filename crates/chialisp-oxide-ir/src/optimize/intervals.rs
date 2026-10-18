//! Branch-local bounds for otherwise strict, checked arithmetic.
//!
//! The only rewrite removes a discarded compiler-style checked Add/Sub tuple
//! when every operand and the operation are proven total. Its repeated result
//! stays checked; no ABI decoder or arithmetic operation becomes unchecked.
//! Facts apply only to immutable values after successful evaluation. Calls and
//! raw-tree operations have no totality summaries, and function scopes start
//! fresh. Budget exhaustion skips this optional pass without rejecting input.
use crate::{BinaryOp, Expr, Parameter, Program, Type, validate::bounds};
use anyhow::Result;
use num_bigint::BigInt;
use std::collections::BTreeSet;

const MAX_NODES: usize = 20_000;
const MAX_WORK: usize = 500_000;
const MAX_BYTES: usize = 4 * 1024 * 1024;
#[derive(Default)]
struct Budget {
    work: usize,
    bytes: usize,
}
impl Budget {
    fn work(&mut self, n: usize) -> Option<()> {
        self.work = self.work.checked_add(n)?;
        (self.work <= MAX_WORK).then_some(())
    }
    fn text(&mut self, s: &str) -> Option<()> {
        self.work(s.len())?;
        self.bytes = self.bytes.checked_add(s.len())?;
        (self.bytes <= MAX_BYTES).then_some(())
    }
    fn ty(&mut self, ty: &Type) -> Option<()> {
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            self.work(1)?;
            if let Type::Tuple { items } = ty {
                self.work(items.len())?;
                pending.extend(items);
            }
        }
        Some(())
    }
}
fn children<'a>(e: &'a Expr, pending: &mut Vec<&'a Expr>, budget: &mut Budget) -> Option<()> {
    let before = pending.len();
    let n = match e {
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
        _ => 0,
    };
    budget.work(n)?;
    (before.checked_add(n)? <= MAX_NODES).then_some(())?;
    match e {
        Expr::Tuple { items }
        | Expr::Sha256 { parts: items }
        | Expr::Call {
            arguments: items, ..
        } => pending.extend(items),
        Expr::Let { value, body, .. }
        | Expr::Sequence {
            first: value,
            then_value: body,
        } => pending.extend([value.as_ref(), body.as_ref()]),
        Expr::Binary { left, right, .. }
        | Expr::TreePair {
            first: left,
            rest: right,
        } => pending.extend([left.as_ref(), right.as_ref()]),
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
        _ => (),
    }
    Some(())
}
fn preflight(p: &Program, b: &mut Budget) -> Option<()> {
    b.text(&p.name)?;
    b.ty(&p.result)?;
    let mut pending = vec![&p.body];
    for parameter in &p.parameters {
        b.text(&parameter.name)?;
        b.ty(&parameter.ty)?;
    }
    for f in &p.functions {
        b.work(1)?;
        b.text(&f.name)?;
        b.ty(&f.result)?;
        for parameter in &f.parameters {
            b.text(&parameter.name)?;
            b.ty(&parameter.ty)?;
        }
        pending.push(&f.body);
    }
    while let Some(e) = pending.pop() {
        b.work(1)?;
        match e {
            Expr::Int { value, ty } => {
                b.text(value)?;
                b.ty(ty)?;
            }
            Expr::Bytes { hex } | Expr::Atom { hex } => b.text(hex)?,
            Expr::Reject { message } => b.text(message)?,
            Expr::Let { ty, .. } | Expr::Binary { ty, .. } => b.ty(ty)?,
            _ => (),
        }
        children(e, &mut pending, b)?;
    }
    Some(())
}
#[derive(Clone)]
struct Interval {
    lo: BigInt,
    hi: BigInt,
}
impl Interval {
    fn full(ty: &Type, b: &mut Budget) -> Option<Option<Self>> {
        b.work(2)?;
        Some(if matches!(ty, Type::Int { .. }) {
            let (lo, hi) = bounds(ty).ok()?;
            Some(Self { lo, hi })
        } else {
            None
        })
    }
    fn join(a: &Self, c: &Self, b: &mut Budget) -> Option<Self> {
        b.work(4)?;
        Some(Self {
            lo: a.lo.clone().min(c.lo.clone()),
            hi: a.hi.clone().max(c.hi.clone()),
        })
    }
}
#[derive(Clone)]
struct Context {
    values: Vec<(usize, Option<Interval>)>,
    arguments: Vec<usize>,
    locals: Vec<(usize, usize)>,
    reachable: bool,
}
impl Context {
    fn new(parameters: &[Parameter], b: &mut Budget) -> Option<Self> {
        b.work(parameters.len())?;
        let mut values = Vec::with_capacity(parameters.len());
        for (i, p) in parameters.iter().enumerate() {
            values.push((i, Interval::full(&p.ty, b)?));
        }
        Some(Self {
            values,
            arguments: (0..parameters.len()).collect(),
            locals: vec![],
            reachable: true,
        })
    }
    fn copy(&self, b: &mut Budget) -> Option<Self> {
        b.work(1 + self.values.len() * 4 + self.locals.len() + self.arguments.len())?;
        Some(self.clone())
    }
    fn key(&self, e: &Expr, b: &mut Budget) -> Option<Option<usize>> {
        b.work(1 + self.locals.len())?;
        Some(match e {
            Expr::Arg { index } => Some(*self.arguments.get(*index)?),
            Expr::Local { local_id } => {
                Some(self.locals.iter().rev().find(|(id, _)| id == local_id)?.1)
            }
            _ => None,
        })
    }
    fn interval(&self, key: usize, b: &mut Budget) -> Option<Option<Interval>> {
        b.work(2 + self.values.len())?;
        Some(self.values.iter().find(|(id, _)| *id == key)?.1.clone())
    }
    fn join(&mut self, a: &Self, c: &Self, b: &mut Budget) -> Option<()> {
        if !a.reachable {
            *self = c.copy(b)?;
            return Some(());
        }
        if !c.reachable {
            *self = a.copy(b)?;
            return Some(());
        }
        // Branches may refine values, but cannot change the active lexical
        // frame. Fail closed before zip rather than silently truncating it.
        b.work(1 + self.arguments.len() * 2 + self.locals.len() * 2)?;
        (self.values.len() == a.values.len()
            && self.values.len() == c.values.len()
            && self.arguments == a.arguments
            && self.arguments == c.arguments
            && self.locals == a.locals
            && self.locals == c.locals)
            .then_some(())?;
        b.work(self.values.len() * 3)?;
        for ((id, value), ((aid, av), (cid, cv))) in
            self.values.iter_mut().zip(a.values.iter().zip(&c.values))
        {
            (*id == *aid && *id == *cid).then_some(())?;
            *value = match (av, cv) {
                (Some(a), Some(c)) => Some(Interval::join(a, c, b)?),
                _ => None,
            };
        }
        self.reachable = true;
        Some(())
    }
}
struct Info {
    interval: Option<Interval>,
    total: bool,
}
impl Info {
    fn unknown() -> Self {
        Self {
            interval: None,
            total: false,
        }
    }
}
struct Analysis<'a> {
    budget: &'a mut Budget,
    // Borrowed source nodes stay at fixed addresses throughout analysis and
    // application. IDs are never dereferenced or reused for copied nodes.
    removals: BTreeSet<*const Expr>,
    next: usize,
    strict: bool,
}
impl Analysis<'_> {
    fn literal(&mut self, e: &Expr) -> Option<Option<BigInt>> {
        self.budget.work(1)?;
        Some(if let Expr::Int { value, .. } = e {
            self.budget.work(value.len())?;
            Some(value.parse().ok()?)
        } else {
            None
        })
    }
    fn guard(&mut self, e: &Expr, yes: bool, c: &mut Context) -> Option<()> {
        self.budget.work(1)?;
        if !c.reachable {
            return Some(());
        }
        if let Expr::Not { value } = e {
            return self.guard(value, !yes, c);
        }
        let Expr::Binary {
            kind,
            left,
            right,
            ty,
        } = e
        else {
            return Some(());
        };
        if *ty == Type::Bool && matches!(kind, BinaryOp::Eq | BinaryOp::Ne) {
            let (pred, value) = match (left.as_ref(), right.as_ref()) {
                (Expr::Bool { value }, pred) | (pred, Expr::Bool { value }) => (pred, *value),
                _ => return Some(()),
            };
            return self.guard(
                pred,
                if *kind == BinaryOp::Eq {
                    yes == value
                } else {
                    yes != value
                },
                c,
            );
        }
        if !matches!(ty, Type::Int { .. }) {
            return Some(());
        }
        let (key, n, kind) =
            if let (Some(key), Some(n)) = (c.key(left, self.budget)?, self.literal(right)?) {
                (key, n, *kind)
            } else if let (Some(key), Some(n)) = (c.key(right, self.budget)?, self.literal(left)?) {
                (
                    key,
                    n,
                    match kind {
                        BinaryOp::Lt => BinaryOp::Gt,
                        BinaryOp::Le => BinaryOp::Ge,
                        BinaryOp::Gt => BinaryOp::Lt,
                        BinaryOp::Ge => BinaryOp::Le,
                        other => *other,
                    },
                )
            } else {
                return Some(());
            };
        self.budget.work(8 + c.values.len())?;
        let Some((_, Some(value))) = c.values.iter_mut().find(|(id, _)| *id == key) else {
            return Some(());
        };
        match (kind, yes) {
            (BinaryOp::Lt, true) | (BinaryOp::Ge, false) => value.hi = value.hi.clone().min(&n - 1),
            (BinaryOp::Le, true) | (BinaryOp::Gt, false) => value.hi = value.hi.clone().min(n),
            (BinaryOp::Gt, true) | (BinaryOp::Le, false) => value.lo = value.lo.clone().max(&n + 1),
            (BinaryOp::Ge, true) | (BinaryOp::Lt, false) => value.lo = value.lo.clone().max(n),
            (BinaryOp::Eq, true) | (BinaryOp::Ne, false) => {
                value.lo = value.lo.clone().max(n.clone());
                value.hi = value.hi.clone().min(n);
            }
            (BinaryOp::Eq, false) | (BinaryOp::Ne, true) => {
                if value.lo == n {
                    value.lo += 1;
                }
                if value.hi == n {
                    value.hi -= 1;
                }
            }
            _ => (),
        }
        if value.lo > value.hi {
            c.reachable = false;
        }
        Some(())
    }
    fn expression(&mut self, e: &Expr, c: &mut Context) -> Option<Info> {
        self.budget.work(1)?;
        if !c.reachable {
            return Some(Info::unknown());
        }
        let out = match e {
            Expr::Arg { .. } | Expr::Local { .. } => {
                let key = c.key(e, self.budget)??;
                Info {
                    interval: c.interval(key, self.budget)?,
                    total: true,
                }
            }
            Expr::Int { .. } => {
                let n = self.literal(e)??;
                self.budget.work(1)?;
                Info {
                    interval: Some(Interval {
                        lo: n.clone(),
                        hi: n,
                    }),
                    total: true,
                }
            }
            Expr::Unit | Expr::Bool { .. } | Expr::Atom { .. } | Expr::Bytes { .. } => Info {
                interval: None,
                total: true,
            },
            Expr::Reject { .. } => {
                c.reachable = false;
                Info::unknown()
            }
            Expr::Let {
                local_id,
                ty,
                value,
                body,
            } => {
                let info = self.expression(value, c)?;
                let alias = c.key(value, self.budget)?;
                let key = if let Some(key) = alias {
                    key
                } else {
                    self.budget.work(3)?;
                    let key = self.next;
                    self.next = self.next.checked_add(1)?;
                    let interval = if info.interval.is_some() {
                        info.interval.clone()
                    } else {
                        Interval::full(ty, self.budget)?
                    };
                    c.values.push((key, interval));
                    key
                };
                self.budget.work(1)?;
                c.locals.push((*local_id, key));
                let output = self.expression(body, c)?;
                c.locals.pop();
                if alias.is_none() {
                    c.values.pop();
                }
                Info {
                    interval: output.interval,
                    total: info.total && output.total,
                }
            }
            Expr::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition_info = self.expression(condition, c)?;
                let mut a = c.copy(self.budget)?;
                let mut z = c.copy(self.budget)?;
                self.guard(condition, true, &mut a)?;
                self.guard(condition, false, &mut z)?;
                let ai = self.expression(then_value, &mut a)?;
                let zi = self.expression(else_value, &mut z)?;
                let interval = if !a.reachable {
                    zi.interval.clone()
                } else if !z.reachable {
                    ai.interval.clone()
                } else {
                    match (&ai.interval, &zi.interval) {
                        (Some(a), Some(z)) => Some(Interval::join(a, z, self.budget)?),
                        _ => None,
                    }
                };
                c.join(&a, &z, self.budget)?;
                Info {
                    interval,
                    total: condition_info.total && ai.total && zi.total,
                }
            }
            Expr::Sequence { first, then_value } => {
                let mut prior = c.copy(self.budget)?;
                let info = self.expression(first, &mut prior)?;
                // Only runtime lowering guarantees source-left-first here.
                if self.strict {
                    *c = prior;
                }
                if info.total
                    && let Some(op) = discarded(first)
                    && contains(then_value, op, self.budget)?
                {
                    self.budget.work(1)?;
                    self.removals.insert(e as *const Expr);
                }
                let output = self.expression(then_value, c)?;
                Info {
                    interval: output.interval,
                    total: info.total && output.total,
                }
            }
            Expr::Binary {
                kind,
                left,
                right,
                ty,
            } => {
                // No sibling facts: legacy opcode order is not uniformly LTR.
                let mut a = c.copy(self.budget)?;
                let mut z = c.copy(self.budget)?;
                let ai = self.expression(left, &mut a)?;
                let zi = self.expression(right, &mut z)?;
                match kind {
                    BinaryOp::Add | BinaryOp::Sub if matches!(ty, Type::Int { .. }) => {
                        let mut total = false;
                        let mut interval = None;
                        if let (Some(a), Some(z)) = (&ai.interval, &zi.interval) {
                            self.budget.work(8)?;
                            let (lo, hi) = if *kind == BinaryOp::Add {
                                (&a.lo + &z.lo, &a.hi + &z.hi)
                            } else {
                                (&a.lo - &z.hi, &a.hi - &z.lo)
                            };
                            let (min, max) = bounds(ty).ok()?;
                            if lo >= min && hi <= max {
                                interval = Some(Interval { lo, hi });
                                total = ai.total && zi.total;
                            }
                        }
                        Info { interval, total }
                    }
                    BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge => Info {
                        interval: None,
                        total: ai.total && zi.total,
                    },
                    _ => Info::unknown(),
                }
            }
            Expr::Tuple { items } => {
                let mut total = true;
                for item in items {
                    let mut copy = c.copy(self.budget)?;
                    total &= self.expression(item, &mut copy)?.total;
                }
                Info {
                    interval: None,
                    total,
                }
            }
            Expr::Not { value } => {
                let info = self.expression(value, c)?;
                Info {
                    interval: None,
                    total: info.total,
                }
            }
            // Traverse conservatively so nested branches can be visited, but
            // never call a hash, call, field or raw-tree operation total.
            _ => {
                let mut pending = vec![];
                children(e, &mut pending, self.budget)?;
                for child in pending {
                    let mut copy = c.copy(self.budget)?;
                    self.expression(child, &mut copy)?;
                }
                Info::unknown()
            }
        };
        Some(out)
    }
}
fn discarded(e: &Expr) -> Option<&Expr> {
    let Expr::Tuple { items } = e else {
        return None;
    };
    let [op, Expr::Bool { value: false }] = items.as_slice() else {
        return None;
    };
    matches!(
        op,
        Expr::Binary {
            kind: BinaryOp::Add | BinaryOp::Sub,
            ty: Type::Int { .. },
            ..
        }
    )
    .then_some(op)
}
fn same(a: &Expr, z: &Expr, b: &mut Budget) -> Option<bool> {
    b.work(1)?;
    Some(match (a, z) {
        (Expr::Arg { index: a }, Expr::Arg { index: z }) => a == z,
        (Expr::Local { local_id: a }, Expr::Local { local_id: z }) => a == z,
        (Expr::Int { value: a, ty: at }, Expr::Int { value: z, ty: zt }) => {
            b.work(a.len() + z.len())?;
            a == z && at == zt
        }
        (
            Expr::Binary {
                kind: a,
                left: al,
                right: ar,
                ty: at,
            },
            Expr::Binary {
                kind: z,
                left: zl,
                right: zr,
                ty: zt,
            },
        ) if matches!(a, BinaryOp::Add | BinaryOp::Sub) => {
            a == z && at == zt && same(al, zl, b)? && same(ar, zr, b)?
        }
        _ => false,
    })
}
fn contains(e: &Expr, op: &Expr, b: &mut Budget) -> Option<bool> {
    let mut pending = vec![e];
    while let Some(e) = pending.pop() {
        if same(e, op, b)? {
            return Some(true);
        }
        children(e, &mut pending, b)?;
    }
    Some(false)
}
fn apply(source: &Expr, out: &mut Expr, remove: &BTreeSet<*const Expr>) {
    if remove.contains(&(source as *const Expr)) {
        let Expr::Sequence {
            then_value: original,
            ..
        } = source
        else {
            unreachable!()
        };
        let Expr::Sequence { then_value, .. } = std::mem::replace(out, Expr::Unit) else {
            unreachable!()
        };
        *out = *then_value;
        apply(original, out, remove);
        return;
    }
    match (source, out) {
        (Expr::Tuple { items: a }, Expr::Tuple { items: z })
        | (Expr::Sha256 { parts: a }, Expr::Sha256 { parts: z })
        | (Expr::Call { arguments: a, .. }, Expr::Call { arguments: z, .. }) => {
            for (a, z) in a.iter().zip(z) {
                apply(a, z, remove);
            }
        }
        (
            Expr::Let {
                value: a, body: b, ..
            },
            Expr::Let {
                value: c, body: d, ..
            },
        )
        | (
            Expr::Sequence {
                first: a,
                then_value: b,
            },
            Expr::Sequence {
                first: c,
                then_value: d,
            },
        )
        | (
            Expr::Binary {
                left: a, right: b, ..
            },
            Expr::Binary {
                left: c, right: d, ..
            },
        )
        | (Expr::TreePair { first: a, rest: b }, Expr::TreePair { first: c, rest: d }) => {
            apply(a, c, remove);
            apply(b, d, remove);
        }
        (
            Expr::If {
                condition: a,
                then_value: b,
                else_value: c,
            },
            Expr::If {
                condition: d,
                then_value: e,
                else_value: f,
            },
        ) => {
            apply(a, d, remove);
            apply(b, e, remove);
            apply(c, f, remove);
        }
        (Expr::Field { value: a, .. }, Expr::Field { value: z, .. })
        | (Expr::Not { value: a }, Expr::Not { value: z })
        | (Expr::AtomIsEmpty { value: a }, Expr::AtomIsEmpty { value: z })
        | (Expr::TreeAtom { value: a }, Expr::TreeAtom { value: z })
        | (Expr::TreeAtomValue { value: a }, Expr::TreeAtomValue { value: z })
        | (Expr::TreeIsPair { value: a }, Expr::TreeIsPair { value: z })
        | (Expr::TreeFirst { value: a }, Expr::TreeFirst { value: z })
        | (Expr::TreeRest { value: a }, Expr::TreeRest { value: z }) => apply(a, z, remove),
        _ => (),
    }
}

pub(super) fn optimize(program: &Program) -> Result<Option<Program>> {
    program.validate()?;
    if !super::budget::allows_recursive_refinement(program) {
        return Ok(None);
    }
    let mut budget = Budget::default();
    if preflight(program, &mut budget).is_none() {
        return Ok(None);
    }
    let mut analysis = Analysis {
        budget: &mut budget,
        removals: BTreeSet::new(),
        next: 0,
        strict: program.uses_runtime(),
    };
    let attempted = (|| {
        let mut context = Context::new(&program.parameters, analysis.budget)?;
        analysis.next = program.parameters.len();
        analysis.expression(&program.body, &mut context)?;
        for f in &program.functions {
            let mut context = Context::new(&f.parameters, analysis.budget)?;
            analysis.next = f.parameters.len();
            analysis.expression(&f.body, &mut context)?;
        }
        Some(())
    })();
    if attempted.is_none() || analysis.removals.is_empty() {
        return Ok(None);
    }
    // Preflight charged every source/type/string before this sole whole copy.
    let mut result = program.clone();
    apply(&program.body, &mut result.body, &analysis.removals);
    for (a, z) in program.functions.iter().zip(&mut result.functions) {
        apply(&a.body, &mut z.body, &analysis.removals);
    }
    result.validate()?;
    Ok(Some(result))
}
