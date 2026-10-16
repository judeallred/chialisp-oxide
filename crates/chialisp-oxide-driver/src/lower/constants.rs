use super::Lowerer;
use chialisp_oxide_ir::Expr;
use rustc_abi::Size;
use rustc_middle::{
    mir::{
        Const, ConstValue,
        interpret::{GlobalAlloc, alloc_range},
    },
    ty::{self, Ty},
};
use rustc_span::Span;

impl<'tcx> Lowerer<'tcx> {
    /// Accept only a value already validated as a fixed byte array or a chain
    /// of supported transparent wrappers. Rustc evaluates the constant; we do
    /// not reinterpret source syntax or follow pointers to other constants.
    pub(super) fn bytes_constant(
        &self,
        constant: Const<'tcx>,
        rust_ty: Ty<'tcx>,
        len: usize,
        span: Span,
    ) -> Result<Expr, String> {
        let env = ty::TypingEnv::fully_monomorphized();
        let size = Size::from_bytes(len);
        let layout = self
            .tcx
            .layout_of(env.as_query_input(rust_ty))
            .map_err(|_| self.error(span, "unsupported fixed-byte constant layout"))?;
        if len > 256 || layout.size != size {
            return Err(self.error(
                span,
                "fixed-byte constant must have its exact bounded byte-array layout",
            ));
        }
        let value = constant
            .eval(self.tcx, env, span)
            .map_err(|_| self.error(span, "unable to evaluate fixed-byte constant"))?;
        if matches!(value, ConstValue::ZeroSized) && len == 0 {
            return Ok(Expr::Bytes { hex: String::new() });
        }
        let ConstValue::Indirect { alloc_id, offset } = value else {
            return Err(self.error(span, "fixed-byte constant requires a by-value byte allocation; references and slices are unsupported"));
        };
        let Some(GlobalAlloc::Memory(allocation)) = self.tcx.try_get_global_alloc(alloc_id) else {
            return Err(self.error(span, "fixed-byte constant requires initialized memory"));
        };
        let allocation = allocation.inner();
        if offset
            .bytes()
            .checked_add(size.bytes())
            .is_none_or(|end| end > allocation.size().bytes())
        {
            return Err(self.error(
                span,
                "fixed-byte constant allocation is shorter than its declared layout",
            ));
        }
        let range = alloc_range(offset, size);
        if allocation.init_mask().is_range_initialized(range).is_err() {
            return Err(self.error(span, "uninitialized fixed-byte constants are unsupported"));
        }
        if !allocation.provenance().range_empty(range, &self.tcx) {
            return Err(self.error(
                span,
                "fixed-byte constants with pointer provenance are unsupported",
            ));
        }
        let bytes = allocation
            .get_bytes_strip_provenance(&self.tcx, range)
            .map_err(|_| {
                self.error(
                    span,
                    "fixed-byte constant contains invalid bytes or pointer provenance",
                )
            })?;
        Ok(Expr::Bytes {
            hex: bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        })
    }
}
