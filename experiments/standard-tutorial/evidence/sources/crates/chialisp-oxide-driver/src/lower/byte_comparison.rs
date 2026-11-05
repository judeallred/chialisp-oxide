//! A reviewed primitive operation, not general trait or standard-library MIR
//! support. Check actual Rust identities/types before erasing references.
use super::Lowerer;
use chialisp_oxide_ir::{BinaryOp, Expr, Type};
use rustc_middle::{
    mir::{Body, Operand, Place},
    ty::{self, Ty},
};
use rustc_span::{Span, Spanned, Symbol};

pub(super) struct Comparison {
    pub kind: BinaryOp,
    pub ty: Type,
}
impl Comparison {
    pub fn expression(self, left: Expr, right: Expr) -> Expr {
        Expr::Binary {
            kind: self.kind,
            ty: self.ty,
            left: Box::new(left),
            right: Box::new(right),
        }
    }
}

impl<'tcx> Lowerer<'tcx> {
    /// Total relative to the already evaluated, well-typed byte operands. VM
    /// resource exhaustion remains inconclusive, as for other total IR nodes.
    /// Never cache this by the generic comparison method's DefId: that same
    /// method can dispatch to a user implementation with arbitrary behavior.
    pub(super) fn byte_comparison(
        &self,
        body: &Body<'tcx>,
        func: &Operand<'tcx>,
        args: &[Spanned<Operand<'tcx>>],
        destination: Place<'tcx>,
        span: Span,
    ) -> Result<Option<Comparison>, String> {
        let ty::FnDef(id, generic) = func.ty(&body.local_decls, self.tcx).kind() else {
            return Ok(None);
        };
        let member = [
            ("cmp_partialeq_eq", BinaryOp::Eq),
            ("cmp_partialeq_ne", BinaryOp::Ne),
            ("cmp_partialord_lt", BinaryOp::Lt),
            ("cmp_partialord_le", BinaryOp::Le),
            ("cmp_partialord_gt", BinaryOp::Gt),
            ("cmp_partialord_ge", BinaryOp::Ge),
        ]
        .into_iter()
        .find(|(item, _)| self.tcx.is_diagnostic_item(Symbol::intern(item), *id));
        let Some((_, kind)) = member else {
            return Ok(None);
        };
        let equality = matches!(kind, BinaryOp::Eq | BinaryOp::Ne);
        let (category, trait_name, trait_id) = if equality {
            (
                "byte equality",
                "PartialEq",
                self.tcx.lang_items().eq_trait(),
            )
        } else {
            (
                "byte ordering",
                "PartialOrd",
                self.tcx.lang_items().partial_ord_trait(),
            )
        };
        let Some(trait_id) = trait_id else {
            return Ok(None);
        };
        if id.is_local()
            || id.krate != trait_id.krate
            || self.tcx.associated_item(*id).trait_container(self.tcx) != Some(trait_id)
        {
            return Err(self.error(
                span,
                format!("{category} requires the actual core {trait_name} trait"),
            ));
        }
        let generic = generic.skip_binder();
        if generic.len() != 2 || args.len() != 2 {
            return Err(self.error(
                span,
                format!("{category} has an unsupported call signature"),
            ));
        }
        let (Some(left), Some(right)) = (generic[0].as_type(), generic[1].as_type()) else {
            return Err(self.error(
                span,
                format!("{category} requires concrete primitive Rust types"),
            ));
        };
        // Bound both primitive type spines before any recursive type erasure
        // or trait-resolution query, including an unsupported RHS.
        let logical = self.byte_comparison_type(left, category, trait_name, span)?;
        self.byte_comparison_type(right, category, trait_name, span)?;
        let erase = |ty| self.tcx.erase_and_anonymize_regions(ty);
        if erase(left) != erase(right) {
            return Err(self.error(span, format!("{category} requires identical Rust operand types; mixed slice/array comparisons are unsupported")));
        }
        if !destination.ty(&body.local_decls, self.tcx).ty.is_bool() {
            return Err(self.error(span, format!("{category} must return bool")));
        }
        for (argument, expected) in args.iter().zip([left, right]) {
            if !matches!(argument.node.ty(&body.local_decls,self.tcx).kind(), ty::Ref(_, inner, ty::Mutability::Not) if erase(*inner)==erase(expected))
            {
                return Err(self.error(
                    span,
                    format!("{category} requires immutable shared operands"),
                ));
            }
        }
        let typing = ty::TypingEnv::post_analysis(self.tcx, body.source.def_id());
        let mut current = left;
        // Validate every immutable forwarding layer and the terminal primitive
        // dispatch. Only PartialEq::ne needs the actual core default trait
        // method exception. Pinned slice/array/ref ordering implementations
        // override all four operators. Do not infer custom PartialOrd behavior
        // from its name, erased bytes, or the laws its author should uphold.
        loop {
            let current_args = self.tcx.mk_args(&[current.into(), current.into()]);
            let instance = ty::Instance::try_resolve(self.tcx, typing, *id, current_args)
                .map_err(|_| self.error(span, format!("cannot resolve standard {category}")))?
                .ok_or_else(|| {
                    self.error(
                        span,
                        format!("standard {category} has no resolved instance"),
                    )
                })?;
            let ty::InstanceKind::Item(resolved) = instance.def else {
                return Err(self.error(
                    span,
                    format!("{category} requires an ordinary core implementation"),
                ));
            };
            if resolved.is_local() || resolved.krate != trait_id.krate {
                return Err(self.error(
                    span,
                    format!("custom {trait_name} implementations are unsupported"),
                ));
            }
            let item = self.tcx.associated_item(resolved);
            if item.trait_item_or_self().ok() != Some(*id) {
                return Err(self.error(
                    span,
                    format!("{category} resolved to an unexpected trait member"),
                ));
            }
            if let Some(implementation) = self.tcx.impl_of_assoc(resolved) {
                let reference = self
                    .tcx
                    .try_instantiate_and_normalize_erasing_regions(
                        instance.args,
                        typing,
                        self.tcx.impl_trait_ref(implementation),
                    )
                    .map_err(|_| {
                        self.error(span, format!("cannot normalize {category} implementation"))
                    })?;
                if reference.def_id != trait_id
                    || reference.args.len() != 2
                    || erase(reference.self_ty()) != erase(current)
                    || erase(reference.args.type_at(1)) != erase(current)
                {
                    return Err(self.error(
                        span,
                        format!("{category} implementation has unexpected primitive types"),
                    ));
                }
            } else if resolved != *id || !matches!(kind, BinaryOp::Ne) {
                return Err(self.error(
                    span,
                    format!("{category} requires a concrete standard implementation"),
                ));
            }
            match current.kind() {
                ty::Ref(_, inner, ty::Mutability::Not) => current = *inner,
                _ => break,
            }
        }
        Ok(Some(Comparison { kind, ty: logical }))
    }

    fn byte_comparison_type(
        &self,
        mut ty: Ty<'tcx>,
        category: &str,
        trait_name: &str,
        span: Span,
    ) -> Result<Type, String> {
        let array_category = if trait_name == "PartialEq" {
            "byte-array equality"
        } else {
            "byte-array ordering"
        };
        for _ in 0..32 {
            match ty.kind() {
                ty::Ref(_, inner, ty::Mutability::Not) => ty = *inner,
                ty::Slice(element) if *element == self.tcx.types.u8 => return Ok(Type::Atom),
                ty::Array(element, length) if *element == self.tcx.types.u8 => {
                    let length = length.try_to_target_usize(self.tcx).ok_or_else(|| {
                        self.error(span, format!("{array_category} requires a constant length"))
                    })?;
                    if length > 256 {
                        return Err(self.error(span, format!("{array_category} is limited to 256 elements")));
                    }
                    return Ok(Type::Bytes { len: length as usize });
                }
                _ => return Err(self.error(span, format!("{category} supports only identical [u8] or [u8; N] operands through immutable references; custom {trait_name} and other elements are unsupported"))),
            }
        }
        Err(self.error(span, format!("{category} reference nesting exceeds 32")))
    }
}
