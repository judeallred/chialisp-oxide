//! Actual primitive Rust IntToInt casts, checked before logical type erasure.
//! This deliberately does not authorize bool/char, floats, pointers, usize/isize,
//! wrappers, custom methods or the rest of rustc's CastKind variants.
use super::{Environment, Lowerer};
use chialisp_oxide_ir::{Expr, Type};
use rustc_middle::{
    mir::{Body, Operand},
    ty::{self, Ty},
};
use rustc_span::Span;
pub(super) fn primitive_integer(ty: Ty<'_>) -> Option<Type> {
    let (bits, signed) = match ty.kind() {
        ty::Int(int) => (int.bit_width()?, true),
        ty::Uint(int) => (int.bit_width()?, false),
        _ => return None,
    };
    if !matches!(bits, 8 | 16 | 32 | 64 | 128) {
        return None;
    }
    Some(Type::Int {
        bits: bits as u16,
        signed,
    })
}
impl<'tcx> Lowerer<'tcx> {
    pub(super) fn integer_cast(
        &self,
        body: &Body<'tcx>,
        operand: &Operand<'tcx>,
        target: Ty<'tcx>,
        locals: &Environment,
        span: Span,
    ) -> Result<Option<Expr>, String> {
        let Some(source) = primitive_integer(operand.ty(&body.local_decls, self.tcx)) else {
            return Ok(None);
        };
        let Some(target) = primitive_integer(target) else {
            return Ok(None);
        };
        let value = self.operand(body, operand, locals, span)?;
        Ok(Some(Expr::IntCast(chialisp_oxide_ir::IntegerCast {
            value: Box::new(value),
            source,
            target,
        })))
    }
}
