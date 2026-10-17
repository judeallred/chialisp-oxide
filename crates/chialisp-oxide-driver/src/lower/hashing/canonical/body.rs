use super::*;

impl<'tcx> Encode<'tcx> {
    pub(super) fn unwind(&mut self, u: mir::UnwindAction) -> R {
        match u {
            mir::UnwindAction::Continue => self.tag(0),
            mir::UnwindAction::Unreachable => self.tag(1),
            mir::UnwindAction::Cleanup(b) => {
                self.tag(2)?;
                self.usize(b.as_usize())
            }
            mir::UnwindAction::Terminate(r) => {
                self.tag(3)?;
                self.tag(match r {
                    mir::UnwindTerminateReason::Abi => 0,
                    mir::UnwindTerminateReason::InCleanup => 1,
                })
            }
        }
    }
    pub(super) fn assertion(&mut self, a: &mir::AssertMessage<'tcx>) -> R {
        use mir::AssertKind::*;
        match a {
            BoundsCheck { len, index } => {
                self.tag(0)?;
                self.operand(len)?;
                self.operand(index)
            }
            Overflow(op, a, b) => {
                self.tag(1)?;
                self.binop(*op)?;
                self.operand(a)?;
                self.operand(b)
            }
            OverflowNeg(a) => {
                self.tag(2)?;
                self.operand(a)
            }
            DivisionByZero(a) => {
                self.tag(3)?;
                self.operand(a)
            }
            RemainderByZero(a) => {
                self.tag(4)?;
                self.operand(a)
            }
            _ => Err("unencoded assertion".into()),
        }
    }
    pub(super) fn preflight(&mut self, body: &mir::Body<'tcx>) -> R {
        if body.local_decls.len() > MAX_LOCALS || body.basic_blocks.len() > MAX_BLOCKS {
            return Err("preclone graph/local budget".into());
        }
        self.charge(body.local_decls.len() + body.basic_blocks.len())?;
        let mut count = 0usize;
        for block in body.basic_blocks.iter() {
            if block.statements.len() > MAX_STATEMENTS {
                return Err("preclone statement budget".into());
            }
            count = count
                .checked_add(block.statements.len())
                .ok_or("preclone statement overflow")?;
            if count > MAX_STATEMENTS {
                return Err("preclone statement budget".into());
            }
            self.charge(block.statements.len())?
        }
        Ok(())
    }
    pub(super) fn body(&mut self, body: &mir::Body<'tcx>) -> R {
        self.charge(1 + body.basic_blocks.len() + body.local_decls.len())?;
        if body.basic_blocks.len() > MAX_BLOCKS || body.local_decls.len() > MAX_LOCALS {
            return Err("MIR graph/local budget".into());
        }
        let count = body
            .basic_blocks
            .iter()
            .try_fold(0usize, |n, b| n.checked_add(b.statements.len()))
            .ok_or("statement overflow")?;
        if count > MAX_STATEMENTS {
            return Err("MIR statement budget".into());
        }
        self.charge(count)?;
        self.blocks += body.basic_blocks.len();
        self.statements += count;
        if body.tainted_by_errors.is_some()
            || body.coroutine.is_some()
            || body.spread_arg.is_some()
            || body.injection_phase.is_some()
            || body.coverage_info_hi.is_some()
            || body.function_coverage_info.is_some()
        {
            return Err("unencoded MIR extension/error".into());
        }
        self.usize(body.arg_count)?;
        self.usize(body.local_decls.len())?;
        for local in &body.local_decls {
            self.boolean(local.mutability.is_mut())?;
            self.ty(local.ty, 0)?
        }
        self.usize(body.basic_blocks.len())?;
        for block in body.basic_blocks.iter() {
            self.boolean(block.is_cleanup)?;
            self.usize(block.statements.len())?;
            for statement in &block.statements {
                match &statement.kind {
                    mir::StatementKind::Assign(value) => {
                        self.tag(1)?;
                        self.place(value.0)?;
                        self.rvalue(&value.1)?
                    }
                    mir::StatementKind::StorageLive(l) => {
                        self.tag(2)?;
                        self.usize(l.as_usize())?
                    }
                    mir::StatementKind::StorageDead(l) => {
                        self.tag(3)?;
                        self.usize(l.as_usize())?
                    }
                    mir::StatementKind::Nop => self.tag(4)?,
                    mir::StatementKind::PlaceMention(p) => {
                        self.tag(5)?;
                        self.place(**p)?
                    }
                    _ => return Err(format!("unencoded statement: {:?}", statement.kind)),
                }
            }
            match &block.terminator().kind {
                mir::TerminatorKind::Return => self.tag(0)?,
                mir::TerminatorKind::Goto { target } => {
                    self.tag(1)?;
                    self.usize(target.as_usize())?
                }
                mir::TerminatorKind::SwitchInt { discr, targets } => {
                    self.tag(2)?;
                    self.operand(discr)?;
                    self.usize(targets.iter().count())?;
                    for (value, target) in targets.iter() {
                        self.raw(&value.to_le_bytes())?;
                        self.usize(target.as_usize())?
                    }
                    self.usize(targets.otherwise().as_usize())?
                }
                mir::TerminatorKind::Assert {
                    cond,
                    expected,
                    msg,
                    target,
                    unwind,
                } => {
                    self.tag(3)?;
                    self.operand(cond)?;
                    self.boolean(*expected)?;
                    self.assertion(msg)?;
                    self.usize(target.as_usize())?;
                    self.unwind(*unwind)?
                }
                mir::TerminatorKind::Call {
                    func,
                    args,
                    destination,
                    target,
                    unwind,
                    ..
                } => {
                    self.tag(4)?;
                    if args.len() > 64 {
                        return Err("call argument bound".into());
                    }
                    self.function_operand(body, func)?;
                    self.usize(args.len())?;
                    for arg in args {
                        self.operand(&arg.node)?
                    }
                    self.place(*destination)?;
                    self.boolean(target.is_some())?;
                    if let Some(target) = target {
                        self.usize(target.as_usize())?
                    }
                    self.unwind(*unwind)?
                }
                mir::TerminatorKind::Unreachable => self.tag(5)?,
                mir::TerminatorKind::Drop { .. } => {
                    return Err("Drop cleanup unencoded, never erased".into());
                }
                _ => {
                    return Err(format!(
                        "unencoded terminator: {:?}",
                        block.terminator().kind
                    ));
                }
            }
        }
        match &body.required_consts {
            None => self.tag(0)?,
            Some(constants) => {
                if constants.len() > 4096 {
                    return Err("required-const bound".into());
                }
                self.tag(1)?;
                self.usize(constants.len())?;
                for c in constants {
                    self.constant(c.const_, c.span)?
                }
            }
        }
        match &body.mentioned_items {
            None => self.tag(0)?,
            Some(items) => {
                if items.len() > 4096 {
                    return Err("mentioned bound".into());
                }
                self.tag(1)?;
                self.usize(items.len())?;
                for item in items {
                    match item.node {
                        mir::MentionedItem::Fn(t) => {
                            let t = self.normal_ty(t)?;
                            let ty::FnDef(def, args) = *t.kind() else {
                                return Err("mentioned indirect item".into());
                            };
                            let instance = ty::Instance::try_resolve(
                                self.tcx,
                                ty::TypingEnv::fully_monomorphized(),
                                def,
                                args.skip_binder(),
                            )
                            .map_err(|_| "mentioned resolution")?
                            .ok_or("unresolved mentioned item")?;
                            self.instance_ref(instance)?
                        }
                        _ => return Err("mentioned drop/unsize/closure unencoded".into()),
                    }
                }
            }
        }
        Ok(())
    }
}
