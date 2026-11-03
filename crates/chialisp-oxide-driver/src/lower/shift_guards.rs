//! A compiler-generated signed shift-count overflow guard is not a Rust cast API.
//! Its unsigned metadata is closed before any replacement. The equivalent guard
//! stays at the original Assert; no failure is moved across the success block.
use super::{Environment, Lowerer};
use chialisp_oxide_ir::{BinaryOp, Expr};
use rustc_middle::{
    mir::{
        self, BasicBlock, BinOp, Body, Local, Location, Operand, Place, Rvalue, StatementKind,
        TerminatorKind,
        visit::{PlaceContext, Visitor},
    },
    ty,
};
use rustc_span::Span;
#[derive(Clone)]
pub(super) struct Plan<'tcx> {
    pub skip: [usize; 2],
    pub source: Operand<'tcx>,
    pub width: u16,
}
fn place<'tcx>(operand: &Operand<'tcx>) -> Option<Place<'tcx>> {
    match operand {
        Operand::Copy(p) | Operand::Move(p) => Some(*p),
        _ => None,
    }
}
struct Uses {
    locals: [Local; 2],
    counts: [usize; 2],
    work: usize,
    limit: usize,
}
impl<'tcx> Visitor<'tcx> for Uses {
    fn visit_local(&mut self, local: Local, _context: PlaceContext, _location: Location) {
        self.work = self.work.saturating_add(1);
        for i in 0..2 {
            if local == self.locals[i] {
                self.counts[i] = self.counts[i].saturating_add(1);
            }
        }
    }
    fn visit_place(&mut self, p: &Place<'tcx>, context: PlaceContext, location: Location) {
        self.work = self.work.saturating_add(p.projection.len() + 1);
        if self.work <= self.limit {
            self.super_place(p, context, location);
        }
    }
    fn visit_rvalue(&mut self, value: &Rvalue<'tcx>, location: Location) {
        self.work = self.work.saturating_add(match value {
            Rvalue::Aggregate(_, items) => items.len() + 1,
            _ => 1,
        });
        if self.work <= self.limit {
            self.super_rvalue(value, location);
        }
    }
    fn visit_basic_block_data(&mut self, block: BasicBlock, data: &mir::BasicBlockData<'tcx>) {
        self.work = self.work.saturating_add(1);
        if self.work <= self.limit {
            self.super_basic_block_data(block, data);
        }
    }
}
impl<'tcx> Lowerer<'tcx> {
    pub(super) fn shift_guard(
        &self,
        body: &'tcx Body<'tcx>,
        block: BasicBlock,
    ) -> Option<Plan<'tcx>> {
        // Body identity is valid only inside the immutable Tcxt query arena and
        // this Lowerer's lifetime. Never persist plans or identify one by name.
        let work = self.expression_work.get().saturating_add(1);
        self.expression_work.set(work);
        if work > 500_000 {
            return None;
        }
        let key = (body as *const Body<'tcx> as usize, block);
        if let Some(plan) = self.shift_plans.borrow().get(&key) {
            return plan.clone();
        }
        // Charge before cache growth/copying. A missing plan never authorizes
        // skipped metadata; bounded ordinary cast lowering is a separate route.
        if self.shift_plans.borrow().len() >= 10_000 {
            return None;
        }
        let plan = self.compute_shift_guard(body, block);
        let work = self.expression_work.get().saturating_add(1);
        self.expression_work.set(work);
        if work > 500_000 {
            return None;
        }
        self.shift_plans.borrow_mut().insert(key, plan.clone());
        plan
    }
    fn compute_shift_guard(&self, body: &'tcx Body<'tcx>, block: BasicBlock) -> Option<Plan<'tcx>> {
        // Bound the candidate and full-use scan before allocating any metadata.
        if body.basic_blocks.len() > 64 || body.local_decls.len() > 2048 {
            return None;
        }
        let data = &body.basic_blocks[block];
        let TerminatorKind::Assert {
            cond,
            expected: true,
            msg,
            target,
            ..
        } = &data.terminator().kind
        else {
            return None;
        };
        let mir::AssertKind::Overflow(op, _, count) = &**msg else {
            return None;
        };
        if !matches!(op, BinOp::Shl | BinOp::Shr) {
            return None;
        }
        // Cheap noncandidate checks above avoid unrelated whole-body scans.
        // Charge every block-length visit before the bounded fold itself.
        let scan_work = self
            .expression_work
            .get()
            .saturating_add(body.basic_blocks.len());
        self.expression_work.set(scan_work);
        if scan_work > 500_000 {
            return None;
        }
        let statements = body
            .basic_blocks
            .iter()
            .try_fold(0usize, |n, b| n.checked_add(b.statements.len()))?;
        if statements > 4096 {
            return None;
        }
        let condition = place(cond)?;
        if !condition.projection.is_empty() {
            return None;
        }
        let last = data.statements.len().checked_sub(1)?;
        let before = last.checked_sub(1)?;
        let StatementKind::Assign(compare) = &data.statements[last].kind else {
            return None;
        };
        let (destination, Rvalue::BinaryOp(BinOp::Lt, operands)) = &**compare else {
            return None;
        };
        if *destination != condition {
            return None;
        }
        let (cast_operand, Operand::Constant(limit)) = &**operands else {
            return None;
        };
        let cast_place = place(cast_operand)?;
        if !cast_place.projection.is_empty() {
            return None;
        }
        let StatementKind::Assign(cast) = &data.statements[before].kind else {
            return None;
        };
        let (cast_destination, Rvalue::Cast(mir::CastKind::IntToInt, source, target_type)) =
            &**cast
        else {
            return None;
        };
        if *cast_destination != cast_place || place(source)? != place(count)? {
            return None;
        }
        let ty::Int(signed) = source.ty(&body.local_decls, self.tcx).kind() else {
            return None;
        };
        let ty::Uint(unsigned) = target_type.kind() else {
            return None;
        };
        let source_bits = signed.bit_width()?;
        if unsigned.bit_width()? != source_bits {
            return None;
        }
        if body.local_decls[cast_place.local].ty != *target_type
            || body.local_decls[condition.local].ty != self.tcx.types.bool
        {
            return None;
        }
        let mir::Const::Val(mir::ConstValue::Scalar(value), limit_type) = limit.const_ else {
            return None;
        };
        if limit_type != *target_type {
            return None;
        }
        let value = value.try_to_scalar_int().ok()?;
        let width = u16::try_from(value.to_bits(value.size())).ok()?;
        if !matches!(width, 8 | 16 | 32 | 64 | 128) {
            return None;
        }
        // The immediate ordinary shift confirms the typed width and original count.
        let first = body.basic_blocks[*target].statements.first()?;
        let StatementKind::Assign(shift) = &first.kind else {
            return None;
        };
        let (_, Rvalue::BinaryOp(shift_op, shift_operands)) = &**shift else {
            return None;
        };
        if shift_op != op || place(&shift_operands.1)? != place(source)? {
            return None;
        }
        let actual_width = match shift_operands.0.ty(&body.local_decls, self.tcx).kind() {
            ty::Int(i) => i.bit_width()?,
            ty::Uint(i) => i.bit_width()?,
            _ => return None,
        };
        if actual_width != u64::from(width) {
            return None;
        }
        // The full-use walk may consume only the remaining global budget.
        // Charge its known header before visiting any child; every aggregate
        // and projection is also charged before its recursive visitor step.
        let start_work = self.expression_work.get();
        let remaining = 500_000usize.saturating_sub(start_work);
        let header = statements.saturating_add(body.local_decls.len());
        if header > remaining {
            return None;
        }
        let mut uses = Uses {
            locals: [cast_place.local, condition.local],
            counts: [0, 0],
            work: header,
            limit: remaining,
        };
        uses.visit_body(body);
        let work = start_work.saturating_add(uses.work);
        self.expression_work.set(work);
        if uses.work > uses.limit || work > 500_000 || uses.counts != [2, 2] {
            return None;
        }
        Some(Plan {
            skip: [before, last],
            source: source.clone(),
            width,
        })
    }
    pub(super) fn shift_guard_condition(
        &self,
        body: &Body<'tcx>,
        plan: &Plan<'tcx>,
        locals: &Environment,
        span: Span,
    ) -> Result<Expr, String> {
        let value = self.operand(body, &plan.source, locals, span)?;
        let ty = self.ty(plan.source.ty(&body.local_decls, self.tcx), span)?;
        let zero = Expr::Int {
            value: "0".into(),
            ty: ty.clone(),
        };
        // Width is a power of two. For signed fixed-width count values,
        // (count & -width) == 0 iff 0 <= count < width. The negative mask
        // fits even when count is i8 and width is 128. Capture count once;
        // no unsigned cast or repeated source initializer enters the IR.
        let masked = Expr::Binary {
            kind: BinaryOp::Bitand,
            ty: ty.clone(),
            left: Box::new(value),
            right: Box::new(Expr::Int {
                value: (-i32::from(plan.width)).to_string(),
                ty: ty.clone(),
            }),
        };
        Ok(Expr::Binary {
            kind: BinaryOp::Eq,
            ty,
            left: Box::new(masked),
            right: Box::new(zero),
        })
    }
}
