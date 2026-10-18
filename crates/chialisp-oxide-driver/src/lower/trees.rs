//! Structural borrowed trees; no SDK symbol grants an intrinsic meaning.
use super::Lowerer;
use chialisp_oxide_ir::{Expr, Type};
use rustc_hir::def_id::DefId;
use rustc_middle::{
    mir::{Body, Const, Operand, Place, Rvalue, StatementKind},
    ty::{self, Ty},
};
use rustc_span::Span;

#[derive(Clone, Copy)]
pub(super) struct TreeShape {
    pub atom: rustc_abi::VariantIdx,
    pub pair: rustc_abi::VariantIdx,
    pub ownership: Ownership,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Ownership {
    BorrowedRefs,
    OwnedBoxes,
}

impl<'tcx> Lowerer<'tcx> {
    pub(super) fn lifetime_parameters_only(&self, mut id: DefId) -> bool {
        loop {
            let generics = self.tcx.generics_of(id);
            if generics
                .own_params
                .iter()
                .any(|parameter| !matches!(parameter.kind, ty::GenericParamDefKind::Lifetime))
            {
                return false;
            }
            match generics.parent {
                Some(parent) => id = parent,
                None => return true,
            }
        }
    }

    pub(super) fn tree_shape(
        &self,
        rust_ty: Ty<'tcx>,
        span: Span,
    ) -> Result<Option<TreeShape>, String> {
        let ty::Adt(definition, args) = rust_ty.kind() else {
            return Ok(None);
        };
        if !definition.is_enum() {
            return Ok(None);
        }
        let invalid = || {
            self.error(span, "borrowed trees require exactly Atom(&[u8]) and Pair(&Self, &Self); owned trees instead require Pair(Box<Self>, Box<Self>); both require one lifetime, ordinary enum representation, and no Drop")
        };
        if (!definition.did().is_local() && !self.allowed_external(definition.did()))
            || definition.has_dtor(self.tcx)
            || definition.variants().len() != 2
            || definition.repr().c()
            || definition.repr().int.is_some()
            || self.tcx.def_span(definition.did()).from_expansion()
            || !self.lifetime_parameters_only(definition.did())
            || args.len() != 1
            || args[0].as_region().is_none()
        {
            return Err(invalid());
        }
        let mut atom = None;
        let mut pair = None;
        let mut ownership = Ownership::BorrowedRefs;
        for (index, variant) in definition.variants().iter_enumerated() {
            if !matches!(variant.fields.len(), 1 | 2) {
                return Err(invalid());
            }
            let fields = variant
                .fields
                .iter()
                .map(|field| {
                    self.tcx
                        .try_normalize_erasing_regions(
                            ty::TypingEnv::fully_monomorphized(),
                            field.ty(self.tcx, args),
                        )
                        .map_err(|_| invalid())
                })
                .collect::<Result<Vec<_>, _>>()?;
            match fields.as_slice() {
                [field] if matches!(field.kind(), ty::Ref(_, inner, ty::Mutability::Not) if matches!(inner.kind(), ty::Slice(element) if *element == self.tcx.types.u8)) => atom = Some(index),
                [first, rest] if [first, rest].into_iter().all(|field| {
                    matches!(field.kind(), ty::Ref(_, inner, ty::Mutability::Not) if self.tcx.erase_and_anonymize_regions(*inner) == self.tcx.erase_and_anonymize_regions(rust_ty))
                }) => pair = Some(index),
                [first, rest] if [first, rest].into_iter().all(|field| {
                    self.owned_box_inner(*field).is_some_and(|inner| self.tcx.erase_and_anonymize_regions(inner) == self.tcx.erase_and_anonymize_regions(rust_ty))
                }) => { pair = Some(index); ownership = Ownership::OwnedBoxes; },
                _ => return Err(invalid()),
            }
        }
        if ownership == Ownership::OwnedBoxes {
            self.check_owned_allocator(span)?;
        }
        Ok(Some(TreeShape {
            atom: atom.ok_or_else(invalid)?,
            pair: pair.ok_or_else(invalid)?,
            ownership,
        }))
    }

    pub(super) fn shared_tree_or_bytes(
        &self,
        rust_ty: Ty<'tcx>,
        span: Span,
    ) -> Result<bool, String> {
        let mut current = rust_ty;
        for _ in 0..32 {
            match current.kind() {
                ty::Ref(_, inner, ty::Mutability::Not) => current = *inner,
                ty::Ref(..) => return Ok(false),
                ty::Slice(element) | ty::Array(element, _) if *element == self.tcx.types.u8 => {
                    return Ok(true);
                }
                _ if self.owned_tree_box(current, span)?.is_some() => {
                    current = self.owned_tree_box(current, span)?.unwrap();
                }
                _ => return Ok(self.tree_shape(current, span)?.is_some()),
            }
        }
        Err(self.error(span, "shared reference nesting limit"))
    }

    pub(super) fn tree_discriminant(
        &self,
        body: &Body<'tcx>,
        operand: &Operand<'tcx>,
        locals: &super::Environment,
        span: Span,
    ) -> Result<Option<(Expr, TreeShape)>, String> {
        let (Operand::Copy(place) | Operand::Move(place)) = operand else {
            return Ok(None);
        };
        if !place.projection.is_empty()
            || !matches!(
                operand.ty(&body.local_decls, self.tcx).kind(),
                ty::Int(ty::IntTy::Isize)
            )
        {
            return Ok(None);
        }
        let mut source = None;
        for block in body.basic_blocks.iter() {
            for statement in &block.statements {
                if let StatementKind::Assign(assignment) = &statement.kind
                    && assignment.0.local == place.local
                {
                    if source.is_some() || !assignment.0.projection.is_empty() {
                        return Err(self.error(
                            span,
                            "tree discriminant local must have one direct definition",
                        ));
                    }
                    let Rvalue::Discriminant(tree) = &assignment.1 else {
                        return Err(self.error(
                            span,
                            "isize values are supported only as direct tree discriminants",
                        ));
                    };
                    source = Some(*tree);
                }
            }
        }
        let Some(tree) = source else { return Ok(None) };
        let shape = self
            .tree_shape(tree.ty(&body.local_decls, self.tcx).ty, span)?
            .ok_or_else(|| {
                self.error(
                    span,
                    "discriminants require a structurally validated borrowed tree",
                )
            })?;
        Ok(Some((
            Expr::TreeIsPair {
                value: Box::new(self.place(body, tree, locals, span)?),
            },
            shape,
        )))
    }

    pub(super) fn borrowed_constant(
        &self,
        constant: Const<'tcx>,
        span: Span,
    ) -> Result<Expr, String> {
        let Const::Unevaluated(value, _) = constant else {
            return Err(self.error(
                span,
                "borrowed tree/atom constants require inspectable constant or promoted MIR",
            ));
        };
        if !value.def.is_local() && !self.allowed_external(value.def) {
            return Err(self.error(span, "borrowed constants must be local or chialisp_oxide"));
        }
        if value
            .args
            .iter()
            .any(|argument| argument.as_region().is_none())
        {
            return Err(self.error(
                span,
                "borrowed constants cannot have type or const arguments",
            ));
        }
        // is_mir_available reports optimized function MIR, not constant CTFE
        // MIR. Nontrivial constants encode their CTFE bodies in metadata even
        // when no optimized body exists; trivial constants encode a value.
        let available = if value.promoted.is_some() {
            self.tcx.is_mir_available(value.def)
                || matches!(
                    self.tcx.def_kind(value.def),
                    rustc_hir::def::DefKind::Const { .. }
                )
        } else {
            matches!(
                self.tcx.def_kind(value.def),
                rustc_hir::def::DefKind::Const { .. } | rustc_hir::def::DefKind::AnonConst
            ) && self.tcx.trivial_const(value.def).is_none()
        };
        if !available {
            return Err(self.error(span, "MIR is unavailable for a borrowed constant; rebuild dependencies through cargo chialisp-oxide"));
        }
        let key = (value.def, value.promoted);
        {
            let mut stack = self.constants.borrow_mut();
            if stack.len() >= 32 || stack.contains(&key) {
                return Err(self.error(
                    span,
                    "recursive or deeply nested borrowed constants are unsupported",
                ));
            }
            stack.push(key);
        }
        let body = if let Some(promoted) = value.promoted {
            &self.tcx.promoted_mir(value.def)[promoted]
        } else {
            self.tcx.mir_for_ctfe(value.def)
        };
        let result = self.lower_body(body, vec![], &mut Vec::new());
        self.constants.borrow_mut().pop();
        result.map_err(Into::into)
    }

    pub(super) fn tree_discriminant_value(
        &self,
        body: &Body<'tcx>,
        place: Place<'tcx>,
        locals: &super::Environment,
        span: Span,
    ) -> Result<Expr, String> {
        let shape = self
            .tree_shape(place.ty(&body.local_decls, self.tcx).ty, span)?
            .ok_or_else(|| self.error(span, "discriminants require a borrowed tree"))?;
        let ty = Type::Int {
            bits: 8,
            signed: false,
        };
        Ok(Expr::If {
            condition: Box::new(Expr::TreeIsPair {
                value: Box::new(self.place(body, place, locals, span)?),
            }),
            then_value: Box::new(self.scalar(shape.pair.index() as u128, &ty, span)?),
            else_value: Box::new(self.scalar(shape.atom.index() as u128, &ty, span)?),
        })
    }
}
