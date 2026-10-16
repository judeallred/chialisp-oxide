//! Conservative totality relative to already evaluated, well-typed operands.
//!
//! Called only after the normal lowering of the statement/function succeeds.
//! Strict primitive arithmetic and failing calls retain their defining effects;
//! copying their values through total Rust helpers must not add extra effects.
use super::Lowerer;
use rustc_hir::def_id::DefId;
use rustc_middle::{
    mir::{BinOp, Body, Rvalue, StatementKind, TerminatorKind, UnOp},
    ty,
};

const MAX_CACHED_FUNCTIONS: usize = 256;
const MAX_TOTALITY_BLOCKS: usize = 1_024;
const MAX_TOTALITY_STATEMENTS: usize = 8_192;

impl<'tcx> Lowerer<'tcx> {
    pub(super) fn rvalue_is_total(value: &Rvalue<'tcx>) -> bool {
        match value {
            // Operands are MIR places/constants, not nested source evaluations.
            // Lowering already rejected unsupported types and projections.
            Rvalue::Use(..)
            | Rvalue::Aggregate(..)
            | Rvalue::Repeat(..)
            | Rvalue::Discriminant(..) => true,
            // Accepted borrows only copy immutable tree/byte values. Their
            // source place has already been evaluated and type checked.
            Rvalue::Ref(_, rustc_middle::mir::BorrowKind::Shared, _) => true,
            // Lowering permits this coercion only for a literal byte array.
            Rvalue::Cast(
                rustc_middle::mir::CastKind::PointerCoercion(
                    ty::adjustment::PointerCoercion::Unsize,
                    _,
                ),
                _,
                _,
            ) => true,
            Rvalue::UnaryOp(UnOp::Not, _) => true,
            Rvalue::BinaryOp(op, _) => matches!(
                op,
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
            ),
            // Checked arithmetic and all uncertain/future operations retain
            // evaluation, even if a particular set of arguments would succeed.
            _ => false,
        }
    }

    pub(super) fn function_is_total(&self, id: DefId) -> bool {
        self.total_functions
            .borrow()
            .get(&id)
            .copied()
            .unwrap_or(false)
    }

    pub(super) fn remember_function_totality(&self, id: DefId, body: &Body<'tcx>) {
        if self.total_functions.borrow().contains_key(&id)
            || self.total_functions.borrow().len() >= MAX_CACHED_FUNCTIONS
        {
            return;
        }
        // No recursive analysis: each reachable callee has already lowered
        // successfully and entered this bounded cache. Missing entries, cycles,
        // excessive bodies, and unreachable uncertain code stay conservative.
        let total = body.basic_blocks.len() <= MAX_TOTALITY_BLOCKS && self.body_is_total(body);
        self.total_functions.borrow_mut().insert(id, total);
    }

    fn body_is_total(&self, body: &Body<'tcx>) -> bool {
        let mut statements = 0usize;
        for block in body.basic_blocks.iter() {
            statements = statements.saturating_add(block.statements.len());
            if statements > MAX_TOTALITY_STATEMENTS {
                return false;
            }
            for statement in &block.statements {
                let total = match &statement.kind {
                    StatementKind::Assign(assignment) => Self::rvalue_is_total(&assignment.1),
                    StatementKind::StorageLive(_)
                    | StatementKind::StorageDead(_)
                    | StatementKind::Nop
                    | StatementKind::ConstEvalCounter
                    | StatementKind::PlaceMention(_) => true,
                    _ => false,
                };
                if !total {
                    return false;
                }
            }
            let total = match &block.terminator().kind {
                TerminatorKind::Return
                | TerminatorKind::Goto { .. }
                | TerminatorKind::SwitchInt { .. } => true,
                TerminatorKind::Call { func, .. } => {
                    match func.ty(&body.local_decls, self.tcx).kind() {
                        ty::FnDef(id, _) => self.function_is_total(*id),
                        _ => false,
                    }
                }
                // Assert, panic/divergence, Unreachable and any unsupported
                // terminator may fail. Do not infer totality from return values.
                _ => false,
            };
            if !total {
                return false;
            }
        }
        true
    }
}
