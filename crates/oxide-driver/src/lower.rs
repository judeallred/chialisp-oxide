use std::cell::Cell;

use oxide_ir::{BinaryOp, Expr, Type};
use rustc_hir::def_id::DefId;
use rustc_middle::{
    mir::{
        AggregateKind, BasicBlock, BinOp, Body, Local, Operand, Place, ProjectionElem, Rvalue,
        START_BLOCK, StatementKind, TerminatorKind, UnOp,
    },
    ty::{self, Ty, TyCtxt},
};
use rustc_span::Span;

mod newtypes;

pub struct Lowerer<'tcx> {
    tcx: TyCtxt<'tcx>,
    blocks: Cell<usize>,
    expression_work: Cell<usize>,
    active_blocks: Cell<usize>,
}

const MAX_EXPRESSION_NODES: usize = 8_192;
const MAX_EXPRESSION_DEPTH: usize = 64;
const MAX_EXPRESSION_WORK: usize = 500_000;

struct BlockDepth<'a>(&'a Cell<usize>);
impl Drop for BlockDepth<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

impl<'tcx> Lowerer<'tcx> {
    fn allowed_external(&self, id: DefId) -> bool {
        !id.is_local() && self.tcx.crate_name(id.krate).as_str() == "oxide_sdk"
    }

    pub fn new(tcx: TyCtxt<'tcx>) -> Self {
        Self {
            tcx,
            blocks: Cell::new(0),
            expression_work: Cell::new(0),
            active_blocks: Cell::new(0),
        }
    }

    fn expression_size(&self, expression: &Expr, span: Span) -> Result<usize, String> {
        let mut pending = vec![(expression, 0)];
        let mut count = 0;
        while let Some((expression, depth)) = pending.pop() {
            count += 1;
            if count > MAX_EXPRESSION_NODES || depth >= MAX_EXPRESSION_DEPTH {
                return Err(self.error(
                    span,
                    "expression expansion limit exceeded (8,192 nodes or depth 64)",
                ));
            }
            let mut push = |expr| pending.push((expr, depth + 1));
            match expression {
                Expr::Tuple { items } => items.iter().for_each(push),
                Expr::Binary { left, right, .. } => {
                    push(left);
                    push(right);
                }
                Expr::Not { value } | Expr::Field { value, .. } => push(value),
                Expr::If {
                    condition,
                    then_value,
                    else_value,
                } => {
                    push(condition);
                    push(then_value);
                    push(else_value);
                }
                Expr::Sequence { first, then_value } => {
                    push(first);
                    push(then_value);
                }
                _ => (),
            }
        }
        Ok(count)
    }

    fn check_expression(&self, expression: &Expr, span: Span) -> Result<(), String> {
        let count = self.expression_size(expression, span)?;
        self.expression_work
            .set(self.expression_work.get().saturating_add(count));
        if self.expression_work.get() > MAX_EXPRESSION_WORK {
            return Err(self.error(
                span,
                "cumulative expression work limit exceeded (500,000 nodes)",
            ));
        }
        Ok(())
    }

    fn clone_locals(
        &self,
        locals: &[Option<Expr>],
        span: Span,
    ) -> Result<Vec<Option<Expr>>, String> {
        let mut count = 0;
        for value in locals.iter().flatten() {
            count += self.expression_size(value, span)?;
            self.check_expression(value, span)?;
            if count > MAX_EXPRESSION_NODES * 4 {
                return Err(self.error(span, "local substitution environment limit exceeded"));
            }
        }
        Ok(locals.to_vec())
    }

    fn error(&self, span: Span, message: impl std::fmt::Display) -> String {
        format!(
            "{}: {message}",
            self.tcx.sess.source_map().span_to_diagnostic_string(span)
        )
    }

    pub fn ty(&self, ty: Ty<'tcx>, span: Span) -> Result<Type, String> {
        self.lower_type(ty, span, 0).map(|(ty, _)| ty)
    }

    fn lower_type(&self, ty: Ty<'tcx>, span: Span, depth: usize) -> Result<(Type, usize), String> {
        if depth >= 32 {
            return Err(self.error(span, "type expansion limit exceeded (depth 32)"));
        }
        let scalar = match ty.kind() {
            ty::Bool => Type::Bool,
            ty::Int(int) => Type::Int {
                bits: int.bit_width().ok_or_else(|| self.error(span, "isize is target-dependent and unsupported"))? as u16,
                signed: true,
            },
            ty::Uint(int) => Type::Int {
                bits: int.bit_width().ok_or_else(|| self.error(span, "usize is target-dependent and unsupported"))? as u16,
                signed: false,
            },
            ty::Tuple(items) if items.is_empty() => Type::Unit,
            ty::Tuple(items) => {
                if items.len() > 256 {
                    return Err(self.error(span, "tuples are limited to 256 fields"));
                }
                let mut fields = Vec::new();
                let mut nodes = 1;
                for item in items.iter() {
                    let (item, count) = self.lower_type(item, span, depth + 1)?;
                    nodes += count;
                    if nodes > 4_096 {
                        return Err(self.error(span, "type expansion limit exceeded (4,096 nodes)"));
                    }
                    fields.push(item);
                }
                return Ok((Type::Tuple { items: fields }, nodes));
            }
            ty::Array(element, count) => {
                let len = count.try_to_target_usize(self.tcx)
                    .ok_or_else(|| self.error(span, "array length must be a known constant"))? as usize;
                if len > 256 {
                    return Err(self.error(span, "arrays are limited to 256 elements"));
                }
                if *element == self.tcx.types.u8 {
                    Type::Bytes { len }
                } else {
                    let (element, nodes) = self.lower_type(*element, span, depth + 1)?;
                    let nodes = nodes * len + 1;
                    if nodes > 4_096 {
                        return Err(self.error(span, "type expansion limit exceeded (4,096 nodes)"));
                    }
                    return Ok((Type::Tuple { items: vec![element; len] }, nodes));
                }
            }
            ty::Adt(..) => {
                let inner = self.transparent_inner(ty, span)?.ok_or_else(|| self.error(span, "expected a transparent newtype"))?;
                return self.lower_type(inner, span, depth + 1);
            }
            _ => return Err(self.error(span, format!("unsupported Rust type `{ty}`; use bool, fixed-width integers, tuples or fixed arrays"))),
        };
        Ok((scalar, 1))
    }

    pub fn function(
        &self,
        id: DefId,
        arguments: Vec<Expr>,
        stack: &mut Vec<DefId>,
    ) -> Result<Expr, String> {
        let span = self.tcx.def_span(id);
        if !id.is_local() && !self.allowed_external(id) {
            return Err(self.error(
                span,
                "only local or oxide_sdk non-generic helper functions are supported",
            ));
        }
        if !self.tcx.is_mir_available(id) {
            return Err(self.error(
                span,
                "MIR is unavailable; rebuild target dependencies through cargo oxide",
            ));
        }
        if stack.contains(&id) || stack.len() >= 32 {
            return Err(self.error(span, "recursion and call depths beyond 32 are unsupported"));
        }
        if self.tcx.generics_of(id).count() != 0 {
            return Err(self.error(span, "generic puzzle functions are unsupported"));
        }
        let signature = self.tcx.fn_sig(id).instantiate_identity().skip_binder();
        if signature.abi() != rustc_abi::ExternAbi::Rust {
            return Err(self.error(span, "only ordinary Rust function ABI is supported; FFI exports and calls are unsupported"));
        }
        if signature.safety().is_unsafe() {
            return Err(self.error(span, "unsafe functions are unsupported"));
        }
        let body = self.tcx.optimized_mir(id);
        if body.arg_count > 64 || body.local_decls.len() > 2_048 {
            return Err(self.error(span, "function exceeds 64 parameters or 2,048 MIR locals"));
        }
        if arguments.len() != body.arg_count {
            return Err(self.error(span, "internal helper argument count mismatch"));
        }
        self.ty(body.return_ty(), span)?;
        let mut locals = vec![None; body.local_decls.len()];
        for (local, value) in body.args_iter().zip(arguments) {
            self.check_expression(&value, span)?;
            self.ty(body.local_decls[local].ty, span)?;
            locals[local.index()] = Some(value);
        }
        stack.push(id);
        let result = self.block(body, START_BLOCK, locals, &mut Vec::new(), stack);
        stack.pop();
        result
    }

    fn block(
        &self,
        body: &Body<'tcx>,
        block: BasicBlock,
        mut locals: Vec<Option<Expr>>,
        path: &mut Vec<BasicBlock>,
        stack: &mut Vec<DefId>,
    ) -> Result<Expr, String> {
        let data = &body.basic_blocks[block];
        let span = data.terminator().source_info.span;
        if self.active_blocks.get() >= MAX_EXPRESSION_DEPTH {
            return Err(self.error(span, "active control-flow depth limit exceeded (64 blocks)"));
        }
        self.active_blocks.set(self.active_blocks.get() + 1);
        let _depth = BlockDepth(&self.active_blocks);
        self.blocks.set(self.blocks.get() + 1);
        if self.blocks.get() > 10_000 {
            return Err(self.error(span, "MIR expansion exceeded 10,000 blocks"));
        }
        if path.contains(&block) {
            return Err(self.error(
                span,
                "loops are unsupported; the puzzle control flow must be acyclic",
            ));
        }
        path.push(block);
        let mut effects = Vec::new();
        for statement in &data.statements {
            let statement_span = statement.source_info.span;
            match &statement.kind {
                StatementKind::Assign(assignment) => {
                    let (place, rvalue) = &**assignment;
                    let value = self.rvalue(body, rvalue, &locals, statement_span)?;
                    self.check_expression(&value, statement_span)?;
                    if may_reject(&value) {
                        effects.push(value.clone());
                    }
                    self.assign(body, *place, value, &mut locals, statement_span)?;
                }
                StatementKind::StorageLive(_)
                | StatementKind::StorageDead(_)
                | StatementKind::Nop
                | StatementKind::ConstEvalCounter
                | StatementKind::PlaceMention(_) => {}
                other => {
                    return Err(self.error(
                        statement_span,
                        format!("unsupported MIR statement: {other:?}"),
                    ));
                }
            }
        }
        let terminator = data.terminator();
        let result = match &terminator.kind {
            TerminatorKind::Return => {
                self.place(body, Place::from(Local::from_usize(0)), &locals, span)?
            }
            TerminatorKind::Goto { target } => self.block(body, *target, locals, path, stack)?,
            TerminatorKind::SwitchInt { discr, targets } => {
                let value = self.operand(body, discr, &locals, span)?;
                let ty = self.ty(discr.ty(&body.local_decls, self.tcx), span)?;
                let mut result = self.block(
                    body,
                    targets.otherwise(),
                    self.clone_locals(&locals, span)?,
                    &mut path.clone(),
                    stack,
                )?;
                for (bits, target) in targets.iter().collect::<Vec<_>>().into_iter().rev() {
                    let condition = Expr::Binary {
                        kind: BinaryOp::Eq,
                        left: Box::new(value.clone()),
                        right: Box::new(self.scalar(bits, &ty, span)?),
                        ty: ty.clone(),
                    };
                    result = Expr::If {
                        condition: Box::new(condition),
                        then_value: Box::new(self.block(
                            body,
                            target,
                            self.clone_locals(&locals, span)?,
                            &mut path.clone(),
                            stack,
                        )?),
                        else_value: Box::new(result),
                    };
                    self.check_expression(&result, span)?;
                }
                result
            }
            TerminatorKind::Assert {
                cond,
                expected,
                target,
                ..
            } => {
                let mut condition = self.operand(body, cond, &locals, span)?;
                if !expected {
                    condition = Expr::Not {
                        value: Box::new(condition),
                    };
                }
                Expr::If {
                    condition: Box::new(condition),
                    then_value: Box::new(self.block(body, *target, locals, path, stack)?),
                    else_value: Box::new(Expr::Reject {
                        message: self.error(span, "Rust assertion failed"),
                    }),
                }
            }
            TerminatorKind::Call {
                func,
                args,
                destination,
                target,
                ..
            } => {
                let fn_ty = func.ty(&body.local_decls, self.tcx);
                let ty::FnDef(id, generic_args) = fn_ty.kind() else {
                    return Err(self.error(span, "indirect function calls are unsupported"));
                };
                let name = self.tcx.def_path_str(*id);
                if name == "core::panicking::panic"
                    || name == "core::panicking::panic_fmt"
                    || name == "core::panicking::panic_explicit"
                {
                    Expr::Reject {
                        message: self.error(span, "Rust panic/assertion rejects the puzzle"),
                    }
                } else {
                    if !generic_args.skip_binder().is_empty() {
                        return Err(
                            self.error(span, format!("generic call `{name}` is unsupported"))
                        );
                    }
                    if !id.is_local() && !self.allowed_external(*id) {
                        return Err(self.error(
                            span,
                            format!(
                                "external call `{name}` is unsupported; use local pure helpers"
                            ),
                        ));
                    }
                    if args.len() > 64 {
                        return Err(self.error(span, "calls are limited to 64 arguments"));
                    }
                    let mut values = Vec::new();
                    let mut nodes = 0;
                    for arg in args {
                        let value = self.operand(body, &arg.node, &locals, arg.span)?;
                        nodes += self.expression_size(&value, arg.span)?;
                        if nodes > MAX_EXPRESSION_NODES * 4 {
                            return Err(
                                self.error(span, "helper argument expansion limit exceeded")
                            );
                        }
                        values.push(value);
                    }
                    let value = self.function(*id, values, stack)?;
                    effects.push(value.clone());
                    self.assign(body, *destination, value, &mut locals, span)?;
                    let target = target.ok_or_else(|| {
                        self.error(span, "diverging helper calls are unsupported")
                    })?;
                    self.block(body, target, locals, path, stack)?
                }
            }
            TerminatorKind::Unreachable => Expr::Reject {
                message: self.error(span, "unreachable Rust path"),
            },
            other => return Err(self.error(span, format!("unsupported MIR terminator: {other:?}"))),
        };
        path.pop();
        let mut result = result;
        for first in effects.into_iter().rev() {
            result = Expr::Sequence {
                first: Box::new(first),
                then_value: Box::new(result),
            };
            self.check_expression(&result, span)?;
        }
        self.check_expression(&result, span)?;
        Ok(result)
    }

    fn operand(
        &self,
        body: &Body<'tcx>,
        operand: &Operand<'tcx>,
        locals: &[Option<Expr>],
        span: Span,
    ) -> Result<Expr, String> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.place(body, *place, locals, span),
            Operand::Constant(constant) => {
                let ty = self.ty(constant.const_.ty(), span)?;
                if ty == Type::Unit {
                    return Ok(Expr::Unit);
                }
                let bits = constant
                    .const_
                    .try_eval_bits(self.tcx, ty::TypingEnv::fully_monomorphized())
                    .ok_or_else(|| {
                        self.error(
                            span,
                            "only scalar compile-time constants are currently supported",
                        )
                    })?;
                self.scalar(bits, &ty, span)
            }
            _ => Err(self.error(span, "unsupported MIR operand")),
        }
    }

    fn scalar(&self, bits: u128, ty: &Type, span: Span) -> Result<Expr, String> {
        Ok(match ty {
            Type::Bool => Expr::Bool { value: bits != 0 },
            Type::Int {
                bits: width,
                signed,
            } => {
                let value = if *signed {
                    let shift = 128 - *width;
                    (((bits << shift) as i128) >> shift).to_string()
                } else {
                    bits.to_string()
                };
                Expr::Int {
                    value,
                    ty: ty.clone(),
                }
            }
            _ => return Err(self.error(span, "switch/constant value must be scalar")),
        })
    }

    fn place(
        &self,
        body: &Body<'tcx>,
        place: Place<'tcx>,
        locals: &[Option<Expr>],
        span: Span,
    ) -> Result<Expr, String> {
        let mut value = locals[place.local.index()]
            .clone()
            .ok_or_else(|| self.error(span, "MIR reads an uninitialized local"))?;
        let mut current_ty = body.local_decls[place.local].ty;
        for projection in place.projection {
            match projection {
                ProjectionElem::Field(index, field_ty) => {
                    if self.transparent_inner(current_ty, span)?.is_some() {
                        if index.index() != 0 {
                            return Err(self.error(span, "transparent newtypes have only field 0"));
                        }
                    } else {
                        value = match value {
                            Expr::Tuple { items } => items
                                .get(index.index())
                                .cloned()
                                .ok_or_else(|| self.error(span, "invalid tuple field"))?,
                            other => Expr::Field {
                                value: Box::new(other),
                                index: index.index(),
                            },
                        };
                    }
                    current_ty = field_ty;
                }
                ProjectionElem::ConstantIndex {
                    offset,
                    from_end: false,
                    ..
                } => {
                    value = match value {
                        Expr::Tuple { items } => items
                            .get(offset as usize)
                            .cloned()
                            .ok_or_else(|| self.error(span, "invalid constant array index"))?,
                        other => Expr::Field {
                            value: Box::new(other),
                            index: offset as usize,
                        },
                    };
                    let ty::Array(element, _) = current_ty.kind() else {
                        return Err(self.error(span, "constant indexing requires a fixed array"));
                    };
                    current_ty = *element;
                }
                _ => {
                    return Err(self.error(
                        span,
                        format!("unsupported place projection: {projection:?}"),
                    ));
                }
            }
        }
        Ok(value)
    }

    fn assign(
        &self,
        body: &Body<'tcx>,
        place: Place<'tcx>,
        value: Expr,
        locals: &mut [Option<Expr>],
        span: Span,
    ) -> Result<(), String> {
        if place.projection.is_empty() {
            locals[place.local.index()] = Some(value);
            return Ok(());
        }
        let mut current_ty = body.local_decls[place.local].ty;
        let mut fields = Vec::new();
        for projection in place.projection {
            match projection {
                ProjectionElem::Field(index, field_ty) => {
                    if self.transparent_inner(current_ty, span)?.is_some() {
                        if index.index() != 0 {
                            return Err(self.error(span, "transparent newtypes have only field 0"));
                        }
                    } else {
                        fields.push(index.index());
                    }
                    current_ty = field_ty;
                }
                _ => {
                    return Err(
                        self.error(span, "only tuple/newtype-field assignment is supported")
                    );
                }
            }
        }
        let ty = self.ty(body.local_decls[place.local].ty, span)?;
        let base = locals[place.local.index()]
            .take()
            .unwrap_or_else(|| empty_value(&ty));
        let result = self.assign_field(base, &fields, value, span)?;
        self.check_expression(&result, span)?;
        locals[place.local.index()] = Some(result);
        Ok(())
    }

    fn assign_field(
        &self,
        base: Expr,
        fields: &[usize],
        value: Expr,
        span: Span,
    ) -> Result<Expr, String> {
        let Some((&index, remaining)) = fields.split_first() else {
            return Ok(value);
        };
        let Expr::Tuple { mut items } = base else {
            return Err(self.error(
                span,
                "assignment into an input tuple is unsupported; bind a new tuple",
            ));
        };
        let slot = items
            .get_mut(index)
            .ok_or_else(|| self.error(span, "invalid assignment field"))?;
        *slot = self.assign_field(slot.clone(), remaining, value, span)?;
        Ok(Expr::Tuple { items })
    }

    fn rvalue(
        &self,
        body: &Body<'tcx>,
        value: &Rvalue<'tcx>,
        locals: &[Option<Expr>],
        span: Span,
    ) -> Result<Expr, String> {
        match value {
            Rvalue::Use(operand, _) => self.operand(body, operand, locals, span),
            Rvalue::BinaryOp(op, operands) => {
                let (left, right) = &**operands;
                let left_ty = self.ty(left.ty(&body.local_decls, self.tcx), span)?;
                let kind = match op {
                    BinOp::Eq => BinaryOp::Eq,
                    BinOp::Ne => BinaryOp::Ne,
                    BinOp::Lt => BinaryOp::Lt,
                    BinOp::Le => BinaryOp::Le,
                    BinOp::Gt => BinaryOp::Gt,
                    BinOp::Ge => BinaryOp::Ge,
                    BinOp::Add | BinOp::AddWithOverflow => BinaryOp::Add,
                    BinOp::Sub | BinOp::SubWithOverflow => BinaryOp::Sub,
                    BinOp::Mul | BinOp::MulWithOverflow => BinaryOp::Mul,
                    _ => return Err(self.error(span, format!("unsupported integer operation `{op:?}`; wrapping, division, shifts and bitwise operations need distinct verified semantics"))),
                };
                let expression = Expr::Binary {
                    kind,
                    left: Box::new(self.operand(body, left, locals, span)?),
                    right: Box::new(self.operand(body, right, locals, span)?),
                    ty: left_ty,
                };
                // Source arithmetic is compiled with overflow checks. The IR's
                // arithmetic rejects overflow itself, so a successful expression
                // has a false overflow flag. Explicit overflowing_* methods are
                // external calls and rejected, rather than miscompiled as checked.
                if matches!(
                    op,
                    BinOp::AddWithOverflow | BinOp::SubWithOverflow | BinOp::MulWithOverflow
                ) {
                    Ok(Expr::Tuple {
                        items: vec![expression, Expr::Bool { value: false }],
                    })
                } else {
                    Ok(expression)
                }
            }
            Rvalue::UnaryOp(UnOp::Not, operand)
                if operand.ty(&body.local_decls, self.tcx).is_bool() =>
            {
                Ok(Expr::Not {
                    value: Box::new(self.operand(body, operand, locals, span)?),
                })
            }
            Rvalue::Aggregate(kind, operands) => {
                let mut items = Vec::new();
                let mut count = 1;
                for operand in operands {
                    let value = self.operand(body, operand, locals, span)?;
                    count += self.expression_size(&value, span)?;
                    if count > MAX_EXPRESSION_NODES {
                        return Err(
                            self.error(span, "aggregate expression expansion limit exceeded")
                        );
                    }
                    items.push(value);
                }
                match &**kind {
                    AggregateKind::Tuple if items.is_empty() => Ok(Expr::Unit),
                    AggregateKind::Tuple => Ok(Expr::Tuple { items }),
                    AggregateKind::Array(element) if *element == self.tcx.types.u8 => {
                        self.byte_array(items, span)
                    }
                    AggregateKind::Array(_) => Ok(Expr::Tuple { items }),
                    AggregateKind::Adt(id, _, args, _, _) => {
                        let rust_ty = Ty::new_adt(self.tcx, self.tcx.adt_def(*id), args);
                        self.transparent_inner(rust_ty, span)?.ok_or_else(|| {
                            self.error(span, "expected a transparent newtype aggregate")
                        })?;
                        self.ty(rust_ty, span)?;
                        let [value] = items.try_into().map_err(|_| {
                            self.error(
                                span,
                                "transparent newtype aggregate requires exactly one field",
                            )
                        })?;
                        Ok(value)
                    }
                    _ => Err(self.error(span, "structs, enums and closures are not yet supported")),
                }
            }
            Rvalue::Repeat(operand, count) => {
                let len = count
                    .try_to_target_usize(self.tcx)
                    .ok_or_else(|| self.error(span, "array repeat length is not constant"))?
                    as usize;
                if len > 256 {
                    return Err(self.error(span, "arrays are limited to 256 elements"));
                }
                let value = self.operand(body, operand, locals, span)?;
                if self.expression_size(&value, span)?.saturating_mul(len) > MAX_EXPRESSION_NODES {
                    return Err(self.error(span, "repeated expression expansion limit exceeded"));
                }
                if operand.ty(&body.local_decls, self.tcx) == self.tcx.types.u8 {
                    self.byte_array(vec![value; len], span)
                } else {
                    Ok(Expr::Tuple {
                        items: vec![value; len],
                    })
                }
            }
            _ => Err(self.error(span, format!("unsupported MIR value: {value:?}"))),
        }
    }

    fn byte_array(&self, values: Vec<Expr>, span: Span) -> Result<Expr, String> {
        let mut hex = String::new();
        for value in values {
            let Expr::Int { value, .. } = value else {
                return Err(self.error(
                    span,
                    "constructing byte arrays requires constant byte elements",
                ));
            };
            let byte: u8 = value
                .parse()
                .map_err(|_| self.error(span, "invalid constant byte"))?;
            hex.push_str(&format!("{byte:02x}"));
        }
        Ok(Expr::Bytes { hex })
    }
}

fn empty_value(ty: &Type) -> Expr {
    match ty {
        Type::Tuple { items } => Expr::Tuple {
            items: items.iter().map(empty_value).collect(),
        },
        Type::Unit => Expr::Unit,
        _ => Expr::Reject {
            message: "uninitialized MIR field".into(),
        },
    }
}

fn may_reject(expr: &Expr) -> bool {
    match expr {
        Expr::Binary {
            kind: BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul,
            ..
        }
        | Expr::Reject { .. } => true,
        Expr::Tuple { items } => items.iter().any(may_reject),
        Expr::Binary { left, right, .. } => may_reject(left) || may_reject(right),
        Expr::Not { value } | Expr::Field { value, .. } => may_reject(value),
        Expr::If { .. } | Expr::Sequence { .. } => true,
        _ => false,
    }
}
