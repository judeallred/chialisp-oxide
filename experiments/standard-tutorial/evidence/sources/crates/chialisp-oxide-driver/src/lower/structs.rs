//! Local named value records use declaration-order proper-list fields.
//!
//! This is source admission, not a native-layout decoder or a new nominal wire
//! format. Rustc retains the nominal types until each checked MIR operation.
use super::Lowerer;
use chialisp_oxide_ir::{Expr, Type};
use rustc_middle::{
    mir::{PlaceElem, ProjectionElem},
    ty::{self, Ty},
};
use rustc_span::Span;

impl<'tcx> Lowerer<'tcx> {
    /// Return fields only for the new, closed local named-record profile.
    /// Existing transparent wrappers and SDK representations keep their routes.
    fn record_fields(
        &self,
        rust_ty: Ty<'tcx>,
        span: Span,
    ) -> Result<Option<Vec<Ty<'tcx>>>, String> {
        let ty::Adt(definition, args) = rust_ty.kind() else {
            return Ok(None);
        };
        if !definition.did().is_local()
            || !definition.is_struct()
            || definition.repr().transparent()
            || definition.non_enum_variant().ctor_kind().is_some()
        {
            return Ok(None);
        }
        let invalid = || {
            self.error(span, "named structs require a local nongeneric declaration, ordinary representation, named fields and no custom Drop")
        };
        if self.tcx.generics_of(definition.did()).count() != 0
            || !args.is_empty()
            || definition.has_dtor(self.tcx)
            || definition.repr().c()
            || definition.repr().int.is_some()
            || definition.repr().pack.is_some()
            || definition.repr().align.is_some()
        {
            return Err(invalid());
        }
        if self.tcx.def_span(definition.did()).from_expansion() {
            return Err(self.error(span, "macro-generated named structs are unsupported"));
        }
        let fields = &definition.non_enum_variant().fields;
        if fields.is_empty() || fields.len() > 256 {
            return Err(self.error(span, "named structs require 1..=256 fields"));
        }
        let work = self.expression_work.get().saturating_add(fields.len());
        self.expression_work.set(work);
        if work > super::MAX_EXPRESSION_WORK {
            return Err(self.error(span, "named struct field work limit exceeded"));
        }
        Ok(Some(
            fields
                .iter()
                .map(|field| self.tcx.type_of(field.did).skip_binder())
                .collect(),
        ))
    }

    pub(super) fn named_struct_fields(
        &self,
        rust_ty: Ty<'tcx>,
        span: Span,
    ) -> Result<Option<Vec<Ty<'tcx>>>, String> {
        let Some(fields) = self.record_fields(rust_ty, span)? else {
            return Ok(None);
        };
        // Inspect original types, including zero-length array element types,
        // before layout/drop queries or logical type erasure. No reference,
        // pointer, Option niche, owned tree, or arbitrary external ADT is data.
        let mut pending = vec![(rust_ty, 0usize)];
        let mut nodes = 0usize;
        while let Some((current, depth)) = pending.pop() {
            nodes += 1;
            let work = self.expression_work.get().saturating_add(1);
            self.expression_work.set(work);
            if nodes > 4096 || depth >= 32 || work > super::MAX_EXPRESSION_WORK {
                return Err(self.error(span, "named struct type/work limit exceeded"));
            }
            let mut children = Vec::new();
            match current.kind() {
                ty::Bool => (),
                ty::Int(kind) if kind.bit_width().is_some() => (),
                ty::Uint(kind) if kind.bit_width().is_some() => (),
                ty::Tuple(items) => {
                    if items.len() > 256 || items.len() > 4096usize.saturating_sub(nodes + pending.len()) {
                        return Err(self.error(span, "named struct type expansion limit exceeded"));
                    }
                    children.extend(items.iter());
                }
                ty::Array(element, len) => {
                    let count = len.try_to_target_usize(self.tcx).filter(|n| *n <= 256)
                        .ok_or_else(|| self.error(span, "named struct arrays are limited to 256 elements"))? as usize;
                    let count = if *element == self.tcx.types.u8 { 1 } else { count.max(1) };
                    if count > 4096usize.saturating_sub(nodes + pending.len()) {
                        return Err(self.error(span, "named struct type expansion limit exceeded"));
                    }
                    children.extend(std::iter::repeat_n(*element, count));
                }
                ty::Adt(definition, args) => {
                    if let Some(items) = self.record_fields(current, span)? {
                        children = items;
                    } else {
                        // Preflight a wrapper's actual declaration BEFORE its
                        // existing helper may query the inner native layout.
                        if (!definition.did().is_local() && !self.allowed_external(definition.did()))
                            || !definition.is_struct() || !definition.repr().transparent()
                            || definition.non_enum_variant().fields.len() != 1
                            || definition.has_dtor(self.tcx)
                            || self.tcx.generics_of(definition.did()).count() != 0 || !args.is_empty()
                            || self.tcx.def_span(definition.did()).from_expansion()
                        { return Err(self.error(span, "named struct fields require drop-free fixed values; references, enums, generic and owned containers are unsupported")); }
                        let field = definition.non_enum_variant().fields.iter().next().ok_or_else(|| self.error(span, "missing transparent field"))?;
                        children.push(self.tcx.type_of(field.did).skip_binder());
                    }
                }
                _ => return Err(self.error(span, "named struct fields require drop-free fixed values; references, enums, generic and platform-sized types are unsupported")),
            }
            if children.len() > 4096usize.saturating_sub(nodes + pending.len()) {
                return Err(self.error(span, "named struct type expansion limit exceeded"));
            }
            pending.extend(children.into_iter().map(|child| (child, depth + 1)));
        }
        if rust_ty.needs_drop(self.tcx, ty::TypingEnv::fully_monomorphized()) {
            return Err(self.error(span, "named structs must be drop-free"));
        }
        let layout = self
            .tcx
            .layout_of(ty::TypingEnv::fully_monomorphized().as_query_input(rust_ty))
            .map_err(|_| self.error(span, "unsupported named struct layout"))?;
        if layout.fields.count() != fields.len() {
            return Err(self.error(
                span,
                "named struct layout field inventory differs from its declaration",
            ));
        }
        Ok(Some(fields))
    }

    /// Discover records in ordinary aggregate locals without granting admission
    /// to an external, generic, or reference-bearing container.
    pub(super) fn contains_named_struct(
        &self,
        rust_ty: Ty<'tcx>,
        span: Span,
    ) -> Result<bool, String> {
        let mut pending = vec![(rust_ty, 0usize)];
        let mut nodes = 0usize;
        while let Some((current, depth)) = pending.pop() {
            nodes += 1;
            let work = self.expression_work.get().saturating_add(1);
            self.expression_work.set(work);
            if nodes > 4096 || depth >= 32 || work > super::MAX_EXPRESSION_WORK {
                return Err(self.error(span, "named struct discovery work limit exceeded"));
            }
            if self.record_fields(current, span)?.is_some() {
                self.named_struct_fields(current, span)?;
                return Ok(true);
            }
            match current.kind() {
                ty::Tuple(items) => {
                    if items.len() > 256
                        || items.len() > 4096usize.saturating_sub(nodes + pending.len())
                    {
                        return Err(
                            self.error(span, "named struct discovery expansion limit exceeded")
                        );
                    }
                    pending.extend(items.iter().map(|item| (item, depth + 1)));
                }
                ty::Array(element, _) | ty::Ref(_, element, _) => {
                    pending.push((*element, depth + 1))
                }
                ty::Adt(definition, args)
                    if definition.repr().transparent()
                        && definition.is_struct()
                        && (definition.did().is_local()
                            || self.allowed_external(definition.did()))
                        && args.is_empty()
                        && self.tcx.generics_of(definition.did()).count() == 0
                        && definition.non_enum_variant().fields.len() == 1 =>
                {
                    let field = definition
                        .non_enum_variant()
                        .fields
                        .iter()
                        .next()
                        .ok_or_else(|| self.error(span, "missing transparent field"))?;
                    pending.push((self.tcx.type_of(field.did).skip_binder(), depth + 1));
                }
                _ => (),
            }
        }
        Ok(false)
    }

    /// Replace fields of an already evaluated record snapshot. Only record-root
    /// updates gain this route; existing tuple and pair assignment stays closed.
    pub(super) fn update_named_struct(
        &self,
        rust_ty: Ty<'tcx>,
        base: Expr,
        projections: &[PlaceElem<'tcx>],
        value: Expr,
        span: Span,
    ) -> Result<Expr, String> {
        if self.named_struct_fields(rust_ty, span)?.is_none() {
            return Err(self.error(
                span,
                "projected assignment requires a supported named struct root",
            ));
        }
        let mut fields = Vec::new();
        let mut current = rust_ty;
        for projection in projections {
            let ProjectionElem::Field(index, field_ty) = *projection else {
                return Err(self.error(span, "named struct updates support only field projections"));
            };
            if self.named_struct_fields(current, span)?.is_some()
                || !matches!(current.kind(), ty::Adt(..))
            {
                fields.push(index.index());
            } else if self.transparent_inner(current, span)?.is_some() {
                if index.index() != 0 {
                    return Err(self.error(span, "invalid transparent field"));
                }
            } else {
                return Err(self.error(span, "unsupported named struct field projection"));
            }
            current = field_ty;
        }
        let logical = self.ty(rust_ty, span)?;
        // Runtime bindings make these snapshots cheap immutable values, never
        // an unevaluated initializer. Preflight every expanded copy BEFORE
        // cloning a growing projection chain or allocating tuple children.
        if !matches!(base, Expr::Arg { .. } | Expr::Local { .. }) {
            return Err(self.error(span, "named struct updates require an evaluated snapshot"));
        }
        let base_nodes = self.expression_size(&base, span)?;
        let value_nodes = self.expression_size(&value, span)?;
        let mut sizes = Vec::new();
        let mut current = &logical;
        let mut copy_work = value_nodes;
        for (level, selected) in fields.iter().copied().enumerate() {
            let Type::Tuple { items } = current else {
                return Err(self.error(span, "named struct update requires a tuple field"));
            };
            current = items
                .get(selected)
                .ok_or_else(|| self.error(span, "invalid named struct field"))?;
            let projection_nodes = base_nodes + level + 1;
            copy_work = copy_work.saturating_add(1 + items.len().saturating_mul(projection_nodes));
            sizes.push((items.len(), projection_nodes));
        }
        let mut result_nodes = value_nodes;
        for (count, projection_nodes) in sizes.into_iter().rev() {
            result_nodes =
                result_nodes.saturating_add(1 + (count - 1).saturating_mul(projection_nodes));
            if result_nodes > super::MAX_EXPRESSION_NODES {
                return Err(self.error(
                    span,
                    "named struct update expression expansion limit exceeded",
                ));
            }
        }
        let work = self.expression_work.get().saturating_add(copy_work);
        if work > super::MAX_EXPRESSION_WORK {
            return Err(self.error(span, "named struct update copy work limit exceeded"));
        }
        self.expression_work.set(work);
        let result = self.replace_record_field(&logical, base, &fields, value, span)?;
        self.check_expression(&result, span)?;
        Ok(result)
    }

    fn replace_record_field(
        &self,
        ty: &Type,
        base: Expr,
        fields: &[usize],
        value: Expr,
        span: Span,
    ) -> Result<Expr, String> {
        let Some((&selected, tail)) = fields.split_first() else {
            return Ok(value);
        };
        let Type::Tuple { items } = ty else {
            return Err(self.error(span, "named struct update requires a tuple field"));
        };
        if selected >= items.len() {
            return Err(self.error(span, "invalid named struct field"));
        }
        let mut replacement = Some(value);
        let values = items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let field = Expr::Field {
                    value: Box::new(base.clone()),
                    index,
                };
                if index == selected {
                    let value = replacement
                        .take()
                        .ok_or_else(|| self.error(span, "duplicate record update field"))?;
                    self.replace_record_field(item, field, tail, value, span)
                } else {
                    Ok(field)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Expr::Tuple { items: values })
    }
}
