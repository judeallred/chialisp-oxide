//! Closed zero-environment closure coercions; no capture/reference/trait API.
use super::Lowerer;
use chialisp_oxide_ir::{Parameter, Type};
use rustc_hir::{def::DefKind, def_id::DefId};
use rustc_middle::{
    middle::codegen_fn_attrs::CodegenFnAttrFlags,
    mir::{
        self, AggregateKind, Body, CastKind, Local, Location, Operand, Place, Rvalue,
        StatementKind,
        visit::{PlaceContext, Visitor},
    },
    ty::{self, Ty},
};
use rustc_span::Span;
use std::{collections::HashSet, rc::Rc};

pub(super) struct Plan<'tcx> {
    original: Ty<'tcx>,
    pub logical: Type,
    pub size: usize,
    inputs: Vec<Ty<'tcx>>,
    result: Ty<'tcx>,
}

impl<'tcx> Lowerer<'tcx> {
    pub(super) fn zero_closure_coercion(
        &self,
        source: Ty<'tcx>,
        target: Ty<'tcx>,
        span: Span,
    ) -> Result<(DefId, Type, usize), String> {
        let ty::Closure(id, args) = source.kind() else {
            return Err(self.error(span, "closure coercion requires an actual compiler closure"));
        };
        if !id.is_local() || self.tcx.def_kind(*id) != DefKind::Closure {
            return Err(self.error(span, "closure coercion requires a local compiler closure"));
        }
        // Bound the actual inference owner and closure HIR before capture/typeck
        // queries. No debug local metadata or source-name correspondence.
        let mut ancestor = *id;
        for depth in 0..=64 {
            if depth == 64 {
                return Err(self.error(span, "closure owner nesting exceeds 64"));
            }
            self.callable_charge(span, 1)?;
            if !self.tcx.is_typeck_child(ancestor) {
                break;
            }
            ancestor = self.tcx.parent(ancestor);
        }
        if self.tcx.def_kind(ancestor) != DefKind::Fn {
            return Err(self.error(span, "closures in constants/statics are unsupported"));
        }
        self.preflight_callable_owner(ancestor, span)?;
        self.preflight_callable_owner(*id, span)?;
        self.closure_identity_args(*id, args, span)?;
        let c = args.as_closure();
        if c.parent_args().len() > 64 {
            return Err(self.error(span, "closure parent metadata exceeds 64"));
        }
        self.callable_charge(span, c.parent_args().len() + 1)?;
        if c.parent_args().iter().any(|a| a.as_region().is_none()) {
            return Err(self.error(span, "generic type/const closure owners are unsupported"));
        }
        let tuple = c.tupled_upvars_ty();
        if !matches!(tuple.kind(),ty::Tuple(fields) if fields.is_empty())
            || !c.upvar_tys().is_empty()
            || !self.tcx.closure_captures(id.expect_local()).is_empty()
        {
            return Err(self.error(
                span,
                "only zero-environment noncapturing closure coercions are supported",
            ));
        }
        if c.kind() != ty::ClosureKind::Fn
            || source.needs_drop(self.tcx, ty::TypingEnv::post_analysis(self.tcx, ancestor))
        {
            return Err(self.error(
                span,
                "closure coercion requires immutable drop-free Fn metadata",
            ));
        }
        for owner in [ancestor, *id] {
            let attrs = self.tcx.codegen_fn_attrs(owner);
            if !attrs.target_features.is_empty() || attrs.flags.contains(CodegenFnAttrFlags::NAKED)
            {
                return Err(self.error(
                    span,
                    "closure target features and naked bodies are unsupported",
                ));
            }
        }
        let binder = c.sig();
        if binder.bound_vars().len() > 64 {
            return Err(self.error(span, "closure binder exceeds 64 lifetimes"));
        }
        self.callable_charge(span, binder.bound_vars().len() + 1)?;
        if binder
            .bound_vars()
            .iter()
            .any(|v| !matches!(v, ty::BoundVariableKind::Region(_)))
        {
            return Err(self.error(span, "closure binders must contain only lifetimes"));
        }
        let sig = binder.skip_binder();
        if sig.abi() != rustc_abi::ExternAbi::RustCall
            || sig.safety().is_unsafe()
            || sig.c_variadic()
            || sig.splatted().is_some()
            || sig.inputs().len() != 1
        {
            return Err(self.error(span, "invalid compiler closure RustCall signature"));
        }
        let ty::Tuple(tuple_inputs) = sig.inputs()[0].kind() else {
            return Err(self.error(
                span,
                "compiler closure signature needs one outer argument tuple",
            ));
        };
        if tuple_inputs.len() > 63 {
            return Err(self.error(
                span,
                "function exceeds 64 parameters or 2,048 MIR locals (closure receiver included)",
            ));
        }
        let (logical, size) = self.callable_type(target, span, 0)?;
        self.callable_charge(span, size + tuple_inputs.len() + 1)?;
        let flat = self.tcx.instantiate_bound_regions_with_erased(binder);
        let ty::Tuple(inputs) = flat.inputs()[0].kind() else {
            unreachable!()
        };
        let pointer = self
            .tcx
            .instantiate_bound_regions_with_erased(target.fn_sig(self.tcx));
        if inputs.len() != pointer.inputs().len() {
            return Err(self.error(span, "closure and pointer arities differ"));
        }
        for (a, b) in inputs.iter().chain(std::iter::once(flat.output())).zip(
            pointer
                .inputs()
                .iter()
                .copied()
                .chain(std::iter::once(pointer.output())),
        ) {
            if self.tcx.erase_and_anonymize_regions(a) != self.tcx.erase_and_anonymize_regions(b) {
                return Err(self.error(
                    span,
                    "closure coercion changes an original Rust argument or result type",
                ));
            }
        }
        // All metadata is bounded before clones; source ABI remains in original.
        let mut cache = self.zero_closures.borrow_mut();
        if let Some(old) = cache.get(id) {
            if old.original != source || old.logical != logical {
                return Err(self.error(span, "inconsistent closure coercion metadata"));
            }
        } else {
            if cache.len() >= 256 {
                return Err(self.error(span, "closure discovery exceeds 256 targets"));
            }
            cache.insert(
                *id,
                Rc::new(Plan {
                    original: source,
                    logical: logical.clone(),
                    size,
                    inputs: inputs.to_vec(),
                    result: flat.output(),
                }),
            );
        }
        Ok((*id, logical, size))
    }

    fn zero_closure_operand(
        &self,
        body: &Body<'tcx>,
        op: &Operand<'tcx>,
        expected: Ty<'tcx>,
        span: Span,
    ) -> Result<(), String> {
        if op.ty(&body.local_decls, self.tcx) != expected {
            return Err(self.error(span, "closure operand type differs from original metadata"));
        }
        match op {
            Operand::Copy(p)|Operand::Move(p) if p.projection.is_empty()=>Ok(()),
            Operand::Constant(c) if matches!(c.const_,mir::Const::Val(mir::ConstValue::ZeroSized,t) if t==expected)=>Ok(()),
            _=>Err(self.error(span,"closure operands must be closed locals or already-evaluated ZeroSized metadata; constant evaluation is unsupported")),
        }
    }
    pub(super) fn check_zero_closure_operand(
        &self,
        body: &Body<'tcx>,
        op: &Operand<'tcx>,
        span: Span,
    ) -> Result<(), String> {
        self.zero_closure_operand(body, op, op.ty(&body.local_decls, self.tcx), span)
    }

    pub(super) fn closure_body(&self, id: DefId) -> Result<&'tcx Body<'tcx>, String> {
        let span = self.tcx.def_span(id);
        let plan = self
            .zero_closures
            .borrow()
            .get(&id)
            .cloned()
            .ok_or_else(|| {
                self.error(span, "closure body has no validated safe pointer coercion")
            })?;
        if !id.is_local()
            || self.tcx.def_kind(id) != DefKind::Closure
            || !self.tcx.is_mir_available(id)
        {
            return Err(self.error(span, "closure body is not an available local compiler body"));
        }
        let body = self.tcx.optimized_mir(id);
        self.owned_plan(body)?;
        if body.arg_count != plan.inputs.len() + 1
            || body.arg_count > 64
            || body.spread_arg.is_some()
        {
            return Err(self.error(
                span,
                "closure MIR has an unsupported receiver/flat-argument ABI",
            ));
        }
        let receiver = body.local_decls[Local::from_usize(1)].ty;
        if !matches!(receiver.kind(),ty::Ref(_,inner,ty::Mutability::Not) if *inner==plan.original)
        {
            return Err(self.error(
                span,
                "closure receiver is not a shared reference to its exact original type",
            ));
        }
        for (local, expected) in body.args_iter().skip(1).zip(&plan.inputs) {
            if self
                .tcx
                .erase_and_anonymize_regions(body.local_decls[local].ty)
                != self.tcx.erase_and_anonymize_regions(*expected)
            {
                return Err(self.error(
                    span,
                    "closure flat MIR argument differs from original signature",
                ));
            }
        }
        if self.tcx.erase_and_anonymize_regions(body.return_ty())
            != self.tcx.erase_and_anonymize_regions(plan.result)
        {
            return Err(self.error(span, "closure MIR return differs from original signature"));
        }
        let receiver = HashSet::from([Local::from_usize(1)]);
        self.scan_closure_places(body, &receiver, true)?;
        Ok(body)
    }
    pub(super) fn closure_signature(
        &self,
        body: &Body<'tcx>,
    ) -> Result<Option<(Vec<Parameter>, Type)>, String> {
        if let Some(signature) = self.captured_signature(body)? {
            return Ok(Some(signature));
        }
        if self.tcx.def_kind(body.source.def_id()) != DefKind::Closure {
            return Ok(None);
        }
        let id = body.source.def_id();
        let p = self
            .zero_closures
            .borrow()
            .get(&id)
            .cloned()
            .ok_or_else(|| self.error(body.span, "closure signature was not admitted"))?;
        self.callable_charge(body.span, p.size)?;
        let Type::Callable { parameters, result } = &p.logical else {
            unreachable!()
        };
        Ok(Some((
            parameters
                .iter()
                .enumerate()
                .map(|(i, t)| Parameter {
                    name: format!("arg{i}"),
                    ty: t.clone(),
                })
                .collect(),
            *result.clone(),
        )))
    }
    pub(super) fn runtime_user_args<'b>(
        &self,
        body: &'b Body<'tcx>,
    ) -> impl Iterator<Item = Local> + 'b {
        body.args_iter().skip(usize::from(
            self.tcx.def_kind(body.source.def_id()) == DefKind::Closure
                && self.captured_plan(body.source.def_id()).is_none(),
        ))
    }
    pub(super) fn metadata_closure_assignment(
        &self,
        body: &Body<'tcx>,
        place: Place<'tcx>,
        value: &Rvalue<'tcx>,
        span: Span,
    ) -> Result<bool, String> {
        let rust_ty = place.ty(&body.local_decls, self.tcx).ty;
        let ty::Closure(id, _) = rust_ty.kind() else {
            return Ok(false);
        };
        if !place.projection.is_empty() || !self.zero_closures.borrow().contains_key(id) {
            return Err(self.error(span,"closure values require an explicit safe pointer coercion and cannot escape into storage"));
        }
        match value {
            Rvalue::Aggregate(k, ops)
                if matches!(&**k,AggregateKind::Closure(cid,args) if *cid==*id && Ty::new_closure(self.tcx,*cid,args)==rust_ty)
                    && ops.is_empty() => {}
            Rvalue::Use(op, _) => self.zero_closure_operand(body, op, rust_ty, span)?,
            _ => {
                return Err(self.error(
                    span,
                    "only empty closure construction and immutable local copies are supported",
                ));
            }
        }
        Ok(true)
    }
    pub(super) fn validate_closure_locals(&self, body: &Body<'tcx>) -> Result<(), String> {
        // Preserve the old v9 analysis budget/route without a closure coercion.
        if self.zero_closures.borrow().is_empty() {
            return Ok(());
        }
        let mut set = HashSet::new();
        for (local, decl) in body.local_decls.iter_enumerated() {
            self.callable_charge(body.span, 1)?;
            if matches!(decl.ty.kind(), ty::Closure(id,_) if self.captured_plan(*id).is_none()) {
                set.insert(local);
            }
        }
        if set.is_empty() {
            return Ok(());
        }
        self.scan_closure_places(body, &set, false)
    }
    fn scan_closure_places(
        &self,
        body: &Body<'tcx>,
        set: &HashSet<Local>,
        receiver: bool,
    ) -> Result<(), String> {
        let mut scan = UseScan {
            set,
            work: 0,
            bad: false,
        };
        for (block, data) in body.basic_blocks.iter_enumerated() {
            self.callable_charge(body.span, data.statements.len() + 1)?;
            for (i, s) in data.statements.iter().enumerate() {
                let loc = Location {
                    block,
                    statement_index: i,
                };
                match &s.kind {
                    StatementKind::StorageLive(_)
                    | StatementKind::StorageDead(_)
                    | StatementKind::Nop => (),
                    StatementKind::Assign(a) if !receiver && set.contains(&a.0.local) => {
                        self.metadata_closure_assignment(body, a.0, &a.1, s.source_info.span)?;
                    }
                    StatementKind::Assign(a)
                        if !receiver
                            && matches!(
                                &a.1,
                                Rvalue::Cast(
                                    CastKind::PointerCoercion(
                                        ty::adjustment::PointerCoercion::ClosureFnPointer(
                                            rustc_hir::Safety::Safe
                                        ),
                                        _
                                    ),
                                    _,
                                    _
                                )
                            ) =>
                    {
                        let Rvalue::Cast(_, op, _) = &a.1 else {
                            unreachable!()
                        };
                        self.check_zero_closure_operand(body, op, s.source_info.span)?;
                        scan.visit_place(
                            &a.0,
                            PlaceContext::MutatingUse(mir::visit::MutatingUseContext::Store),
                            loc,
                        );
                    }
                    _ => scan.visit_statement(s, loc),
                }
                if scan.bad || scan.work > super::MAX_EXPRESSION_WORK {
                    return Err(self.error(s.source_info.span,"closure values/receivers may only be constructed, copied and safely coerced; reference, call, projection and storage uses are unsupported"));
                }
            }
            scan.visit_terminator(
                data.terminator(),
                Location {
                    block,
                    statement_index: data.statements.len(),
                },
            );
            if scan.bad || scan.work > super::MAX_EXPRESSION_WORK {
                return Err(self.error(
                    body.span,
                    "closure value/receiver escapes through a call or terminator",
                ));
            }
        }
        self.callable_charge(body.span, scan.work)
    }
}
struct UseScan<'a> {
    set: &'a HashSet<Local>,
    work: usize,
    bad: bool,
}
impl<'tcx> Visitor<'tcx> for UseScan<'_> {
    fn visit_place(&mut self, p: &Place<'tcx>, _: PlaceContext, _: Location) {
        self.work = self.work.saturating_add(p.projection.len() + 1);
        self.bad |= self.set.contains(&p.local);
    }
    fn visit_rvalue(&mut self, v: &Rvalue<'tcx>, l: Location) {
        self.work = self.work.saturating_add(match v {
            Rvalue::Aggregate(_, ops) => ops.len() + 1,
            _ => 1,
        });
        if self.work <= super::MAX_EXPRESSION_WORK && !self.bad {
            self.super_rvalue(v, l);
        }
    }
    fn visit_terminator(&mut self, t: &mir::Terminator<'tcx>, l: Location) {
        self.work = self.work.saturating_add(match &t.kind {
            mir::TerminatorKind::Call { args, .. } => args.len() + 1,
            _ => 1,
        });
        if self.work <= super::MAX_EXPRESSION_WORK && !self.bad {
            self.super_terminator(t, l);
        }
    }
}

#[cfg(test)]
mod tests;

pub(super) mod promoted;
