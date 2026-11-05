use super::Lowerer;
use rustc_middle::ty::{self, Ty};
use rustc_span::Span;

impl<'tcx> Lowerer<'tcx> {
    /// This erases a deliberately narrow source-level nominal wrapper. Rustc
    /// checks its nominal identity before we expose the inner CLVM ABI type.
    pub(super) fn transparent_inner(
        &self,
        rust_ty: Ty<'tcx>,
        span: Span,
    ) -> Result<Option<Ty<'tcx>>, String> {
        let ty::Adt(definition, args) = rust_ty.kind() else {
            return Ok(None);
        };
        if (!definition.did().is_local() && !self.allowed_external(definition.did()))
            || !definition.is_struct()
            || !definition.repr().transparent()
            || definition.non_enum_variant().fields.len() != 1
        {
            return Err(self.error(
                span,
                "only local or chialisp_oxide #[repr(transparent)] structs with exactly one field are supported",
            ));
        }
        if self.tcx.def_span(definition.did()).from_expansion() {
            return Err(self.error(span, "macro-generated newtype declarations are unsupported"));
        }
        if self.tcx.generics_of(definition.did()).count() != 0 || !args.is_empty() {
            return Err(self.error(span, "generic transparent newtypes are unsupported"));
        }
        if definition.has_dtor(self.tcx) {
            return Err(self.error(
                span,
                "transparent newtypes with custom Drop are unsupported",
            ));
        }
        let field = definition
            .non_enum_variant()
            .fields
            .iter()
            .next()
            .ok_or_else(|| self.error(span, "transparent newtype requires a field"))?;
        let inner = self
            .tcx
            .try_normalize_erasing_regions(
                ty::TypingEnv::fully_monomorphized(),
                field.ty(self.tcx, args),
            )
            .map_err(|_| self.error(span, "unsupported transparent newtype field type"))?;
        let layout = self
            .tcx
            .layout_of(ty::TypingEnv::fully_monomorphized().as_query_input(inner))
            .map_err(|_| self.error(span, "unsupported transparent newtype field layout"))?;
        if layout.size.bytes() == 0 {
            return Err(self.error(span, "zero-sized transparent newtypes are unsupported"));
        }
        Ok(Some(inner))
    }
}
