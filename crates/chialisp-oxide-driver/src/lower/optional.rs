//! A closed output encoding for the compiler's actual standard Option enum.
//! None is nil and Some(value) is one proper-list element, never a Rust tag.
use super::Lowerer;
use chialisp_oxide_ir::Expr;
use rustc_abi::VariantIdx;
use rustc_hir::def_id::DefId;
use rustc_middle::{
    mir::Const,
    ty::{self, Ty, TypeVisitableExt},
};
use rustc_span::Span;

pub(super) struct OptionalShape<'tcx> {
    pub item: Ty<'tcx>,
    pub none: VariantIdx,
    pub some: VariantIdx,
}

impl<'tcx> Lowerer<'tcx> {
    pub(super) fn optional_method(&self, id: DefId) -> bool {
        let Some(option) = self.tcx.lang_items().option_type() else {
            return false;
        };
        if id.is_local() || id.krate != option.krate {
            return false;
        }
        self.tcx.impl_of_assoc(id).is_some_and(|implementation| {
            matches!(self.tcx.type_of(implementation).skip_binder().kind(), ty::Adt(definition, _) if definition.did() == option)
        })
    }

    pub(super) fn optional_shape(
        &self,
        rust_ty: Ty<'tcx>,
        span: Span,
    ) -> Result<Option<OptionalShape<'tcx>>, String> {
        let ty::Adt(definition, args) = rust_ty.kind() else {
            return Ok(None);
        };
        let language = self.tcx.lang_items();
        if language.option_type() != Some(definition.did()) {
            return Ok(None);
        }
        let invalid = || {
            self.error(span, "optional lists require the actual standard Option enum with None and Some(T), ordinary representation and one concrete payload type")
        };
        // Toolchain admission pins the loaded core owner; a display path or a
        // same-named local enum cannot select this representation.
        if definition.did().is_local()
            || language
                .panic_fn()
                .is_none_or(|id| id.krate != definition.did().krate)
            || !definition.is_enum()
            || definition.has_dtor(self.tcx)
            || definition.variants().len() != 2
            || definition.repr().c()
            || definition.repr().transparent()
            || definition.repr().int.is_some()
            || definition.repr().pack.is_some()
            || definition.repr().align.is_some()
            || args.len() != 1
        {
            return Err(invalid());
        }
        let generics = self.tcx.generics_of(definition.did());
        if generics.parent.is_some()
            || generics.own_params.len() != 1
            || !matches!(
                generics.own_params[0].kind,
                ty::GenericParamDefKind::Type {
                    has_default: false,
                    ..
                }
            )
        {
            return Err(invalid());
        }
        let item = args[0].as_type().ok_or_else(invalid)?;
        let none_id = language.option_none_variant().ok_or_else(invalid)?;
        let some_id = language.option_some_variant().ok_or_else(invalid)?;
        let mut none = None;
        let mut some = None;
        for (index, variant) in definition.variants().iter_enumerated() {
            if variant.def_id == none_id && variant.fields.is_empty() {
                none = Some(index);
            } else if variant.def_id == some_id && variant.fields.len() == 1 {
                let field = variant.fields.iter().next().ok_or_else(invalid)?;
                let raw = self.tcx.type_of(field.did).skip_binder();
                if !matches!(raw.kind(), ty::Param(parameter) if parameter.index == 0) {
                    return Err(invalid());
                }
                some = Some(index);
            } else {
                return Err(invalid());
            }
        }
        Ok(Some(OptionalShape {
            item,
            none: none.ok_or_else(invalid)?,
            some: some.ok_or_else(invalid)?,
        }))
    }

    /// Select strict runtime binding even for a discarded optional constructor.
    /// This scan does not normalize arbitrary aliases or admit their operations.
    pub(super) fn contains_optional(&self, rust_ty: Ty<'tcx>, span: Span) -> Result<bool, String> {
        let mut pending = vec![(rust_ty, 0usize)];
        let mut visited = 0usize;
        while let Some((ty, depth)) = pending.pop() {
            visited += 1;
            if visited > 4096 || depth >= 32 {
                return Err(self.error(span, "optional type discovery limit exceeded"));
            }
            if self.optional_shape(ty, span)?.is_some() {
                return Ok(true);
            }
            match ty.kind() {
                ty::Tuple(items) => {
                    if items.len() > 256
                        || items.len() > 4096usize.saturating_sub(visited + pending.len())
                    {
                        return Err(self.error(span, "optional type discovery limit exceeded"));
                    }
                    pending.extend(items.iter().map(|ty| (ty, depth + 1)));
                }
                ty::Array(element, _) | ty::Ref(_, element, _) => {
                    pending.push((*element, depth + 1))
                }
                ty::Adt(definition, args) => {
                    if let Some(fields) = self.cons_fields(ty, span)? {
                        if 2 > 4096usize.saturating_sub(visited + pending.len()) {
                            return Err(self.error(span, "optional type discovery limit exceeded"));
                        }
                        pending.extend(fields.into_iter().map(|ty| (ty, depth + 1)));
                    } else if definition.is_struct()
                        && definition.repr().transparent()
                        && (definition.did().is_local() || self.allowed_external(definition.did()))
                        && definition.non_enum_variant().fields.len() == 1
                        && args.is_empty()
                        && self.tcx.generics_of(definition.did()).count() == 0
                    {
                        // Occurrence discovery must not normalize aliases or
                        // query layouts. Full admission happens in lower_type.
                        let field = definition
                            .non_enum_variant()
                            .fields
                            .iter()
                            .next()
                            .ok_or_else(|| {
                                self.error(span, "transparent optional discovery requires a field")
                            })?;
                        pending.push((self.tcx.type_of(field.did).skip_binder(), depth + 1));
                    }
                }
                _ => (),
            }
        }
        Ok(false)
    }

    /// Read only the fieldless standard None variant. Never reinterpret a
    /// niche as zero bits or consume diagnostic-only payload ConstValues.
    pub(super) fn optional_constant(
        &self,
        constant: Const<'tcx>,
        span: Span,
    ) -> Result<Expr, String> {
        let rust_ty = constant.ty();
        let shape = self
            .optional_shape(rust_ty, span)?
            .ok_or_else(|| self.error(span, "expected the actual standard Option constant"))?;
        // Validate payload identity, size and ownership even though None has
        // no live payload. Unevaluated CTFE is deliberately not extended here.
        self.ty(rust_ty, span)?;
        let invalid = || {
            self.error(span, "optional constants support only already evaluated None; construct Some and nontrivial constants in a supported function")
        };
        let is_none = match constant {
            Const::Ty(_, constant) => {
                let ty::ConstKind::Value(value) = constant.kind() else {
                    return Err(invalid());
                };
                if value.ty != rust_ty {
                    return Err(invalid());
                }
                let Some([tag]) = value.try_to_branch() else {
                    return Err(invalid());
                };
                let ty::ConstKind::Value(tag) = tag.kind() else {
                    return Err(invalid());
                };
                let Some(index) = tag.try_to_leaf() else {
                    return Err(invalid());
                };
                tag.ty == self.tcx.types.u32
                    && index.size().bytes() == 4
                    && index.to_u32() == shape.none.as_u32()
            }
            Const::Val(value, _) if !rust_ty.has_non_region_param() => self
                .tcx
                .try_destructure_mir_constant_for_user_output(value, rust_ty)
                .is_some_and(|value| value.variant == Some(shape.none) && value.fields.is_empty()),
            _ => return Err(invalid()),
        };
        if !is_none {
            return Err(invalid());
        }
        Ok(Expr::OptionalNone {
            item: self.ty(shape.item, span)?,
        })
    }
}
