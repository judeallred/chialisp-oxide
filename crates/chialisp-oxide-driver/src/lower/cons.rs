//! Explicit SDK wire-pair representation; no type or method spelling is an intrinsic.
use super::Lowerer;
use rustc_middle::ty::{self, Ty};
use rustc_span::Span;

const MARKER: &str = "__chialisp_oxide_wire_pair_v1";

impl<'tcx> Lowerer<'tcx> {
    /// Initial pairs carry only values with closed, allocation-free ownership.
    /// Check actual Rust identities before the final drop query, not the erased
    /// logical Tree type (which also represents separately supported boxes).
    pub(super) fn check_cons_components(
        &self,
        components: [Ty<'tcx>; 2],
        span: Span,
    ) -> Result<(), String> {
        let invalid = || {
            self.error(span, "Cons components require drop-free fixed values and borrowed Tree data; owned trees/boxes are unsupported")
        };
        let mut pending = components
            .into_iter()
            .map(|ty| (ty, 0usize))
            .collect::<Vec<_>>();
        let mut visited = 0usize;
        while let Some((ty, depth)) = pending.pop() {
            visited += 1;
            if visited > 4096 || depth >= 32 {
                return Err(self.error(span, "Cons component type discovery limit exceeded"));
            }
            match ty.kind() {
                ty::Tuple(items) => {
                    if items.len() > 256
                        || items.len() > 4096usize.saturating_sub(visited + pending.len())
                    {
                        return Err(
                            self.error(span, "Cons component type discovery limit exceeded")
                        );
                    }
                    pending.extend(items.iter().map(|ty| (ty, depth + 1)));
                }
                ty::Array(element, _) | ty::Ref(_, element, ty::Mutability::Not) => {
                    pending.push((*element, depth + 1))
                }
                ty::Adt(..) => {
                    if self.owned_box_inner(ty).is_some() {
                        return Err(invalid());
                    }
                    if let Some(shape) = self.optional_shape(ty, span)? {
                        pending.push((shape.item, depth + 1));
                    } else if let Some(shape) = self.tree_shape(ty, span)? {
                        if shape.ownership == super::trees::Ownership::OwnedBoxes {
                            return Err(invalid());
                        }
                    } else if let Some(fields) = self.cons_fields(ty, span)? {
                        if 2 > 4096usize.saturating_sub(visited + pending.len()) {
                            return Err(
                                self.error(span, "Cons component type discovery limit exceeded")
                            );
                        }
                        pending.extend(fields.into_iter().map(|ty| (ty, depth + 1)));
                    } else if let Some(inner) = self.transparent_inner(ty, span)? {
                        pending.push((inner, depth + 1));
                    }
                }
                ty::Bool | ty::Int(..) | ty::Uint(..) | ty::Slice(..) => (),
                _ => return Err(self.error(span, "unsupported Cons component type")),
            }
        }
        // lower_type has already admitted the concrete component types, and
        // the bounded scan excluded owned boxes and owned-tree variants.
        if components
            .into_iter()
            .any(|ty| ty.needs_drop(self.tcx, ty::TypingEnv::fully_monomorphized()))
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub(super) fn cons_fields(
        &self,
        rust_ty: Ty<'tcx>,
        span: Span,
    ) -> Result<Option<[Ty<'tcx>; 2]>, String> {
        let ty::Adt(definition, args) = rust_ty.kind() else {
            return Ok(None);
        };
        if !definition.did().is_local() && !self.allowed_external(definition.did()) {
            return Ok(None);
        }
        // The pinned rustc's find_attr! uses this provider for local and
        // external parsed DocComment attributes. Filtering by has_name(doc)
        // drops those comments; no deprecated attribute getter is needed.
        let attributes = rustc_hir::attrs::HasAttrs::get_attrs(definition.did(), &self.tcx);
        if attributes.len() > 256 {
            return Err(self.error(
                span,
                "wire-pair declaration metadata attribute limit exceeded",
            ));
        }
        let markers = attributes
            .iter()
            .filter_map(|attr| attr.doc_str())
            .filter(|text| text.as_str().starts_with("__chialisp_oxide_wire_pair_"))
            .take(2)
            .collect::<Vec<_>>();
        if markers.is_empty() {
            return Ok(None);
        }
        let invalid = || {
            self.error(span, "wire pairs require the marked SDK two-field Cons declaration with two concrete type arguments, ordinary representation and no Drop")
        };
        if markers.len() != 1
            || markers[0].as_str() != MARKER
            || !self.allowed_external(definition.did())
            || !definition.is_struct()
            || definition.has_dtor(self.tcx)
            || definition.repr().c()
            || definition.repr().transparent()
            || definition.repr().int.is_some()
            || definition.repr().pack.is_some()
            || definition.repr().align.is_some()
            || self.tcx.def_span(definition.did()).from_expansion()
            || args.len() != 2
        {
            return Err(invalid());
        }
        let generics = self.tcx.generics_of(definition.did());
        if generics.parent.is_some()
            || generics.own_params.len() != 2
            || generics.own_params.iter().any(|p| {
                !matches!(
                    p.kind,
                    ty::GenericParamDefKind::Type {
                        has_default: false,
                        ..
                    }
                )
            })
        {
            return Err(invalid());
        }
        let [Some(first), Some(rest)] = [args[0].as_type(), args[1].as_type()] else {
            return Err(invalid());
        };
        let fields = &definition.non_enum_variant().fields;
        if fields.len() != 2 {
            return Err(invalid());
        }
        for (index, field) in fields.iter().enumerate() {
            if !field.vis.is_public() {
                return Err(invalid());
            }
            // Require the raw declaration to use Head/Tail directly before
            // any normalization or component inspection. A forged marker
            // cannot authorize arbitrary alias/constant evaluation here.
            let raw = self.tcx.type_of(field.did).skip_binder();
            if !matches!(raw.kind(), ty::Param(parameter) if parameter.index == index as u32) {
                return Err(invalid());
            }
        }
        Ok(Some([first, rest]))
    }

    /// Discover pair-bearing local snapshots without interpreting pointer locals
    /// or otherwise admitting unsupported Rust types. Bound work before scans.
    pub(super) fn contains_cons(&self, rust_ty: Ty<'tcx>, span: Span) -> Result<bool, String> {
        let mut pending = vec![(rust_ty, 0usize)];
        let mut visited = 0usize;
        while let Some((ty, depth)) = pending.pop() {
            visited += 1;
            if visited > 4096 || depth >= 32 {
                return Err(self.error(span, "pair type discovery limit exceeded"));
            }
            if self.cons_fields(ty, span)?.is_some() {
                return Ok(true);
            }
            match ty.kind() {
                ty::Tuple(items) => {
                    if items.len() > 256
                        || items.len() > 4096usize.saturating_sub(visited + pending.len())
                    {
                        return Err(self.error(span, "pair type discovery tuple limit exceeded"));
                    }
                    pending.extend(items.iter().map(|ty| (ty, depth + 1)));
                }
                ty::Array(element, _) | ty::Ref(_, element, _) => {
                    pending.push((*element, depth + 1))
                }
                _ => (),
            }
        }
        Ok(false)
    }
}
