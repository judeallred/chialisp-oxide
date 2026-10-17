//! Pointer-free interpretation of the compiler's closed standard-Box plumbing.
//!
//! The original typed MIR is never relabelled or mutated. A plan accounts for
//! every internal definition/use and captures the logical value at the pointer
//! origin. Only dereferences of that validated origin may read the capture.
use super::Lowerer;
use rustc_middle::{
    mir::{
        visit::{MutatingUseContext, NonMutatingUseContext, PlaceContext, Visitor},
        *,
    },
    ty::{self, Ty},
};
use rustc_span::{Span, sym};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

const MAX_WORK: usize = 500_000;

#[derive(Clone, Copy)]
pub(super) enum Action<'tcx> {
    Capture { pointer: Local, source: Place<'tcx> },
    Skip,
}
#[derive(Default)]
pub(super) struct Plan<'tcx> {
    pub actions: HashMap<Location, Action<'tcx>>,
    pub targets: HashMap<BasicBlock, BasicBlock>,
    pub roots: HashMap<Local, Ty<'tcx>>,
}

#[derive(Default)]
struct Uses<'tcx> {
    values: Vec<(Place<'tcx>, PlaceContext, Location)>,
    work: usize,
}
impl<'tcx> Visitor<'tcx> for Uses<'tcx> {
    fn visit_place(&mut self, place: &Place<'tcx>, context: PlaceContext, location: Location) {
        self.work = self.work.saturating_add(place.projection.len() + 1);
        if self.work <= MAX_WORK {
            self.values.push((*place, context, location));
        }
    }
    fn visit_rvalue(&mut self, value: &Rvalue<'tcx>, location: Location) {
        self.work = self.work.saturating_add(match value {
            Rvalue::Aggregate(_, operands) => operands.len() + 1,
            _ => 1,
        });
        if self.work <= MAX_WORK {
            self.super_rvalue(value, location);
        }
    }
    fn visit_terminator(&mut self, value: &Terminator<'tcx>, location: Location) {
        self.work = self.work.saturating_add(match &value.kind {
            TerminatorKind::Call { args, .. } => args.len() + 1,
            _ => 1,
        });
        if self.work <= MAX_WORK {
            self.super_terminator(value, location);
        }
    }
}

fn plain(operand: &Operand<'_>) -> Option<Local> {
    match operand {
        Operand::Copy(place) | Operand::Move(place) if place.projection.is_empty() => {
            Some(place.local)
        }
        _ => None,
    }
}

impl<'tcx> Lowerer<'tcx> {
    pub(super) fn check_owned_allocator(&self, span: Span) -> Result<(), String> {
        let explicit = if let Some(explicit) = self.explicit_allocator.get() {
            explicit
        } else {
            // This pinned query freezes the CStore's loaded-crate list before
            // returning it; caching cannot miss a later metadata-loaded crate.
            let crates = self.tcx.crates(());
            if crates.len() > 1024 {
                return Err(self.error(
                    span,
                    "owned allocator identity analysis exceeds 1,024 dependency crates",
                ));
            }
            let explicit = std::iter::once(rustc_hir::def_id::LOCAL_CRATE)
                .chain(crates.iter().copied())
                .any(|id| self.tcx.has_global_allocator(id));
            self.explicit_allocator.set(Some(explicit));
            explicit
        };
        if explicit {
            return Err(self.error(span,"owned trees do not support an explicit global allocator (#[global_allocator]) in the selected crate or its dependencies"));
        }
        Ok(())
    }
    pub(super) fn owned_tree_box(
        &self,
        rust_ty: Ty<'tcx>,
        span: Span,
    ) -> Result<Option<Ty<'tcx>>, String> {
        let Some(inner) = self.owned_box_inner(rust_ty) else {
            return Ok(None);
        };
        if self
            .tree_shape(inner, span)?
            .is_none_or(|shape| shape.ownership != super::trees::Ownership::OwnedBoxes)
        {
            return Err(self.error(span,"Box supports only an owned Atom/Pair(Box<Self>, Box<Self>) tree, not borrowed trees or other pointees"));
        }
        Ok(Some(inner))
    }
    /// The actual language-item Box with exactly its default Global allocator.
    /// The pointee is an enum; its full injective Tree shape is checked at use.
    pub(super) fn owned_box_inner(&self, rust_ty: Ty<'tcx>) -> Option<Ty<'tcx>> {
        let ty::Adt(definition, args) = rust_ty.kind() else {
            return None;
        };
        if Some(definition.did()) != self.tcx.lang_items().owned_box() || args.len() != 2 {
            return None;
        }
        let (Some(inner), Some(allocator)) = (args[0].as_type(), args[1].as_type()) else {
            return None;
        };
        let ty::Adt(global, global_args) = allocator.kind() else {
            return None;
        };
        (Some(global.did()) == self.tcx.lang_items().global_alloc_ty()
            && global_args.is_empty()
            && matches!(inner.kind(),ty::Adt(definition,_) if definition.is_enum()))
        .then_some(inner)
    }

    pub(super) fn box_constructor(
        &self,
        body: &Body<'tcx>,
        func: &Operand<'tcx>,
        args: &[rustc_span::Spanned<Operand<'tcx>>],
        destination: Place<'tcx>,
        span: Span,
    ) -> Result<bool, String> {
        let ty::FnDef(id, generic) = func.ty(&body.local_decls, self.tcx).kind() else {
            return Ok(false);
        };
        if !self.tcx.is_diagnostic_item(sym::box_new, *id) {
            return Ok(false);
        }
        let Some(owned) = self.tcx.lang_items().owned_box() else {
            return Ok(false);
        };
        if owned.krate != id.krate || id.is_local() {
            return Ok(false);
        }
        let rust_ty = destination.ty(&body.local_decls, self.tcx).ty;
        let Some(inner) = self.owned_tree_box(rust_ty, span)? else {
            return Err(self.error(
                span,
                "Box::new supports only a validated owned Tree and the default Global allocator",
            ));
        };
        if self.tree_shape(inner, span)?.is_none()
            || generic.skip_binder().len() != 1
            || generic.skip_binder()[0].as_type() != Some(inner)
            || args.len() != 1
            || self
                .tcx
                .erase_and_anonymize_regions(args[0].node.ty(&body.local_decls, self.tcx))
                != self.tcx.erase_and_anonymize_regions(inner)
        {
            return Err(self.error(span, "Box::new has an unsupported pointee or signature"));
        }
        Ok(true)
    }

    fn box_drop(
        &self,
        body: &Body<'tcx>,
        func: &Operand<'tcx>,
        args: &[rustc_span::Spanned<Operand<'tcx>>],
        span: Span,
    ) -> Result<Option<Local>, String> {
        let ty::FnDef(id, generic) = func.ty(&body.local_decls, self.tcx).kind() else {
            return Ok(None);
        };
        if !matches!(self.tcx.def_kind(*id), rustc_hir::def::DefKind::AssocFn)
            || self.tcx.associated_item(*id).trait_container(self.tcx)
                != self.tcx.lang_items().drop_trait()
            || args.len() != 1
        {
            return Ok(None);
        }
        let Some(argument) = plain(&args[0].node) else {
            return Ok(None);
        };
        let ty::Ref(_, boxed, ty::Mutability::Mut) = body.local_decls[argument].ty.kind() else {
            return Ok(None);
        };
        let Some(inner) = self.owned_tree_box(*boxed, span)? else {
            return Ok(None);
        };
        if self.tree_shape(inner, span)?.is_none() {
            return Ok(None);
        }
        let Some(owned) = self.tcx.lang_items().owned_box() else {
            return Ok(None);
        };
        let expected = self.tcx.adt_def(owned).destructor(self.tcx).map(|d| d.did);
        let resolved = ty::Instance::try_resolve(
            self.tcx,
            ty::TypingEnv::post_analysis(self.tcx, body.source.def_id()),
            *id,
            generic.skip_binder(),
        )
        .map_err(|_| self.error(span, "cannot resolve standard Box destructor"))?;
        Ok(resolved
            .filter(|instance| Some(instance.def_id()) == expected)
            .map(|_| argument))
    }

    pub(super) fn owned_plan(&self, body: &Body<'tcx>) -> Result<Rc<Plan<'tcx>>, String> {
        let key = std::ptr::from_ref(body) as usize;
        if let Some(plan) = self.owned_plans.borrow().get(&key) {
            return Ok(Rc::clone(plan));
        }
        if self.owned_plans.borrow().len() >= 256 {
            return Err(self.error(body.span, "owned MIR analysis exceeds 256 bodies"));
        }
        let plan = Rc::new(self.analyze_owned(body)?);
        self.owned_plans.borrow_mut().insert(key, Rc::clone(&plan));
        Ok(plan)
    }

    fn analyze_owned(&self, body: &Body<'tcx>) -> Result<Plan<'tcx>, String> {
        let mut count = 0usize;
        for data in body.basic_blocks.iter() {
            count = count.saturating_add(data.statements.len());
            if count > 8192 || body.basic_blocks.len() > 10_000 || body.local_decls.len() > 2048 {
                return Err(self.error(
                    body.span,
                    "owned MIR analysis exceeds 8,192 statements, 10,000 blocks or 2,048 locals",
                ));
            }
        }
        // The unchanged route avoids both copying MIR and introducing new IR.
        if !body
            .local_decls
            .iter()
            .any(|decl| self.owned_box_inner(decl.ty).is_some())
            && !body
                .basic_blocks
                .iter()
                .any(|block| matches!(block.terminator().kind, TerminatorKind::Drop { .. }))
        {
            return Ok(Plan::default());
        }
        let mut uses = Uses::default();
        uses.visit_body(body);
        if uses.work > MAX_WORK {
            return Err(self.error(body.span, "owned MIR analysis exceeds 500,000 use work"));
        }
        let mut definitions = HashMap::new();
        let mut multiple = HashSet::new();
        for (block, data) in body.basic_blocks.iter_enumerated() {
            for (statement_index, statement) in data.statements.iter().enumerate() {
                if let StatementKind::Assign(assignment) = &statement.kind {
                    let location = Location {
                        block,
                        statement_index,
                    };
                    if assignment.0.projection.is_empty()
                        && definitions
                            .insert(assignment.0.local, (location, &assignment.1))
                            .is_some()
                    {
                        multiple.insert(assignment.0.local);
                    }
                }
            }
        }
        let mut plan = Plan::default();
        let mut roots = HashMap::new();
        let mut internal = HashSet::new();
        let mut allowed = HashMap::<Local, HashSet<Location>>::new();
        let mut guard_work = uses.work.saturating_add(count);
        for (local, (location, value)) in &definitions {
            let Rvalue::Cast(CastKind::Transmute, Operand::Copy(pointer), target) = value else {
                continue;
            };
            let ty::RawPtr(pointee, ty::Mutability::Not) = target.kind() else {
                continue;
            };
            let projection = pointer.projection;
            if projection.len() < 2 {
                continue;
            }
            let source = Place {
                local: pointer.local,
                projection: self.tcx.mk_place_elems(&projection[..projection.len() - 2]),
            };
            let Some(inner) =
                self.owned_tree_box(source.ty(&body.local_decls, self.tcx).ty, body.span)?
            else {
                continue;
            };
            if self.tcx.erase_and_anonymize_regions(inner)
                != self.tcx.erase_and_anonymize_regions(*pointee)
                || self.tree_shape(inner, body.span)?.is_none()
            {
                return Err(self.error(body.span, "Box pointer origin has unsupported pointee"));
            }
            // Derive genuine Unique/NonNull field identities from actual Box.
            let mut current = source.ty(&body.local_decls, self.tcx).ty;
            for elem in &projection[projection.len() - 2..] {
                let ty::Adt(def, args) = current.kind() else {
                    return Err(self.error(body.span, "invalid Box pointer layout"));
                };
                let ProjectionElem::Field(index, field_ty) = elem else {
                    return Err(self.error(body.span, "invalid Box pointer projection"));
                };
                let expected = def
                    .non_enum_variant()
                    .fields
                    .get(*index)
                    .filter(|_| index.index() == 0)
                    .map(|field| field.ty(self.tcx, args).skip_norm_wip());
                if expected != Some(*field_ty) {
                    return Err(self.error(body.span, "invalid Box pointer field type"));
                }
                current = *field_ty;
            }
            if multiple.contains(local) || body.local_decls[*local].ty != *target {
                return Err(self.error(
                    body.span,
                    "Box pointer temporary must have one direct definition",
                ));
            }
            roots.insert(*local, (source, *location, *pointee));
            plan.roots.insert(*local, *pointee);
        }
        // Pointer metadata may only form exact compiler-inserted guards.
        for (block, data) in body.basic_blocks.iter_enumerated() {
            let terminator = data.terminator();
            let location = Location {
                block,
                statement_index: data.statements.len(),
            };
            if let TerminatorKind::Assert {
                cond,
                expected: true,
                msg,
                target,
                ..
            } = &terminator.kind
                && matches!(
                    **msg,
                    AssertKind::MisalignedPointerDereference { .. }
                        | AssertKind::NullPointerDereference
                        | AssertKind::NullReferenceConstructed
                )
            {
                let mut checker = Guard {
                    lower: self,
                    definitions: &definitions,
                    multiple: &multiple,
                    roots: &roots,
                    internal: &mut internal,
                    allowed: &mut allowed,
                    work: 0,
                };
                let root = checker.guard(cond, msg, location).ok_or_else(|| {
                    self.error(
                        terminator.source_info.span,
                        "unsupported pointer guard; only closed genuine Box metadata is accepted",
                    )
                })?;
                guard_work = guard_work.saturating_add(checker.work);
                if guard_work > MAX_WORK {
                    return Err(self.error(body.span, "owned MIR analysis exceeds 500,000 work"));
                }
                if !roots.contains_key(&root) {
                    return Err(self.error(body.span, "pointer guard lacks Box origin"));
                }
                plan.targets.insert(block, *target);
            }
            if let TerminatorKind::Call {
                func,
                args,
                target: Some(target),
                ..
            } = &terminator.kind
                && let Some(argument) =
                    self.box_drop(body, func, args, terminator.source_info.span)?
            {
                let Some((definition, Rvalue::Ref(_, BorrowKind::Mut { .. }, source))) =
                    definitions.get(&argument)
                else {
                    return Err(self.error(
                        body.span,
                        "standard Box cleanup requires a direct compiler borrow",
                    ));
                };
                if multiple.contains(&argument)
                    || self
                        .owned_tree_box(source.ty(&body.local_decls, self.tcx).ty, body.span)?
                        .is_none()
                {
                    return Err(self.error(body.span, "invalid Box cleanup borrow"));
                }
                internal.insert(argument);
                allowed
                    .entry(argument)
                    .or_default()
                    .extend([*definition, location]);
                plan.targets.insert(block, *target);
            }
            if let TerminatorKind::Drop { place, target, .. } = &terminator.kind {
                let rust_ty = place.ty(&body.local_decls, self.tcx).ty;
                if self.pure_owned_drop(rust_ty, terminator.source_info.span)? {
                    plan.targets.insert(block, *target);
                }
            }
        }
        let mut captured = HashSet::new();
        for (place, context, location) in &uses.values {
            if let Some((_, definition, _)) = roots.get(&place.local) {
                // Drop elaboration may destroy the old pointee on a Box
                // reassignment. It is pure cleanup, not a value read; do not
                // manufacture a snapshot of a possibly partially moved value.
                if *context == PlaceContext::MutatingUse(MutatingUseContext::Drop)
                    && location.statement_index
                        == body.basic_blocks[location.block].statements.len()
                    && plan.targets.contains_key(&location.block)
                    && matches!(&body.basic_blocks[location.block].terminator().kind,TerminatorKind::Drop{place:drop,..} if drop==place)
                {
                    continue;
                }
                let permitted = (*location == *definition
                    && place.projection.is_empty()
                    && *context == PlaceContext::MutatingUse(MutatingUseContext::Store))
                    || (place.projection.first() == Some(&ProjectionElem::Deref)
                        && matches!(
                            context,
                            PlaceContext::NonMutatingUse(
                                NonMutatingUseContext::Copy
                                    | NonMutatingUseContext::Move
                                    | NonMutatingUseContext::Inspect
                                    | NonMutatingUseContext::SharedBorrow
                            )
                        ));
                if permitted {
                    if !place.projection.is_empty() {
                        captured.insert(place.local);
                    }
                    continue;
                }
                if place.projection.is_empty()
                    && allowed
                        .get(&place.local)
                        .is_some_and(|locations| locations.contains(location))
                {
                    continue;
                }
                return Err(self.error(
                    body.source_info(*location).span,
                    format!("Box pointer metadata escapes its validated dereference/guard chain ({context:?} at {location:?}, {place:?})"),
                ));
            }
            if internal.contains(&place.local)
                && !allowed
                    .get(&place.local)
                    .is_some_and(|locations| locations.contains(location))
            {
                return Err(self.error(
                    body.span,
                    "Box guard/drop metadata escapes its closed use chain",
                ));
            }
        }
        for (pointer, (source, location, _)) in roots {
            plan.actions.insert(
                location,
                if captured.contains(&pointer) {
                    Action::Capture { pointer, source }
                } else {
                    Action::Skip
                },
            );
        }
        for local in internal {
            let Some((location, _)) = definitions.get(&local) else {
                return Err(self.error(body.span, "Box internal metadata has no unique definition"));
            };
            if multiple.contains(&local) {
                return Err(self.error(body.span, "Box internal metadata is overwritten"));
            }
            plan.actions.insert(*location, Action::Skip);
        }
        Ok(plan)
    }

    fn pure_owned_drop(&self, rust_ty: Ty<'tcx>, span: Span) -> Result<bool, String> {
        if self.owned_tree_box(rust_ty, span)?.is_some() {
            return Ok(true);
        }
        Ok(self
            .tree_shape(rust_ty, span)?
            .is_some_and(|shape| shape.ownership == super::trees::Ownership::OwnedBoxes))
    }
}

type Definitions<'a, 'tcx> = HashMap<Local, (Location, &'a Rvalue<'tcx>)>;
struct Guard<'a, 'b, 'tcx> {
    lower: &'a Lowerer<'tcx>,
    definitions: &'a Definitions<'b, 'tcx>,
    multiple: &'a HashSet<Local>,
    roots: &'a HashMap<Local, (Place<'tcx>, Location, Ty<'tcx>)>,
    internal: &'a mut HashSet<Local>,
    allowed: &'a mut HashMap<Local, HashSet<Location>>,
    work: usize,
}
impl<'b, 'tcx> Guard<'_, 'b, 'tcx> {
    fn definition(
        &mut self,
        operand: &Operand<'tcx>,
        use_at: Location,
    ) -> Option<(&'b Rvalue<'tcx>, Location)> {
        self.work += 1;
        if self.work > 64 {
            return None;
        }
        let local = plain(operand)?;
        if self.multiple.contains(&local) {
            return None;
        }
        let (location, value) = self.definitions.get(&local)?;
        self.internal.insert(local);
        self.allowed
            .entry(local)
            .or_default()
            .extend([*location, use_at]);
        Some((*value, *location))
    }
    fn number(&self, operand: &Operand<'tcx>, number: u128) -> bool {
        matches!(operand,Operand::Constant(c) if c.const_.ty()==self.lower.tcx.types.usize&&c.const_.try_eval_bits(self.lower.tcx,ty::TypingEnv::fully_monomorphized())==Some(number))
    }
    fn layout(&self, operand: &Operand<'tcx>, root: Local, align: bool) -> bool {
        let Operand::Constant(c) = operand else {
            return false;
        };
        let Const::Unevaluated(value, ty) = c.const_ else {
            return false;
        };
        Some(value.def)
            == if align {
                self.lower.tcx.lang_items().align_const()
            } else {
                self.lower.tcx.lang_items().size_const()
            }
            && ty == self.lower.tcx.types.usize
            && value.args.len() == 1
            && value.args[0].as_type().is_some_and(|ty| {
                self.lower.tcx.erase_and_anonymize_regions(ty)
                    == self
                        .lower
                        .tcx
                        .erase_and_anonymize_regions(self.roots[&root].2)
            })
    }
    fn address(&mut self, operand: &Operand<'tcx>, at: Location) -> Option<Local> {
        let (value, at) = self.definition(operand, at)?;
        let Rvalue::Cast(CastKind::Transmute, operand, target) = value else {
            return None;
        };
        if *target != self.lower.tcx.types.usize {
            return None;
        }
        let (value, at) = self.definition(operand, at)?;
        let Rvalue::Cast(CastKind::PtrToPtr, operand, target) = value else {
            return None;
        };
        if *target != Ty::new_imm_ptr(self.lower.tcx, self.lower.tcx.types.unit) {
            return None;
        }
        let root = plain(operand)?;
        if !self.roots.contains_key(&root) {
            return None;
        }
        self.allowed.entry(root).or_default().insert(at);
        Some(root)
    }
    fn guard(
        &mut self,
        operand: &Operand<'tcx>,
        message: &AssertMessage<'tcx>,
        at: Location,
    ) -> Option<Local> {
        let assertion_at = at;
        match message {
            AssertKind::MisalignedPointerDereference { required, found } => {
                let (value, at) = self.definition(operand, at)?;
                let Rvalue::BinaryOp(BinOp::Eq, args) = value else {
                    return None;
                };
                if !self.number(&args.1, 0) {
                    return None;
                }
                let (value, at) = self.definition(&args.0, at)?;
                let Rvalue::BinaryOp(BinOp::BitAnd, args) = value else {
                    return None;
                };
                let root = self.address(&args.0, at)?;
                let (value, _) = self.definition(&args.1, at)?;
                let Rvalue::BinaryOp(BinOp::Sub, args) = value else {
                    return None;
                };
                if !self.layout(&args.0, root, true)
                    || !self.number(&args.1, 1)
                    || !self.layout(required, root, true)
                {
                    return None;
                }
                // The diagnostic repeats the same address, without executing a
                // separate source expression. Account for that second use.
                let root_found = self.address(found, assertion_at)?;
                (root_found == root).then_some(root)
            }
            AssertKind::NullPointerDereference | AssertKind::NullReferenceConstructed => {
                let (value, at) = self.definition(operand, at)?;
                let Rvalue::UnaryOp(UnOp::Not, operand) = value else {
                    return None;
                };
                let (value, at) = self.definition(operand, at)?;
                let Rvalue::BinaryOp(BinOp::BitAnd, args) = value else {
                    return None;
                };
                let (left, left_at) = self.definition(&args.0, at)?;
                let Rvalue::BinaryOp(BinOp::Eq, left) = left else {
                    return None;
                };
                if !self.number(&left.1, 0) {
                    return None;
                }
                let root = self.address(&left.0, left_at)?;
                if matches!(message, AssertKind::NullReferenceConstructed) {
                    return matches!(&args.1,Operand::Constant(constant) if constant.const_.ty()==self.lower.tcx.types.bool && constant.const_.try_eval_bits(self.lower.tcx,ty::TypingEnv::fully_monomorphized())==Some(1)).then_some(root);
                }
                let (right, _) = self.definition(&args.1, at)?;
                let Rvalue::BinaryOp(BinOp::Ne, right) = right else {
                    return None;
                };
                (self.layout(&right.0, root, false) && self.number(&right.1, 0)).then_some(root)
            }
            _ => None,
        }
    }
}
