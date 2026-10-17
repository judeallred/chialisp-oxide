use super::*;

impl<'tcx> Encode<'tcx> {
    pub(super) fn raw_graph(&mut self, start: Raw<'tcx>) -> R {
        let mut pending = vec![(start, 0usize)];
        while let Some((node, depth)) = pending.pop() {
            self.charge(1)?;
            if depth >= 32 {
                return Err("raw type/constant depth bound".into());
            }
            let mut push = |this: &mut Self, item: Raw<'tcx>| -> R {
                this.charge(1)?;
                if pending.len() >= 4096 {
                    return Err("raw pending bound".into());
                }
                pending.push((item, depth + 1));
                Ok(())
            };
            match node {
                Raw::Ty(t) => match t.kind() {
                    ty::Bool | ty::Never | ty::Int(_) | ty::Uint(_) => (),
                    ty::Array(element, length) => {
                        push(self, Raw::Ty(*element))?;
                        push(self, Raw::Const(*length))?;
                    }
                    ty::Slice(element) | ty::Ref(_, element, _) => push(self, Raw::Ty(*element))?,
                    ty::Tuple(items) => {
                        if items.len() > 64 {
                            return Err("raw tuple bound".into());
                        }
                        for item in items.iter() {
                            push(self, Raw::Ty(item))?
                        }
                    }
                    ty::FnDef(_, args) => push(self, Raw::Args(args.skip_binder()))?,
                    ty::Param(parameter) => {
                        let args = self.instance.ok_or("raw parameter outside instance")?.args;
                        let item = args
                            .get(parameter.index as usize)
                            .and_then(|a| a.as_type())
                            .ok_or("unbound raw type parameter")?;
                        push(self, Raw::Ty(item))?
                    }
                    _ => return Err("raw alias/complex type is unreviewed".into()),
                },
                Raw::Args(args) => {
                    if args.len() > 64 {
                        return Err("raw generic-argument bound".into());
                    }
                    for arg in args.iter() {
                        match arg.kind() {
                            ty::GenericArgKind::Lifetime(_) => (),
                            ty::GenericArgKind::Type(t) => push(self, Raw::Ty(t))?,
                            ty::GenericArgKind::Const(c) => push(self, Raw::Const(c))?,
                        }
                    }
                }
                Raw::Const(c) => match c.kind() {
                    ty::ConstKind::Value(value) => {
                        push(self, Raw::Ty(value.ty))?;
                        match **value.valtree {
                            ty::ValTreeKind::Leaf(_) => (),
                            ty::ValTreeKind::Branch(items) => {
                                if items.len() > 4096 {
                                    return Err("raw constant branch bound".into());
                                }
                                for item in items {
                                    push(self, Raw::Const(item))?
                                }
                            }
                        }
                    }
                    ty::ConstKind::Param(parameter) => {
                        let args = self.instance.ok_or("raw constant outside instance")?.args;
                        let item = args
                            .get(parameter.index as usize)
                            .and_then(|a| a.as_const())
                            .ok_or("unbound raw constant parameter")?;
                        push(self, Raw::Const(item))?
                    }
                    _ => return Err("raw constant alias/expression is unreviewed".into()),
                },
            }
        }
        Ok(())
    }
    pub(super) fn literal_count(&mut self, c: ty::Const<'tcx>) -> R<u64> {
        self.raw_graph(Raw::Const(c))?;
        let ty::ConstKind::Value(v) = c.kind() else {
            return Err("CTFE nonliteral count unreviewed".into());
        };
        if v.ty != self.tcx.types.usize {
            return Err("CTFE count must be usize".into());
        }
        let ty::ValTreeKind::Leaf(value) = **v.valtree else {
            return Err("CTFE nonscalar count".into());
        };
        if value.size() != self.tcx.data_layout.pointer_size() {
            return Err("CTFE count size mismatch".into());
        }
        let count = value.to_bits(value.size());
        if count > 4096 {
            return Err("CTFE array bound".into());
        }
        Ok(count as u64)
    }
    pub(super) fn literal_size(&mut self, t: Ty<'tcx>, depth: usize) -> R<u64> {
        self.charge(1)?;
        if depth >= 32 {
            return Err("CTFE type depth".into());
        }
        let bytes = match t.kind() {
            ty::Bool => 1,
            ty::Int(i) => match i {
                ty::IntTy::I8 => 1,
                ty::IntTy::I16 => 2,
                ty::IntTy::I32 => 4,
                ty::IntTy::I64 => 8,
                ty::IntTy::I128 => 16,
                ty::IntTy::Isize => self.tcx.data_layout.pointer_size().bytes(),
            },
            ty::Uint(i) => match i {
                ty::UintTy::U8 => 1,
                ty::UintTy::U16 => 2,
                ty::UintTy::U32 => 4,
                ty::UintTy::U64 => 8,
                ty::UintTy::U128 => 16,
                ty::UintTy::Usize => self.tcx.data_layout.pointer_size().bytes(),
            },
            ty::Array(element, count) => {
                let count = self.literal_count(*count)?;
                let size = self.literal_size(*element, depth + 1)?;
                count.checked_mul(size).ok_or("CTFE byte-size overflow")?
            }
            ty::Tuple(items) if items.is_empty() => 0,
            _ => return Err("CTFE nonliteral local type unreviewed".into()),
        };
        if bytes > 16384 {
            return Err("CTFE byte-size bound".into());
        }
        Ok(bytes)
    }
    pub(super) fn literal_ctfe_input(&mut self, uv: mir::UnevaluatedConst<'tcx>) -> R {
        self.raw_graph(Raw::Args(uv.args))?;
        if uv.promoted.is_some() || !uv.args.is_empty() {
            return Err("promoted/generic CTFE input is unreviewed".into());
        }
        if !matches!(
            self.tcx.def_kind(uv.def),
            rustc_hir::def::DefKind::Const { .. } | rustc_hir::def::DefKind::AnonConst
        ) {
            return Err("nonliteral CTFE source kind is unreviewed".into());
        }
        let body = self.tcx.mir_for_ctfe(uv.def);
        self.preflight(body)?;
        if body.basic_blocks.len() != 1
            || body.arg_count != 0
            || body.local_decls.len() > 64
            || !matches!(
                body.basic_blocks[mir::START_BLOCK].terminator().kind,
                mir::TerminatorKind::Return
            )
        {
            return Err("CTFE loops/calls/control flow are unreviewed".into());
        }
        for local in &body.local_decls {
            self.raw_graph(Raw::Ty(local.ty))?;
            self.literal_size(local.ty, 0)?;
        }
        for statement in &body.basic_blocks[mir::START_BLOCK].statements {
            match &statement.kind {
                mir::StatementKind::Assign(value) => {
                    if !value.0.projection.is_empty() {
                        return Err("CTFE projected mutation unreviewed".into());
                    }
                    match &value.1 {
                        mir::Rvalue::Aggregate(kind, operands) => {
                            if operands.len() > 4096 {
                                return Err("raw CTFE aggregate bound".into());
                            }
                            match &**kind {
                                mir::AggregateKind::Array(t) => self.raw_graph(Raw::Ty(*t))?,
                                mir::AggregateKind::Tuple => (),
                                _ => return Err("CTFE aggregate unreviewed".into()),
                            }
                            for operand in operands {
                                self.literal_ctfe_operand(operand)?;
                            }
                        }
                        mir::Rvalue::Use(operand, _) => self.literal_ctfe_operand(operand)?,
                        mir::Rvalue::Repeat(operand, length) => {
                            self.literal_ctfe_operand(operand)?;
                            self.raw_graph(Raw::Const(*length))?;
                            self.literal_count(*length)?;
                        }
                        _ => return Err("nonliteral CTFE computation is unreviewed".into()),
                    }
                }
                mir::StatementKind::StorageLive(_)
                | mir::StatementKind::StorageDead(_)
                | mir::StatementKind::Nop
                | mir::StatementKind::ConstEvalCounter => (),
                _ => return Err("CTFE statement unreviewed".into()),
            }
        }
        // No hidden unevaluated dependency or call can precede c.eval.
        if body.required_consts.as_ref().is_some_and(|v| !v.is_empty())
            || body.mentioned_items.as_ref().is_some_and(|v| !v.is_empty())
        {
            return Err("CTFE required/mentioned dependency unreviewed".into());
        }
        Ok(())
    }
    pub(super) fn literal_ctfe_operand(&mut self, operand: &mir::Operand<'tcx>) -> R {
        let mir::Operand::Constant(c) = operand else {
            return Err("CTFE nonliteral operand unreviewed".into());
        };
        self.raw_graph(Raw::Ty(c.const_.ty()))?;
        match c.const_ {
            mir::Const::Val(_, _) => Ok(()),
            mir::Const::Ty(_, value) => self.raw_graph(Raw::Const(value)),
            _ => Err("nested unevaluated CTFE dependency unreviewed".into()),
        }
    }
}
