use std::collections::{HashMap, HashSet};

use clvm_traits::{ToClvm, clvm_list, clvm_quote, clvm_tuple};
use clvmr::{Allocator, NodePtr};
use id_arena::Arena;
use num_bigint::BigUint;

use crate::{ClvmOp, Lir, LirId, Result, path_to_atom};

#[derive(Debug, Default, Clone, Copy)]
pub struct CodegenOptions {
    pub optimize_static_pairs: bool,
}

pub fn codegen(
    arena: &Arena<Lir>,
    allocator: &mut Allocator,
    lir: LirId,
    options: CodegenOptions,
) -> Result<NodePtr> {
    codegen_impl(arena, allocator, lir, options)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Mode {
    Program,
    Static,
}
#[derive(Clone, Copy)]
enum Finish {
    Program,
    Static,
    QuotedPair,
    StaticApply,
}
enum Work {
    Visit(LirId, Mode),
    Finish {
        lir: LirId,
        mode: Mode,
        kind: Finish,
        start: usize,
    },
    ClosureStart {
        lir: LirId,
        start: usize,
    },
    ClosureNext {
        lir: LirId,
        start: usize,
        function: NodePtr,
        args: NodePtr,
        remaining: usize,
    },
    ClosureWrap {
        lir: LirId,
        start: usize,
        function: NodePtr,
        args: NodePtr,
        remaining: usize,
    },
}

fn codegen_impl(
    arena: &Arena<Lir>,
    allocator: &mut Allocator,
    lir: LirId,
    options: CodegenOptions,
) -> Result<NodePtr> {
    let mut work = vec![Work::Visit(lir, Mode::Program)];
    let mut values = Vec::new();
    let mut active = HashSet::new();
    let mut static_values = HashMap::new();
    while let Some(task) = work.pop() {
        match task {
            Work::Visit(lir, mode) => {
                if !active.insert((lir, mode)) {
                    return Err(crate::Error::CodegenCycle);
                }
                if mode == Mode::Program
                    && let Lir::Closure(function, _, _) = &arena[lir]
                {
                    // Closures interleave allocations with child emission.
                    // q1 precedes every capture, and each wrapper precedes the
                    // next capture, matching allocator failure precedence.
                    work.push(Work::ClosureStart {
                        lir,
                        start: values.len(),
                    });
                    work.push(Work::Visit(*function, Mode::Program));
                    continue;
                }
                let kind = if mode == Mode::Static {
                    Finish::Static
                } else if options.optimize_static_pairs {
                    match &arena[lir] {
                        Lir::Cons(first, rest)
                            if is_static_value(arena, *first, &mut static_values)?
                                && is_static_value(arena, *rest, &mut static_values)? =>
                        {
                            Finish::QuotedPair
                        }
                        Lir::Run(callee, env)
                            if matches!(&arena[*env], Lir::Path(path) if path == &BigUint::from(1_u8))
                                && is_static_value(arena, *callee, &mut static_values)? =>
                        {
                            Finish::StaticApply
                        }
                        _ => Finish::Program,
                    }
                } else {
                    Finish::Program
                };
                work.push(Work::Finish {
                    lir,
                    mode,
                    kind,
                    start: values.len(),
                });
                match kind {
                    Finish::Static => match &arena[lir] {
                        Lir::Atom(_) => (),
                        Lir::Cons(first, rest) => {
                            work.push(Work::Visit(*rest, Mode::Static));
                            work.push(Work::Visit(*first, Mode::Static));
                        }
                        _ => return Err(crate::Error::CodegenChildren),
                    },
                    Finish::QuotedPair => {
                        let Lir::Cons(first, rest) = &arena[lir] else {
                            unreachable!()
                        };
                        work.push(Work::Visit(*rest, Mode::Static));
                        work.push(Work::Visit(*first, Mode::Static));
                    }
                    Finish::StaticApply => {
                        let Lir::Run(callee, _) = &arena[lir] else {
                            unreachable!()
                        };
                        work.push(Work::Visit(*callee, Mode::Static));
                    }
                    Finish::Program => push_program_children(arena, lir, &mut work)?,
                }
            }
            Work::Finish {
                lir,
                mode,
                kind,
                start,
            } => {
                let children = &values[start..];
                let value = match kind {
                    Finish::Program => emit_program(arena, allocator, lir, children)?,
                    Finish::Static => match &arena[lir] {
                        Lir::Atom(atom) if atom.is_empty() => NodePtr::NIL,
                        Lir::Atom(atom) => allocator.new_atom(atom)?,
                        Lir::Cons(_, _) if children.len() == 2 => {
                            allocator.new_pair(children[0], children[1])?
                        }
                        _ => return Err(crate::Error::CodegenChildren),
                    },
                    Finish::QuotedPair if children.len() == 2 => {
                        let pair = allocator.new_pair(children[0], children[1])?;
                        clvm_quote!(pair).to_clvm(allocator)?
                    }
                    Finish::StaticApply if children.len() == 1 => children[0],
                    Finish::QuotedPair | Finish::StaticApply => {
                        return Err(crate::Error::CodegenChildren);
                    }
                };
                values.truncate(start);
                values.push(value);
                active.remove(&(lir, mode));
            }
            Work::ClosureStart { lir, start } => {
                if values.len() != start + 1 {
                    return Err(crate::Error::CodegenChildren);
                }
                let function = values.pop().ok_or(crate::Error::CodegenChildren)?;
                let Lir::Closure(_, captures, has_parameters) = &arena[lir] else {
                    return Err(crate::Error::CodegenChildren);
                };
                let args = if *has_parameters {
                    clvm_quote!(1).to_clvm(allocator)?
                } else {
                    NodePtr::NIL
                };
                work.push(Work::ClosureNext {
                    lir,
                    start,
                    function,
                    args,
                    remaining: captures.len(),
                });
            }
            Work::ClosureNext {
                lir,
                start,
                function,
                args,
                remaining,
            } => {
                if values.len() != start {
                    return Err(crate::Error::CodegenChildren);
                }
                if remaining == 0 {
                    values.push(emit_closure_function(allocator, function, args)?);
                    active.remove(&(lir, Mode::Program));
                } else {
                    let Lir::Closure(_, captures, _) = &arena[lir] else {
                        return Err(crate::Error::CodegenChildren);
                    };
                    let next = remaining - 1;
                    let capture = *captures.get(next).ok_or(crate::Error::CodegenChildren)?;
                    work.push(Work::ClosureWrap {
                        lir,
                        start,
                        function,
                        args,
                        remaining: next,
                    });
                    work.push(Work::Visit(capture, Mode::Program));
                }
            }
            Work::ClosureWrap {
                lir,
                start,
                function,
                args,
                remaining,
            } => {
                if values.len() != start + 1 {
                    return Err(crate::Error::CodegenChildren);
                }
                let capture = values.pop().ok_or(crate::Error::CodegenChildren)?;
                let args = emit_closure_capture(allocator, capture, args)?;
                work.push(Work::ClosureNext {
                    lir,
                    start,
                    function,
                    args,
                    remaining,
                });
            }
        }
    }
    values.pop().ok_or(crate::Error::CodegenChildren)
}

/// Postorder scheduling preserves the former recursive allocation order. Do
/// not memoize emitted nodes: repeated DAG edges historically allocate again.
fn push_program_children(arena: &Arena<Lir>, lir: LirId, work: &mut Vec<Work>) -> Result<()> {
    let mut push = |id| work.push(Work::Visit(id, Mode::Program));
    match &arena[lir] {
        Lir::Atom(_) | Lir::Path(_) => (),
        Lir::Quote(id)
        | Lir::First(id)
        | Lir::Rest(id)
        | Lir::Listp(id, _)
        | Lir::Not(id)
        | Lir::Strlen(id)
        | Lir::Lognot(id)
        | Lir::PubkeyForExp(id)
        | Lir::G1Negate(id)
        | Lir::G2Negate(id)
        | Lir::Op(_, id)
        | Lir::DebugPrint(_, id) => push(*id),
        Lir::Run(a, b)
        | Lir::Cons(a, b)
        | Lir::Div(a, b)
        | Lir::Divmod(a, b)
        | Lir::Mod(a, b)
        | Lir::Eq(a, b)
        | Lir::Gt(a, b)
        | Lir::GtBytes(a, b)
        | Lir::Ash(a, b)
        | Lir::Lsh(a, b)
        | Lir::G1Multiply(a, b)
        | Lir::G2Multiply(a, b) => {
            push(*b);
            push(*a);
        }
        Lir::Modpow(a, b, c)
        | Lir::CoinId(a, b, c)
        | Lir::K1Verify(a, b, c)
        | Lir::R1Verify(a, b, c)
        | Lir::If(a, b, c, _) => {
            push(*c);
            push(*b);
            push(*a);
        }
        Lir::Add(args)
        | Lir::Sub(args)
        | Lir::Mul(args)
        | Lir::All(args)
        | Lir::Any(args)
        | Lir::Raise(args)
        | Lir::Concat(args)
        | Lir::Logand(args)
        | Lir::Logior(args)
        | Lir::Logxor(args)
        | Lir::G1Add(args)
        | Lir::G1Subtract(args)
        | Lir::G2Add(args)
        | Lir::G2Subtract(args)
        | Lir::BlsPairingIdentity(args)
        | Lir::Sha256(args)
        | Lir::Sha256Inline(args)
        | Lir::Keccak256(args)
        | Lir::Keccak256Inline(args) => {
            for id in args.iter().rev() {
                push(*id);
            }
        }
        Lir::Closure(..) => return Err(crate::Error::CodegenChildren),
        Lir::BlsVerify(first, args) => {
            for id in args.iter().rev() {
                push(*id);
            }
            push(*first);
        }
        Lir::Substr(first, start, end) => {
            if let Some(end) = end {
                push(*end);
            }
            push(*start);
            push(*first);
        }
        Lir::G1Map(first, dst) | Lir::G2Map(first, dst) => {
            if let Some(dst) = dst {
                push(*dst);
            }
            push(*first);
        }
    }
    Ok(())
}

/// Static-data recognition also uses an explicit stack. Cache only this pure
/// classification, never allocated CLVM nodes or their allocation effects.
fn is_static_value(
    arena: &Arena<Lir>,
    lir: LirId,
    cache: &mut HashMap<LirId, bool>,
) -> Result<bool> {
    let mut work = vec![(lir, false)];
    let mut active = HashSet::new();
    while let Some((id, finish)) = work.pop() {
        if cache.contains_key(&id) {
            continue;
        }
        match &arena[id] {
            Lir::Atom(_) => {
                cache.insert(id, true);
            }
            Lir::Cons(first, rest) => {
                if finish {
                    let value = cache[first] && cache[rest];
                    cache.insert(id, value);
                    active.remove(&id);
                } else {
                    if !active.insert(id) {
                        return Err(crate::Error::CodegenCycle);
                    }
                    work.push((id, true));
                    work.push((*rest, false));
                    work.push((*first, false));
                }
            }
            _ => {
                cache.insert(id, false);
            }
        }
    }
    Ok(cache[&lir])
}

fn emit_closure_capture(
    allocator: &mut Allocator,
    capture: NodePtr,
    args: NodePtr,
) -> Result<NodePtr> {
    Ok(clvm_list!(
        ClvmOp::Cons,
        clvm_quote!(ClvmOp::Cons),
        clvm_list!(
            ClvmOp::Cons,
            clvm_list!(ClvmOp::Cons, clvm_quote!(ClvmOp::Quote), capture),
            clvm_list!(ClvmOp::Cons, args, ())
        )
    )
    .to_clvm(allocator)?)
}

fn emit_closure_function(
    allocator: &mut Allocator,
    function: NodePtr,
    args: NodePtr,
) -> Result<NodePtr> {
    Ok(clvm_list!(
        ClvmOp::Cons,
        clvm_quote!(ClvmOp::Apply),
        clvm_list!(
            ClvmOp::Cons,
            clvm_list!(ClvmOp::Cons, clvm_quote!(ClvmOp::Quote), function),
            clvm_list!(ClvmOp::Cons, args, ())
        )
    )
    .to_clvm(allocator)?)
}

fn emit_program(
    arena: &Arena<Lir>,
    allocator: &mut Allocator,
    lir: LirId,
    emitted: &[NodePtr],
) -> Result<NodePtr> {
    let mut children = emitted.iter().copied();
    let mut codegen = |_arena: &Arena<Lir>, _allocator: &mut Allocator, _lir: LirId| {
        children.next().ok_or(crate::Error::CodegenChildren)
    };
    let result = match &arena[lir] {
        Lir::Atom(atom) => {
            if atom.is_empty() {
                return Ok(NodePtr::NIL);
            }
            let atom = allocator.new_atom(atom)?;
            Ok(clvm_quote!(atom).to_clvm(allocator)?)
        }
        Lir::Quote(arg) => {
            let arg = codegen(arena, allocator, *arg)?;
            Ok(clvm_quote!(arg).to_clvm(allocator)?)
        }
        Lir::Path(path) => {
            let atom = path_to_atom(path);
            Ok(allocator.new_atom(&atom)?)
        }
        Lir::Run(callee, env) => {
            let callee = codegen(arena, allocator, *callee)?;
            let env = codegen(arena, allocator, *env)?;
            Ok(clvm_list!(ClvmOp::Apply, callee, env).to_clvm(allocator)?)
        }
        Lir::Closure(..) => Err(crate::Error::CodegenChildren),
        Lir::First(arg) => {
            let arg = codegen(arena, allocator, *arg)?;
            Ok(clvm_list!(ClvmOp::First, arg).to_clvm(allocator)?)
        }
        Lir::Rest(arg) => {
            let arg = codegen(arena, allocator, *arg)?;
            Ok(clvm_list!(ClvmOp::Rest, arg).to_clvm(allocator)?)
        }
        Lir::Cons(first, rest) => {
            let first = codegen(arena, allocator, *first)?;
            let rest = codegen(arena, allocator, *rest)?;
            Ok(clvm_list!(ClvmOp::Cons, first, rest).to_clvm(allocator)?)
        }
        Lir::Listp(arg, _) => {
            let arg = codegen(arena, allocator, *arg)?;
            Ok(clvm_list!(ClvmOp::Listp, arg).to_clvm(allocator)?)
        }
        Lir::Add(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::Add, args).to_clvm(allocator)?)
        }
        Lir::Sub(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::Sub, args).to_clvm(allocator)?)
        }
        Lir::Mul(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::Mul, args).to_clvm(allocator)?)
        }
        Lir::Div(first, second) => {
            let first = codegen(arena, allocator, *first)?;
            let second = codegen(arena, allocator, *second)?;
            Ok(clvm_list!(ClvmOp::Div, first, second).to_clvm(allocator)?)
        }
        Lir::Divmod(first, second) => {
            let first = codegen(arena, allocator, *first)?;
            let second = codegen(arena, allocator, *second)?;
            Ok(clvm_list!(ClvmOp::Divmod, first, second).to_clvm(allocator)?)
        }
        Lir::Mod(first, second) => {
            let first = codegen(arena, allocator, *first)?;
            let second = codegen(arena, allocator, *second)?;
            Ok(clvm_list!(ClvmOp::Mod, first, second).to_clvm(allocator)?)
        }
        Lir::Modpow(base, exponent, modulus) => {
            let base = codegen(arena, allocator, *base)?;
            let exponent = codegen(arena, allocator, *exponent)?;
            let modulus = codegen(arena, allocator, *modulus)?;
            Ok(clvm_list!(ClvmOp::Modpow, base, exponent, modulus).to_clvm(allocator)?)
        }
        Lir::Eq(first, second) => {
            let first = codegen(arena, allocator, *first)?;
            let second = codegen(arena, allocator, *second)?;
            Ok(clvm_list!(ClvmOp::Eq, first, second).to_clvm(allocator)?)
        }
        Lir::Gt(first, second) => {
            let first = codegen(arena, allocator, *first)?;
            let second = codegen(arena, allocator, *second)?;
            Ok(clvm_list!(ClvmOp::Gt, first, second).to_clvm(allocator)?)
        }
        Lir::GtBytes(first, second) => {
            let first = codegen(arena, allocator, *first)?;
            let second = codegen(arena, allocator, *second)?;
            Ok(clvm_list!(ClvmOp::GtBytes, first, second).to_clvm(allocator)?)
        }
        Lir::Not(arg) => {
            let arg = codegen(arena, allocator, *arg)?;
            Ok(clvm_list!(ClvmOp::Not, arg).to_clvm(allocator)?)
        }
        Lir::All(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::All, args).to_clvm(allocator)?)
        }
        Lir::Any(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::Any, args).to_clvm(allocator)?)
        }
        Lir::If(cond, then_lir, else_lir, inline) => {
            let cond = codegen(arena, allocator, *cond)?;
            let then_ptr = codegen(arena, allocator, *then_lir)?;
            let else_ptr = codegen(arena, allocator, *else_lir)?;

            if *inline {
                Ok(clvm_list!(ClvmOp::If, cond, then_ptr, else_ptr).to_clvm(allocator)?)
            } else {
                Ok(clvm_list!(
                    ClvmOp::Apply,
                    clvm_list!(
                        ClvmOp::If,
                        cond,
                        clvm_quote!(then_ptr),
                        clvm_quote!(else_ptr)
                    ),
                    1
                )
                .to_clvm(allocator)?)
            }
        }
        Lir::Raise(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::Raise, args).to_clvm(allocator)?)
        }
        Lir::Concat(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::Concat, args).to_clvm(allocator)?)
        }
        Lir::Strlen(arg) => {
            let arg = codegen(arena, allocator, *arg)?;
            Ok(clvm_list!(ClvmOp::Strlen, arg).to_clvm(allocator)?)
        }
        Lir::Substr(arg, start, end) => {
            let arg = codegen(arena, allocator, *arg)?;
            let start = codegen(arena, allocator, *start)?;
            let end = end.map(|end| codegen(arena, allocator, end)).transpose()?;
            if let Some(end) = end {
                Ok(clvm_list!(ClvmOp::Substr, arg, start, end).to_clvm(allocator)?)
            } else {
                Ok(clvm_list!(ClvmOp::Substr, arg, start).to_clvm(allocator)?)
            }
        }
        Lir::Logand(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::Logand, args).to_clvm(allocator)?)
        }
        Lir::Logior(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::Logior, args).to_clvm(allocator)?)
        }
        Lir::Logxor(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::Logxor, args).to_clvm(allocator)?)
        }
        Lir::Lognot(arg) => {
            let arg = codegen(arena, allocator, *arg)?;
            Ok(clvm_list!(ClvmOp::Lognot, arg).to_clvm(allocator)?)
        }
        Lir::Ash(arg, shift) => {
            let arg = codegen(arena, allocator, *arg)?;
            let shift = codegen(arena, allocator, *shift)?;
            Ok(clvm_list!(ClvmOp::Ash, arg, shift).to_clvm(allocator)?)
        }
        Lir::Lsh(arg, shift) => {
            let arg = codegen(arena, allocator, *arg)?;
            let shift = codegen(arena, allocator, *shift)?;
            Ok(clvm_list!(ClvmOp::Lsh, arg, shift).to_clvm(allocator)?)
        }
        Lir::PubkeyForExp(arg) => {
            let arg = codegen(arena, allocator, *arg)?;
            Ok(clvm_list!(ClvmOp::PubkeyForExp, arg).to_clvm(allocator)?)
        }
        Lir::G1Add(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::G1Add, args).to_clvm(allocator)?)
        }
        Lir::G1Subtract(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::G1Subtract, args).to_clvm(allocator)?)
        }
        Lir::G1Multiply(first, second) => {
            let first = codegen(arena, allocator, *first)?;
            let second = codegen(arena, allocator, *second)?;
            Ok(clvm_list!(ClvmOp::G1Multiply, first, second).to_clvm(allocator)?)
        }
        Lir::G1Negate(arg) => {
            let arg = codegen(arena, allocator, *arg)?;
            Ok(clvm_list!(ClvmOp::G1Negate, arg).to_clvm(allocator)?)
        }
        Lir::G1Map(value, dst) => {
            let value = codegen(arena, allocator, *value)?;
            let dst = dst.map(|dst| codegen(arena, allocator, dst)).transpose()?;
            if let Some(dst) = dst {
                Ok(clvm_list!(ClvmOp::G1Map, value, dst).to_clvm(allocator)?)
            } else {
                Ok(clvm_list!(ClvmOp::G1Map, value).to_clvm(allocator)?)
            }
        }
        Lir::G2Add(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::G2Add, args).to_clvm(allocator)?)
        }
        Lir::G2Subtract(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::G2Subtract, args).to_clvm(allocator)?)
        }
        Lir::G2Multiply(first, second) => {
            let first = codegen(arena, allocator, *first)?;
            let second = codegen(arena, allocator, *second)?;
            Ok(clvm_list!(ClvmOp::G2Multiply, first, second).to_clvm(allocator)?)
        }
        Lir::G2Negate(arg) => {
            let arg = codegen(arena, allocator, *arg)?;
            Ok(clvm_list!(ClvmOp::G2Negate, arg).to_clvm(allocator)?)
        }
        Lir::G2Map(value, dst) => {
            let value = codegen(arena, allocator, *value)?;
            let dst = dst.map(|dst| codegen(arena, allocator, dst)).transpose()?;
            if let Some(dst) = dst {
                Ok(clvm_list!(ClvmOp::G2Map, value, dst).to_clvm(allocator)?)
            } else {
                Ok(clvm_list!(ClvmOp::G2Map, value).to_clvm(allocator)?)
            }
        }
        Lir::BlsPairingIdentity(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::BlsPairingIdentity, args).to_clvm(allocator)?)
        }
        Lir::BlsVerify(arg, args) => {
            let arg = codegen(arena, allocator, *arg)?;
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok(clvm_tuple!(ClvmOp::BlsVerify, arg, args).to_clvm(allocator)?)
        }
        Lir::Sha256(args) | Lir::Sha256Inline(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::Sha256, args).to_clvm(allocator)?)
        }
        Lir::Keccak256(args) | Lir::Keccak256Inline(args) => {
            let args = args
                .iter()
                .map(|arg| codegen(arena, allocator, *arg))
                .collect::<Result<Vec<_>>>()?;
            Ok((ClvmOp::Keccak256, args).to_clvm(allocator)?)
        }
        Lir::CoinId(parent, puzzle, amount) => {
            let parent = codegen(arena, allocator, *parent)?;
            let puzzle = codegen(arena, allocator, *puzzle)?;
            let amount = codegen(arena, allocator, *amount)?;
            Ok(clvm_list!(ClvmOp::CoinId, parent, puzzle, amount).to_clvm(allocator)?)
        }
        Lir::K1Verify(pubkey, message, signature) => {
            let pubkey = codegen(arena, allocator, *pubkey)?;
            let message = codegen(arena, allocator, *message)?;
            let signature = codegen(arena, allocator, *signature)?;
            Ok(
                clvm_list!(ClvmOp::Secp256K1Verify, pubkey, message, signature)
                    .to_clvm(allocator)?,
            )
        }
        Lir::R1Verify(pubkey, message, signature) => {
            let pubkey = codegen(arena, allocator, *pubkey)?;
            let message = codegen(arena, allocator, *message)?;
            let signature = codegen(arena, allocator, *signature)?;
            Ok(
                clvm_list!(ClvmOp::Secp256R1Verify, pubkey, message, signature)
                    .to_clvm(allocator)?,
            )
        }
        Lir::Op(op, args) => {
            let args = codegen(arena, allocator, *args)?;
            Ok(clvm_list!(
                ClvmOp::Apply,
                clvm_list!(
                    ClvmOp::Cons,
                    clvm_list!(ClvmOp::Cons, clvm_quote!(op), ()),
                    args
                ),
                ()
            )
            .to_clvm(allocator)?)
        }
        Lir::DebugPrint(srcloc, value) => {
            let value = codegen(arena, allocator, *value)?;
            Ok(clvm_list!(ClvmOp::DebugPrint, clvm_quote!(srcloc), value).to_clvm(allocator)?)
        }
    };
    if children.next().is_some() {
        return Err(crate::Error::CodegenChildren);
    }
    result
}

#[cfg(test)]
mod tests {
    use expect_test::{Expect, expect};
    use rue_clvm::disassemble_program;

    use super::*;

    #[allow(clippy::needless_pass_by_value)]
    fn check(arena: &Arena<Lir>, lir: LirId, expect: Expect) {
        let mut allocator = Allocator::new();
        let ptr = codegen(arena, &mut allocator, lir, CodegenOptions::default()).unwrap();
        let result = disassemble_program(&allocator, ptr);
        expect.assert_eq(&result);
    }

    #[allow(clippy::needless_pass_by_value)]
    fn check_with_options(
        arena: &Arena<Lir>,
        lir: LirId,
        optimize_static_pairs: bool,
        expect: Expect,
    ) {
        let mut allocator = Allocator::new();
        let ptr = codegen(
            arena,
            &mut allocator,
            lir,
            CodegenOptions {
                optimize_static_pairs,
            },
        )
        .unwrap();
        let result = disassemble_program(&allocator, ptr);
        expect.assert_eq(&result);
    }

    #[test]
    fn test_atom() {
        let mut arena = Arena::new();
        let lir = arena.alloc(Lir::Atom(b"hello".to_vec()));
        check(&arena, lir, expect![[r#"(q . "hello")"#]]);
    }

    #[test]
    fn test_nil() {
        let mut arena = Arena::new();
        let lir = arena.alloc(Lir::Atom(Vec::new()));
        check(&arena, lir, expect!["()"]);
    }

    #[test]
    fn test_path() {
        let mut arena = Arena::new();
        let lir = arena.alloc(Lir::Path(1u8.into()));
        check(&arena, lir, expect!["1"]);
    }

    #[test]
    fn test_quote() {
        let mut arena = Arena::new();
        let lir = arena.alloc(Lir::Atom(b"hello".to_vec()));
        let quote = arena.alloc(Lir::Quote(lir));
        check(&arena, quote, expect![[r#"(q 1 . "hello")"#]]);
    }

    #[test]
    fn test_run() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(b"a".to_vec()));
        let b = arena.alloc(Lir::Atom(b"b".to_vec()));
        let c = arena.alloc(Lir::Atom(b"c".to_vec()));
        let mut env = arena.alloc(Lir::Atom(Vec::new()));
        for &arg in [b, c].iter().rev() {
            env = arena.alloc(Lir::Cons(arg, env));
        }
        let lir = arena.alloc(Lir::Run(a, env));
        check(
            &arena,
            lir,
            expect!["(a (q . 97) (c (q . 98) (c (q . 99) ())))"],
        );
    }

    #[test]
    fn test_run_static_program_with_entire_env() {
        let mut arena = Arena::new();
        let nil = arena.alloc(Lir::Atom(Vec::new()));
        let five = arena.alloc(Lir::Atom(vec![5]));
        let args = arena.alloc(Lir::Cons(five, nil));
        let two = arena.alloc(Lir::Atom(vec![2]));
        let args = arena.alloc(Lir::Cons(two, args));
        let add = arena.alloc(Lir::Atom(vec![16]));
        let program = arena.alloc(Lir::Cons(add, args));
        let env = arena.alloc(Lir::Path(1u8.into()));
        let lir = arena.alloc(Lir::Run(program, env));

        check_with_options(
            &arena,
            lir,
            false,
            expect!["(a (c (q . 16) (c (q . 2) (c (q . 5) ()))) 1)"],
        );
        check_with_options(&arena, lir, true, expect!["(+ 2 5)"]);
    }

    #[test]
    fn test_closure() {
        let mut arena = Arena::new();
        let path = arena.alloc(Lir::Path(2u8.into()));
        let add = arena.alloc(Lir::Add(vec![path, path]));
        let function = arena.alloc(Lir::Quote(add));
        let capture = arena.alloc(Lir::Atom(vec![0x10]));
        let closure = arena.alloc(Lir::Closure(function, vec![capture], true));
        check(
            &arena,
            closure,
            expect![
                "(c (q . 2) (c (c (q . 1) (q 16 2 2)) (c (c (q . 4) (c (c (q . 1) (q . 16)) (c (q . 1) ()))) ())))"
            ],
        );
    }

    #[test]
    fn test_first() {
        let mut arena = Arena::new();
        let first = arena.alloc(Lir::Atom(b"first".to_vec()));
        let rest = arena.alloc(Lir::Atom(b"rest".to_vec()));
        let pair = arena.alloc(Lir::Cons(first, rest));
        let lir = arena.alloc(Lir::First(pair));
        check(
            &arena,
            lir,
            expect![[r#"(f (c (q . "first") (q . "rest")))"#]],
        );
    }

    #[test]
    fn test_rest() {
        let mut arena = Arena::new();
        let first = arena.alloc(Lir::Atom(b"first".to_vec()));
        let rest = arena.alloc(Lir::Atom(b"rest".to_vec()));
        let pair = arena.alloc(Lir::Cons(first, rest));
        let lir = arena.alloc(Lir::Rest(pair));
        check(
            &arena,
            lir,
            expect![[r#"(r (c (q . "first") (q . "rest")))"#]],
        );
    }

    #[test]
    fn test_cons() {
        let mut arena = Arena::new();
        let first = arena.alloc(Lir::Atom(b"first".to_vec()));
        let rest = arena.alloc(Lir::Atom(b"rest".to_vec()));
        let lir = arena.alloc(Lir::Cons(first, rest));
        check(&arena, lir, expect![[r#"(c (q . "first") (q . "rest"))"#]]);
        check_with_options(&arena, lir, true, expect![[r#"(q "first" . "rest")"#]]);

        let path = arena.alloc(Lir::Path(2u8.into()));
        let lir = arena.alloc(Lir::Cons(path, lir));
        check_with_options(
            &arena,
            lir,
            true,
            expect![[r#"(c 2 (q "first" . "rest"))"#]],
        );
    }

    #[test]
    fn test_listp() {
        let mut arena = Arena::new();
        let value = arena.alloc(Lir::Atom(b"value".to_vec()));
        let lir = arena.alloc(Lir::Listp(value, true));
        check(&arena, lir, expect![[r#"(l (q . "value"))"#]]);
    }

    #[test]
    fn test_add() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x01]));
        let b = arena.alloc(Lir::Atom(vec![0x02]));
        let lir = arena.alloc(Lir::Add(vec![a, b]));
        check(&arena, lir, expect!["(+ (q . 1) (q . 2))"]);
    }

    #[test]
    fn test_sub() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x01]));
        let b = arena.alloc(Lir::Atom(vec![0x02]));
        let lir = arena.alloc(Lir::Sub(vec![a, b]));
        check(&arena, lir, expect!["(- (q . 1) (q . 2))"]);
    }

    #[test]
    fn test_mul() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x01]));
        let b = arena.alloc(Lir::Atom(vec![0x02]));
        let lir = arena.alloc(Lir::Mul(vec![a, b]));
        check(&arena, lir, expect!["(* (q . 1) (q . 2))"]);
    }

    #[test]
    fn test_div() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x01]));
        let b = arena.alloc(Lir::Atom(vec![0x02]));
        let lir = arena.alloc(Lir::Div(a, b));
        check(&arena, lir, expect!["(/ (q . 1) (q . 2))"]);
    }

    #[test]
    fn test_divmod() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x34]));
        let b = arena.alloc(Lir::Atom(vec![0x07]));
        let lir = arena.alloc(Lir::Divmod(a, b));
        check(&arena, lir, expect!["(divmod (q . 52) (q . 7))"]);
    }

    #[test]
    fn test_mod() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x34]));
        let b = arena.alloc(Lir::Atom(vec![0x07]));
        let lir = arena.alloc(Lir::Mod(a, b));
        check(&arena, lir, expect!["(% (q . 52) (q . 7))"]);
    }

    #[test]
    fn test_modpow() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x34]));
        let b = arena.alloc(Lir::Atom(vec![0x07]));
        let c = arena.alloc(Lir::Atom(vec![0x03]));
        let lir = arena.alloc(Lir::Modpow(a, b, c));
        check(&arena, lir, expect!["(modpow (q . 52) (q . 7) (q . 3))"]);
    }

    #[test]
    fn test_eq() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x01]));
        let b = arena.alloc(Lir::Atom(vec![0x02]));
        let lir = arena.alloc(Lir::Eq(a, b));
        check(&arena, lir, expect!["(= (q . 1) (q . 2))"]);
    }

    #[test]
    fn test_gt() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x01]));
        let b = arena.alloc(Lir::Atom(vec![0x02]));
        let lir = arena.alloc(Lir::Gt(a, b));
        check(&arena, lir, expect!["(> (q . 1) (q . 2))"]);
    }

    #[test]
    fn test_not() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x01]));
        let lir = arena.alloc(Lir::Not(a));
        check(&arena, lir, expect!["(not (q . 1))"]);
    }

    #[test]
    fn test_all() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x01]));
        let b = arena.alloc(Lir::Atom(vec![0x02]));
        let lir = arena.alloc(Lir::All(vec![a, b]));
        check(&arena, lir, expect!["(all (q . 1) (q . 2))"]);
    }

    #[test]
    fn test_any() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x01]));
        let b = arena.alloc(Lir::Atom(vec![0x02]));
        let lir = arena.alloc(Lir::Any(vec![a, b]));
        check(&arena, lir, expect!["(any (q . 1) (q . 2))"]);
    }

    #[test]
    fn test_if() {
        let mut arena = Arena::new();
        let cond = arena.alloc(Lir::Atom(vec![0x01]));
        let then = arena.alloc(Lir::Atom(vec![0x02]));
        let else_ = arena.alloc(Lir::Atom(vec![0x03]));
        let lir = arena.alloc(Lir::If(cond, then, else_, false));
        check(
            &arena,
            lir,
            expect!["(a (i (q . 1) (q 1 . 2) (q 1 . 3)) 1)"],
        );
    }

    #[test]
    fn test_raise() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x01]));
        let lir = arena.alloc(Lir::Raise(vec![a]));
        check(&arena, lir, expect!["(x (q . 1))"]);
    }

    #[test]
    fn test_gt_bytes() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(b"abc".to_vec()));
        let b = arena.alloc(Lir::Atom(b"def".to_vec()));
        let lir = arena.alloc(Lir::GtBytes(a, b));
        check(&arena, lir, expect![[r#"(>s (q . "abc") (q . "def"))"#]]);
    }

    #[test]
    fn test_concat() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(b"hello ".to_vec()));
        let b = arena.alloc(Lir::Atom(b"world".to_vec()));
        let lir = arena.alloc(Lir::Concat(vec![a, b]));
        check(
            &arena,
            lir,
            expect![[r#"(concat (q . "hello ") (q . "world"))"#]],
        );
    }

    #[test]
    fn test_strlen() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(b"hello".to_vec()));
        let lir = arena.alloc(Lir::Strlen(a));
        check(&arena, lir, expect![[r#"(strlen (q . "hello"))"#]]);
    }

    #[test]
    fn test_substr() {
        let mut arena = Arena::new();
        let str = arena.alloc(Lir::Atom(b"hello world".to_vec()));
        let start = arena.alloc(Lir::Atom(vec![]));
        let end = arena.alloc(Lir::Atom(vec![0x05]));
        let lir = arena.alloc(Lir::Substr(str, start, Some(end)));
        check(
            &arena,
            lir,
            expect![[r#"(substr (q . "hello world") () (q . 5))"#]],
        );

        // Test without end parameter
        let lir = arena.alloc(Lir::Substr(str, start, None));
        check(&arena, lir, expect![[r#"(substr (q . "hello world") ())"#]]);
    }

    #[test]
    fn test_bitwise_ops() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x0F]));
        let b = arena.alloc(Lir::Atom(vec![0xF0]));

        let logand = arena.alloc(Lir::Logand(vec![a, b]));
        check(&arena, logand, expect!["(logand (q . 15) (q . -16))"]);

        let logior = arena.alloc(Lir::Logior(vec![a, b]));
        check(&arena, logior, expect!["(logior (q . 15) (q . -16))"]);

        let logxor = arena.alloc(Lir::Logxor(vec![a, b]));
        check(&arena, logxor, expect!["(logxor (q . 15) (q . -16))"]);

        let lognot = arena.alloc(Lir::Lognot(a));
        check(&arena, lognot, expect!["(lognot (q . 15))"]);
    }

    #[test]
    fn test_shifts() {
        let mut arena = Arena::new();
        let value = arena.alloc(Lir::Atom(vec![0x0F]));
        let shift = arena.alloc(Lir::Atom(vec![0x02]));

        let ash = arena.alloc(Lir::Ash(value, shift));
        check(&arena, ash, expect!["(ash (q . 15) (q . 2))"]);

        let lsh = arena.alloc(Lir::Lsh(value, shift));
        check(&arena, lsh, expect!["(lsh (q . 15) (q . 2))"]);
    }

    #[test]
    fn test_pubkey_for_exp() {
        let mut arena = Arena::new();
        let exp = arena.alloc(Lir::Atom(vec![0x01]));
        let lir = arena.alloc(Lir::PubkeyForExp(exp));
        check(&arena, lir, expect!["(pubkey_for_exp (q . 1))"]);
    }

    #[test]
    fn test_g1_ops() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x01]));
        let b = arena.alloc(Lir::Atom(vec![0x02]));

        let add = arena.alloc(Lir::G1Add(vec![a, b]));
        check(&arena, add, expect!["(point_add (q . 1) (q . 2))"]);

        let sub = arena.alloc(Lir::G1Subtract(vec![a, b]));
        check(&arena, sub, expect!["(g1_subtract (q . 1) (q . 2))"]);

        let mul = arena.alloc(Lir::G1Multiply(a, b));
        check(&arena, mul, expect!["(g1_multiply (q . 1) (q . 2))"]);

        let neg = arena.alloc(Lir::G1Negate(a));
        check(&arena, neg, expect!["(g1_negate (q . 1))"]);

        let map = arena.alloc(Lir::G1Map(a, Some(b)));
        check(&arena, map, expect!["(g1_map (q . 1) (q . 2))"]);

        let map_no_dst = arena.alloc(Lir::G1Map(a, None));
        check(&arena, map_no_dst, expect!["(g1_map (q . 1))"]);
    }

    #[test]
    fn test_g2_ops() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x01]));
        let b = arena.alloc(Lir::Atom(vec![0x02]));

        let add = arena.alloc(Lir::G2Add(vec![a, b]));
        check(&arena, add, expect!["(g2_add (q . 1) (q . 2))"]);

        let sub = arena.alloc(Lir::G2Subtract(vec![a, b]));
        check(&arena, sub, expect!["(g2_subtract (q . 1) (q . 2))"]);

        let mul = arena.alloc(Lir::G2Multiply(a, b));
        check(&arena, mul, expect!["(g2_multiply (q . 1) (q . 2))"]);

        let neg = arena.alloc(Lir::G2Negate(a));
        check(&arena, neg, expect!["(g2_negate (q . 1))"]);

        let map = arena.alloc(Lir::G2Map(a, Some(b)));
        check(&arena, map, expect!["(g2_map (q . 1) (q . 2))"]);

        let map_no_dst = arena.alloc(Lir::G2Map(a, None));
        check(&arena, map_no_dst, expect!["(g2_map (q . 1))"]);
    }

    #[test]
    fn test_bls_ops() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(vec![0x01]));
        let b = arena.alloc(Lir::Atom(vec![0x02]));

        let identity = arena.alloc(Lir::BlsPairingIdentity(vec![a, b]));
        check(
            &arena,
            identity,
            expect!["(bls_pairing_identity (q . 1) (q . 2))"],
        );

        let verify = arena.alloc(Lir::BlsVerify(a, vec![b]));
        check(&arena, verify, expect!["(bls_verify (q . 1) (q . 2))"]);
    }

    #[test]
    fn test_hash_functions() {
        let mut arena = Arena::new();
        let a = arena.alloc(Lir::Atom(b"hello".to_vec()));
        let b = arena.alloc(Lir::Atom(b"world".to_vec()));

        let sha256 = arena.alloc(Lir::Sha256(vec![a, b]));
        check(
            &arena,
            sha256,
            expect![[r#"(sha256 (q . "hello") (q . "world"))"#]],
        );

        let keccak256 = arena.alloc(Lir::Keccak256(vec![a, b]));
        check(
            &arena,
            keccak256,
            expect![[r#"(keccak256 (q . "hello") (q . "world"))"#]],
        );
    }

    #[test]
    fn test_coin_id() {
        let mut arena = Arena::new();
        let parent = arena.alloc(Lir::Atom(vec![0x01]));
        let puzzle = arena.alloc(Lir::Atom(vec![0x02]));
        let amount = arena.alloc(Lir::Atom(vec![0x03]));
        let lir = arena.alloc(Lir::CoinId(parent, puzzle, amount));
        check(&arena, lir, expect!["(coinid (q . 1) (q . 2) (q . 3))"]);
    }

    #[test]
    fn test_signature_verification() {
        let mut arena = Arena::new();
        let pubkey = arena.alloc(Lir::Atom(vec![0x01]));
        let message = arena.alloc(Lir::Atom(vec![0x02]));
        let signature = arena.alloc(Lir::Atom(vec![0x03]));

        let k1_verify = arena.alloc(Lir::K1Verify(pubkey, message, signature));
        check(
            &arena,
            k1_verify,
            expect!["(secp256k1_verify (q . 1) (q . 2) (q . 3))"],
        );

        let r1_verify = arena.alloc(Lir::R1Verify(pubkey, message, signature));
        check(
            &arena,
            r1_verify,
            expect!["(secp256r1_verify (q . 1) (q . 2) (q . 3))"],
        );
    }
}
