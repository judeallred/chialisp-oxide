use crate::{BinaryOp, Expr, Program, Type, validate::bounds};
use anyhow::Result;
use clvmr::{Allocator, serde::node_to_bytes};
use id_arena::Arena;
use num_bigint::BigInt;
use rue_lir::{CodegenOptions, Lir, LirId};

mod limits;
mod runtime;

struct Lower<'a> {
    arena: Arena<Lir>,
    program: &'a Program,
    args: Vec<LirId>,
    optimize_integer_decoder: bool,
}
impl Lower<'_> {
    fn alloc(&mut self, lir: Lir) -> LirId {
        self.arena.alloc(lir)
    }
    fn bytes(&mut self, bytes: Vec<u8>) -> LirId {
        self.alloc(Lir::Atom(bytes))
    }
    fn number(&mut self, n: BigInt) -> LirId {
        self.bytes(if n == 0.into() {
            vec![]
        } else {
            n.to_signed_bytes_be()
        })
    }
    fn int(&mut self, n: i64) -> LirId {
        self.number(n.into())
    }
    fn nil(&mut self) -> LirId {
        self.bytes(vec![])
    }
    fn list(&mut self, items: &[LirId]) -> LirId {
        let mut tail = self.nil();
        for item in items.iter().rev() {
            tail = self.alloc(Lir::Cons(*item, tail));
        }
        tail
    }
    fn reject(&mut self) -> LirId {
        self.alloc(Lir::Raise(vec![]))
    }
    fn branch(&mut self, cond: LirId, yes: LirId, no: LirId) -> LirId {
        self.alloc(Lir::If(cond, yes, no, false))
    }
    fn require(&mut self, cond: LirId, yes: LirId) -> LirId {
        let no = self.reject();
        self.branch(cond, yes, no)
    }
    fn scalar(&mut self, value: LirId, yes: LirId) -> LirId {
        let pair = self.alloc(Lir::Listp(value, true));
        let no = self.reject();
        self.branch(pair, no, yes)
    }
    fn eq_int(&mut self, value: LirId, n: i64) -> LirId {
        let n = self.int(n);
        self.alloc(Lir::Eq(value, n))
    }
    fn range(&mut self, value: LirId, ty: &Type, yes: LirId) -> Result<LirId> {
        let (min, max) = bounds(ty)?;
        let before = self.number(min - 1);
        let after = self.number(max + 1);
        let lower = self.alloc(Lir::Gt(value, before));
        let upper = self.alloc(Lir::Gt(after, value));
        let yes = self.require(upper, yes);
        Ok(self.require(lower, yes))
    }
    // Reject padded or nonminimal signed CLVM integers. This is structural, so
    // algebraic rewrites in the Rue optimizer cannot erase canonicality checks.
    fn canonical_int(&mut self, value: LirId, yes: LirId) -> LirId {
        if self.optimize_integer_decoder {
            // A nonempty signed atom is minimal exactly when its lexicographic
            // bytes lie in [0080, ff80): 00 needs a high second byte, ff needs
            // a low second byte, and all other leading bytes are minimal.
            // Scalar and length checks have already succeeded. Keep nil as a
            // lazy fast path and include the continuation only twice.
            let minimum = self.bytes(vec![0, 0x80]);
            let maximum = self.bytes(vec![0xff, 0x80]);
            let too_low = self.alloc(Lir::GtBytes(minimum, value));
            let below_maximum = self.alloc(Lir::GtBytes(maximum, value));
            let nonempty = self.require(below_maximum, yes);
            let reject = self.reject();
            let nonempty = self.branch(too_low, reject, nonempty);
            return self.branch(value, nonempty, yes);
        }
        let length = self.alloc(Lir::Strlen(value));
        let zero = self.int(0);
        let one = self.int(1);
        let two = self.int(2);
        let first = self.alloc(Lir::Substr(value, zero, Some(one)));
        let second = self.alloc(Lir::Substr(value, one, Some(two)));
        let leading_zero = self.bytes(vec![0]);
        let leading_ff = self.bytes(vec![255]);
        let high_boundary = self.bytes(vec![127]);
        let low_boundary = self.bytes(vec![128]);
        let first_zero = self.alloc(Lir::Eq(first, leading_zero));
        let first_ff = self.alloc(Lir::Eq(first, leading_ff));
        let high = self.alloc(Lir::GtBytes(second, high_boundary));
        let low = self.alloc(Lir::GtBytes(low_boundary, second));
        let zero_ok = self.require(high, yes);
        let ff_ok = self.require(low, yes);
        let multi = self.branch(first_ff, ff_ok, yes);
        let multi = self.branch(first_zero, zero_ok, multi);
        let reject = self.reject();
        let single = self.branch(first_zero, reject, yes);
        let length_one = self.eq_int(length, 1);
        let nonempty = self.branch(length_one, single, multi);
        let length_zero = self.eq_int(length, 0);
        self.branch(length_zero, yes, nonempty)
    }
    fn decode(&mut self, value: LirId, ty: &Type, yes: LirId) -> Result<LirId> {
        Ok(match ty {
            Type::Tree => yes,
            Type::Atom => self.scalar(value, yes),
            Type::Unit => {
                let nil = self.nil();
                let empty = self.alloc(Lir::Eq(value, nil));
                self.require(empty, yes)
            }
            Type::Bool => {
                let nil = self.nil();
                let one = self.int(1);
                let is_zero = self.alloc(Lir::Eq(value, nil));
                let is_one = self.alloc(Lir::Eq(value, one));
                let yes_one = self.require(is_one, yes);
                let body = self.branch(is_zero, yes, yes_one);
                self.scalar(value, body)
            }
            Type::Int { bits, .. } => {
                let yes = self.range(value, ty, yes)?;
                let yes = self.canonical_int(value, yes);
                let len = self.alloc(Lir::Strlen(value));
                let bound = self.int(i64::from(*bits) / 8 + 2);
                let short = self.alloc(Lir::Gt(bound, len));
                let yes = self.require(short, yes);
                self.scalar(value, yes)
            }
            Type::Bytes { len } => {
                let length = self.alloc(Lir::Strlen(value));
                let correct = self.eq_int(length, *len as i64);
                let yes = self.require(correct, yes);
                self.scalar(value, yes)
            }
            Type::Tuple { items } => self.decode_list(value, items, yes)?,
            Type::Pair { first, rest } => {
                let head = self.alloc(Lir::First(value));
                let tail = self.alloc(Lir::Rest(value));
                let body = self.decode(tail, rest, yes)?;
                let body = self.decode(head, first, body)?;
                let pair = self.alloc(Lir::Listp(value, true));
                self.require(pair, body)
            }
        })
    }
    fn decode_list(&mut self, value: LirId, items: &[Type], yes: LirId) -> Result<LirId> {
        if let Some((first, rest)) = items.split_first() {
            let head = self.alloc(Lir::First(value));
            let tail = self.alloc(Lir::Rest(value));
            let body = self.decode_list(tail, rest, yes)?;
            let body = self.decode(head, first, body)?;
            let pair = self.alloc(Lir::Listp(value, true));
            Ok(self.require(pair, body))
        } else {
            let nil = self.nil();
            let empty = self.alloc(Lir::Eq(value, nil));
            Ok(self.require(empty, yes))
        }
    }
    fn field(&mut self, mut base: LirId, index: usize, ty: Option<Type>) -> LirId {
        if matches!(ty, Some(Type::Bytes { .. })) {
            let start = self.int(index as i64);
            let end = self.int(index as i64 + 1);
            let byte = self.alloc(Lir::Substr(base, start, Some(end)));
            // Keep a high byte positive, then canonicalize its numeric atom.
            let zero_byte = self.bytes(vec![0]);
            let numeric = self.alloc(Lir::Concat(vec![zero_byte, byte]));
            let zero = self.nil();
            self.alloc(Lir::Add(vec![zero, numeric]))
        } else {
            for _ in 0..index {
                base = self.alloc(Lir::Rest(base));
            }
            self.alloc(Lir::First(base))
        }
    }
    fn greater(&mut self, l: LirId, r: LirId, ty: &Type) -> LirId {
        // Atoms and fixed bytes use unsigned byte lexicographic ordering;
        // integer comparison keeps signed CLVM numeric semantics.
        self.alloc(if matches!(ty, Type::Atom | Type::Bytes { .. }) {
            Lir::GtBytes(l, r)
        } else {
            Lir::Gt(l, r)
        })
    }
    fn binary(&mut self, kind: BinaryOp, l: LirId, r: LirId, ty: &Type) -> Result<LirId> {
        let root = match kind {
            BinaryOp::Eq => self.alloc(Lir::Eq(l, r)),
            BinaryOp::Ne => {
                let equal = self.alloc(Lir::Eq(l, r));
                self.alloc(Lir::Not(equal))
            }
            BinaryOp::Gt => self.greater(l, r, ty),
            BinaryOp::Lt => self.greater(r, l, ty),
            BinaryOp::Ge => {
                let less = self.greater(r, l, ty);
                self.alloc(Lir::Not(less))
            }
            BinaryOp::Le => {
                let greater = self.greater(l, r, ty);
                self.alloc(Lir::Not(greater))
            }
            BinaryOp::Add => self.alloc(Lir::Add(vec![l, r])),
            BinaryOp::Sub => self.alloc(Lir::Sub(vec![l, r])),
            BinaryOp::Mul => self.alloc(Lir::Mul(vec![l, r])),
            // Fixed-width integer operands are already range checked. Infinite
            // two's-complement operations therefore stay in the same width;
            // canonical bools are precisely the integer values zero and one.
            BinaryOp::Bitand => self.alloc(Lir::Logand(vec![l, r])),
            BinaryOp::Bitor => self.alloc(Lir::Logior(vec![l, r])),
            BinaryOp::Bitxor => self.alloc(Lir::Logxor(vec![l, r])),
            _ => unreachable!("validated operations only"),
        };
        Ok(
            if matches!(kind, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul) {
                // Bind once: repeating nested arithmetic would expand exponentially.
                let local = self.alloc(Lir::Path(2u8.into()));
                let checked = self.range(local, ty, local)?;
                let quoted = self.alloc(Lir::Quote(checked));
                let environment = self.list(&[root]);
                self.alloc(Lir::Run(quoted, environment))
            } else {
                root
            },
        )
    }
    fn expression(&mut self, expr: &Expr) -> Result<LirId> {
        Ok(match expr {
            Expr::Sha256 { .. } => {
                anyhow::bail!("SHA256 requires strict version-4 lowering")
            }
            Expr::Atom { hex } => self.bytes(hex::decode(hex)?),
            Expr::AtomIsEmpty { value } => {
                let value = self.expression(value)?;
                let nil = self.nil();
                self.alloc(Lir::Eq(value, nil))
            }
            Expr::TreeAtom { value } => self.expression(value)?,
            Expr::TreePair { first, rest } | Expr::Pair { first, rest } => {
                let first = self.expression(first)?;
                let rest = self.expression(rest)?;
                self.alloc(Lir::Cons(first, rest))
            }
            Expr::PairFirst { .. } | Expr::PairRest { .. } => {
                anyhow::bail!("typed pairs require strict version-5 lowering")
            }
            Expr::TreeIsPair { value } => {
                let value = self.expression(value)?;
                self.alloc(Lir::Listp(value, true))
            }
            Expr::TreeFirst { value } => {
                let value = self.expression(value)?;
                self.alloc(Lir::First(value))
            }
            Expr::TreeRest { value } => {
                let value = self.expression(value)?;
                self.alloc(Lir::Rest(value))
            }
            Expr::TreeAtomValue { value } => {
                let value = self.expression(value)?;
                self.scalar(value, value)
            }
            Expr::Unit => self.nil(),
            Expr::Bool { value } => self.int(i64::from(*value)),
            Expr::Int { value, .. } => self.number(value.parse()?),
            Expr::Bytes { hex } => self.bytes(hex::decode(hex)?),
            Expr::Arg { index } => self.args[*index],
            Expr::Call { .. } | Expr::Let { .. } | Expr::Local { .. } => {
                anyhow::bail!("runtime nodes require the version-4 lowering")
            }
            Expr::Reject { .. } => self.reject(),
            Expr::Tuple { items } => {
                let values = items
                    .iter()
                    .map(|e| self.expression(e))
                    .collect::<Result<Vec<_>>>()?;
                self.list(&values)
            }
            Expr::Field { value, index } => {
                let ty = self.program.type_of(value)?;
                let base = self.expression(value)?;
                self.field(base, *index, ty)
            }
            Expr::Sequence { first, then_value } => {
                let first = self.expression(first)?;
                let then_value = self.expression(then_value)?;
                let pair = self.alloc(Lir::Cons(first, then_value));
                self.alloc(Lir::Rest(pair))
            }
            Expr::Not { value } => {
                let value = self.expression(value)?;
                self.alloc(Lir::Not(value))
            }
            Expr::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.expression(condition)?;
                let yes = self.expression(then_value)?;
                let no = self.expression(else_value)?;
                self.branch(condition, yes, no)
            }
            Expr::Binary {
                kind,
                left,
                right,
                ty,
            } => {
                let l = self.expression(left)?;
                let r = self.expression(right)?;
                self.binary(*kind, l, r, ty)?
            }
        })
    }
}
pub fn compile(program: &Program, optimize: bool) -> Result<Vec<u8>> {
    // Bound even structural scans before observing the original runtime mode.
    program.validate()?;
    // Optional inlining may remove the last table, call or binding. The
    // original program still selects strict runtime operand evaluation; table
    // pruning must not silently choose the legacy lowering convention.
    let runtime_required = program.uses_runtime();
    if !optimize {
        return emit(program, false, runtime_required);
    }
    let plan = crate::optimize::plan(program)?;
    let result = emit(&plan.program, true, runtime_required);
    match (result, plan.before_inlining) {
        (Err(error), Some(previous)) if error.is::<limits::Limit>() => {
            emit_preceding(&previous, true, runtime_required)
        }
        (result, _) => result,
    }
}

fn emit(program: &Program, optimize: bool, runtime_required: bool) -> Result<Vec<u8>> {
    if optimize {
        let result = emit_layout_attempt(program, true, runtime_required, true, true, true);
        if !result.as_ref().is_err_and(|error| {
            error.is::<runtime::AtomBudget>()
                || error.is::<runtime::ProjectionBudget>()
                || error.is::<limits::Limit>()
        }) {
            return result;
        }
        // Frame compaction is optional. Any typed analysis/emission limit
        // retries the complete preceding emitter with its original layout.
    }
    emit_preceding(program, optimize, runtime_required)
}

fn emit_preceding(program: &Program, optimize: bool, runtime_required: bool) -> Result<Vec<u8>> {
    if optimize {
        let result = emit_atom_attempt(program, optimize, runtime_required, true, true);
        if !result.as_ref().is_err_and(|error| {
            error.is::<runtime::AtomBudget>()
                || error.is::<runtime::ProjectionBudget>()
                || error.is::<limits::Limit>()
        }) {
            return result;
        }
        // Atom-side facts have their own optional attempt. Its limits must
        // retain the current pair-only guarded emitter in a fresh arena.
    }
    let result = emit_attempt(program, optimize, runtime_required, optimize);
    if optimize
        && result.as_ref().is_err_and(|error| {
            error.is::<runtime::ProjectionBudget>() || error.is::<limits::Limit>()
        })
    {
        // Optional projection analysis may spend work or change the expansion
        // shape. Retry the preceding ON lowering in a fresh arena before the
        // existing inlining fallback, keeping the original evaluation mode.
        return emit_attempt(program, optimize, runtime_required, false);
    }
    result
}

fn emit_attempt(
    program: &Program,
    optimize: bool,
    runtime_required: bool,
    guarded_projections: bool,
) -> Result<Vec<u8>> {
    emit_atom_attempt(
        program,
        optimize,
        runtime_required,
        guarded_projections,
        false,
    )
}

fn emit_atom_attempt(
    program: &Program,
    optimize: bool,
    runtime_required: bool,
    guarded_projections: bool,
    atom_facts: bool,
) -> Result<Vec<u8>> {
    emit_layout_attempt(
        program,
        optimize,
        runtime_required,
        guarded_projections,
        atom_facts,
        false,
    )
}

fn emit_layout_attempt(
    program: &Program,
    optimize: bool,
    runtime_required: bool,
    guarded_projections: bool,
    atom_facts: bool,
    compact_unary: bool,
) -> Result<Vec<u8>> {
    program.validate()?;
    let mut lower = Lower {
        arena: Arena::new(),
        program,
        args: vec![],
        optimize_integer_decoder: optimize,
    };
    let env = lower.alloc(Lir::Path(1u8.into()));
    if program.environment == crate::Environment::Raw {
        lower.args.push(env);
    } else {
        let mut tail = env;
        for _ in &program.parameters {
            let head = lower.alloc(Lir::First(tail));
            lower.args.push(head);
            tail = lower.alloc(Lir::Rest(tail));
        }
    }
    let body = if runtime_required || program.uses_runtime() {
        runtime::bootstrap(
            &mut lower,
            optimize,
            guarded_projections,
            atom_facts,
            compact_unary,
        )?
    } else {
        lower.expression(&program.body)?
    };
    let types = program
        .parameters
        .iter()
        .map(|p| p.ty.clone())
        .collect::<Vec<_>>();
    let mut root = match program.environment {
        crate::Environment::Strict => lower.decode_list(env, &types, body)?,
        crate::Environment::Ignored => body,
        crate::Environment::Raw => body,
    };
    check_expansion(&lower.arena, root)?;
    if optimize {
        root = rue_lir::optimize(&mut lower.arena, root);
    }
    check_expansion(&lower.arena, root)?;
    let mut allocator = Allocator::new();
    let ptr = rue_lir::codegen(
        &lower.arena,
        &mut allocator,
        root,
        CodegenOptions {
            optimize_static_pairs: optimize,
        },
    )?;
    Ok(node_to_bytes(&allocator, ptr)?)
}

// Rue codegen expands arena references into a serialized tree. Bound that
// expansion before allocating it; limiting arena nodes alone is insufficient.
fn check_expansion(arena: &Arena<Lir>, root: LirId) -> Result<()> {
    use anyhow::{bail, ensure};
    use std::collections::HashMap;
    fn size(
        arena: &Arena<Lir>,
        id: LirId,
        depth: usize,
        memo: &mut HashMap<LirId, u64>,
    ) -> Result<u64> {
        ensure!(depth < 96, limits::Limit::ExpansionDepth);
        if let Some(size) = memo.get(&id) {
            return Ok(*size);
        }
        let (base, children): (u64, Vec<LirId>) = match &arena[id] {
            Lir::Atom(atom) => (atom.len() as u64 + 8, vec![]),
            Lir::Path(path) => (path.bits() / 8 + 8, vec![]),
            Lir::Quote(a)
            | Lir::First(a)
            | Lir::Rest(a)
            | Lir::Listp(a, _)
            | Lir::Not(a)
            | Lir::Strlen(a) => (256, vec![*a]),
            Lir::Run(a, b)
            | Lir::Cons(a, b)
            | Lir::Eq(a, b)
            | Lir::Gt(a, b)
            | Lir::GtBytes(a, b) => (256, vec![*a, *b]),
            Lir::Add(args)
            | Lir::Sub(args)
            | Lir::Mul(args)
            | Lir::Logand(args)
            | Lir::Logior(args)
            | Lir::Logxor(args)
            | Lir::Sha256(args)
            | Lir::All(args)
            | Lir::Any(args)
            | Lir::Concat(args)
            | Lir::Raise(args) => (256, args.clone()),
            Lir::If(c, t, e, _) => (256, vec![*c, *t, *e]),
            Lir::Substr(a, b, c) => (
                256,
                vec![Some(*a), Some(*b), *c].into_iter().flatten().collect(),
            ),
            _ => bail!("unexpected optimizer operation outside supported codegen budget"),
        };
        let mut total = base;
        for child in children {
            total = total.saturating_add(size(arena, child, depth + 1, memo)?);
            ensure!(total <= 4_000_000, limits::Limit::ExpansionSize);
        }
        memo.insert(id, total);
        Ok(total)
    }
    size(arena, root, 0, &mut HashMap::new())?;
    Ok(())
}
