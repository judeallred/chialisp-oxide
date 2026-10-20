use super::*;

impl<'tcx> Encode<'tcx> {
    pub(super) fn function_operand(
        &mut self,
        body: &mir::Body<'tcx>,
        operand: &mir::Operand<'tcx>,
    ) -> R {
        let function_ty = self.normal_ty(operand.ty(&body.local_decls, self.tcx))?;
        let ty::FnDef(def, args) = function_ty.kind() else {
            return Err("indirect calls unencoded".into());
        };
        let instance = ty::Instance::try_resolve(
            self.tcx,
            ty::TypingEnv::fully_monomorphized(),
            *def,
            args.skip_binder(),
        )
        .map_err(|_| "call resolution error")?
        .ok_or("unresolved call")?;
        self.instance_ref(instance)
    }
    pub(super) fn place(&mut self, p: mir::Place<'tcx>) -> R {
        if p.projection.len() > 32 {
            return Err("place depth".into());
        }
        self.usize(p.local.as_usize())?;
        self.usize(p.projection.len())?;
        for p in p.projection.iter() {
            self.charge(1)?;
            match p {
                mir::ProjectionElem::Deref => self.tag(1)?,
                mir::ProjectionElem::Field(i, t) => {
                    self.tag(2)?;
                    self.usize(i.as_usize())?;
                    self.ty(t, 0)?
                }
                mir::ProjectionElem::Index(i) => {
                    self.tag(3)?;
                    self.usize(i.as_usize())?
                }
                mir::ProjectionElem::ConstantIndex {
                    offset,
                    min_length,
                    from_end,
                } => {
                    self.tag(4)?;
                    self.u64(offset)?;
                    self.u64(min_length)?;
                    self.boolean(from_end)?
                }
                _ => return Err(format!("unencoded projection: {p:?}")),
            }
        }
        Ok(())
    }
    pub(super) fn constant(&mut self, c: mir::Const<'tcx>, span: rustc_span::Span) -> R {
        self.charge(1)?;
        self.constants += 1;
        let c = self.normal_const(c)?;
        self.ty(c.ty(), 0)?;
        match c {
            mir::Const::Unevaluated(uv, _) => {
                self.tag(1)?;
                let id = self.id(Node::Constant(uv))?;
                self.usize(id)?
            }
            mir::Const::Ty(_, value) => {
                if !matches!(value.kind(), ty::ConstKind::Value(_)) {
                    return Err("unencoded type-level unevaluated constant".into());
                }
                self.tag(0)?
            }
            mir::Const::Val(_, _) => self.tag(0)?,
        }
        let layout = self
            .tcx
            .layout_of(ty::TypingEnv::fully_monomorphized().as_query_input(c.ty()))
            .map_err(|_| "constant layout")?;
        if layout.size.bytes() > 16384 {
            return Err("constant byte budget".into());
        }
        let v = c
            .eval(self.tcx, ty::TypingEnv::fully_monomorphized(), span)
            .map_err(|_| "constant evaluation")?;
        match v {
            mir::ConstValue::ZeroSized => {
                if layout.size.bytes() != 0 {
                    return Err("bad zero-sized const".into());
                }
                self.tag(0)?
            }
            mir::ConstValue::Scalar(s) => {
                let n = s
                    .try_to_scalar_int()
                    .map_err(|_| "pointer scalar constants unencoded")?;
                if n.size() != layout.size {
                    return Err("scalar layout mismatch".into());
                }
                self.tag(1)?;
                self.raw(&n.to_bits(n.size()).to_le_bytes())?
            }
            mir::ConstValue::Indirect { alloc_id, offset } => {
                let Some(GlobalAlloc::Memory(allocation)) = self.tcx.try_get_global_alloc(alloc_id)
                else {
                    return Err("static/function/vtable allocation unencoded".into());
                };
                let allocation = allocation.inner();
                if allocation.size().bytes() > 16384 {
                    return Err("whole constant allocation bound".into());
                }
                let end = offset
                    .bytes()
                    .checked_add(layout.size.bytes())
                    .ok_or("constant offset overflow")?;
                if end > allocation.size().bytes() {
                    return Err("short allocation".into());
                }
                let range = alloc_range(offset, layout.size);
                if allocation.init_mask().is_range_initialized(range).is_err()
                    || !allocation.provenance().range_empty(range, &self.tcx)
                {
                    return Err("uninitialized/provenance constant".into());
                }
                let b = allocation
                    .get_bytes_strip_provenance(&self.tcx, range)
                    .map_err(|_| "invalid constant bytes")?;
                self.tag(2)?;
                self.usize(b.len())?;
                self.raw(b)?
            }
            mir::ConstValue::Slice { alloc_id, meta } => {
                // The core panic leaf's `&'static str` message. Encode the exact
                // initialized bytes of the whole literal; provenance is rejected.
                let Some(GlobalAlloc::Memory(allocation)) = self.tcx.try_get_global_alloc(alloc_id)
                else {
                    return Err("static/function/vtable slice allocation unencoded".into());
                };
                let allocation = allocation.inner();
                if meta > 4096 || allocation.size().bytes() != meta {
                    return Err("slice constant bound/length mismatch".into());
                }
                let range = alloc_range(rustc_abi::Size::ZERO, allocation.size());
                if allocation.init_mask().is_range_initialized(range).is_err()
                    || !allocation.provenance().range_empty(range, &self.tcx)
                {
                    return Err("uninitialized/provenance slice constant".into());
                }
                let b = allocation
                    .get_bytes_strip_provenance(&self.tcx, range)
                    .map_err(|_| "invalid slice constant bytes")?;
                self.tag(3)?;
                self.usize(b.len())?;
                self.raw(b)?
            }
        }
        Ok(())
    }
    pub(super) fn operand(&mut self, o: &mir::Operand<'tcx>) -> R {
        match o {
            mir::Operand::Copy(p) => {
                self.tag(1)?;
                self.place(*p)
            }
            mir::Operand::Move(p) => {
                self.tag(2)?;
                self.place(*p)
            }
            mir::Operand::Constant(c) => {
                self.tag(3)?;
                self.constant(c.const_, c.span)
            }
            mir::Operand::RuntimeChecks(_) => Err("runtime-check query operands unencoded".into()),
        }
    }
    pub(super) fn binop(&mut self, b: mir::BinOp) -> R {
        use mir::BinOp::*;
        self.tag(match b {
            Add => 1,
            Sub => 2,
            Mul => 3,
            Div => 4,
            Rem => 5,
            BitXor => 6,
            BitAnd => 7,
            BitOr => 8,
            Shl => 9,
            Shr => 10,
            Eq => 11,
            Lt => 12,
            Le => 13,
            Ne => 14,
            Ge => 15,
            Gt => 16,
            AddWithOverflow => 17,
            SubWithOverflow => 18,
            MulWithOverflow => 19,
            _ => return Err(format!("unencoded binary operation: {b:?}")),
        })
    }
    pub(super) fn rvalue(&mut self, r: &mir::Rvalue<'tcx>) -> R {
        match r {
            mir::Rvalue::Use(o, retag) => {
                self.tag(1)?;
                self.boolean(retag.yes())?;
                self.operand(o)
            }
            mir::Rvalue::Repeat(o, n) => {
                self.tag(2)?;
                let length = self
                    .normal_ty_const(*n)?
                    .try_to_target_usize(self.tcx)
                    .ok_or("repeat length")?;
                self.u64(length)?;
                self.operand(o)
            }
            mir::Rvalue::Ref(_, kind, p) => {
                self.tag(3)?;
                self.tag(match kind {
                    mir::BorrowKind::Shared => 0,
                    mir::BorrowKind::Mut {
                        kind: mir::MutBorrowKind::Default,
                    } => 1,
                    mir::BorrowKind::Mut {
                        kind: mir::MutBorrowKind::TwoPhaseBorrow,
                    } => 2,
                    _ => return Err("unencoded borrow kind".into()),
                })?;
                self.place(*p)
            }
            mir::Rvalue::Cast(mir::CastKind::IntToInt, o, t) => {
                self.tag(4)?;
                self.operand(o)?;
                self.ty(*t, 0)
            }
            mir::Rvalue::BinaryOp(b, ops) => {
                self.tag(5)?;
                self.binop(*b)?;
                self.operand(&ops.0)?;
                self.operand(&ops.1)
            }
            mir::Rvalue::UnaryOp(b, o) => {
                self.tag(6)?;
                self.tag(match b {
                    mir::UnOp::Not => 0,
                    mir::UnOp::Neg => 1,
                    mir::UnOp::PtrMetadata => 2,
                })?;
                self.operand(o)
            }
            mir::Rvalue::Aggregate(kind, operands) => {
                self.tag(7)?;
                match &**kind {
                    mir::AggregateKind::Array(t) => {
                        self.tag(0)?;
                        self.ty(*t, 0)?
                    }
                    mir::AggregateKind::Tuple => self.tag(1)?,
                    _ => return Err("unencoded aggregate".into()),
                }
                if operands.len() > 4096 {
                    return Err("aggregate size".into());
                }
                self.usize(operands.len())?;
                for o in operands {
                    self.operand(o)?
                }
                Ok(())
            }
            _ => Err(format!("unencoded rvalue: {r:?}")),
        }
    }
}
