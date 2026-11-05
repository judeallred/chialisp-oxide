//! Instantiate only the pinned compiler's three closure-metadata slots in
//! bounded promoted MIR. This is not source type/const generic admission.
use super::*;
use rustc_middle::ty::{TypeSuperVisitable, TypeVisitable, TypeVisitor};
use std::ops::ControlFlow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Context<'tcx> {
    pub def: DefId,
    pub args: ty::GenericArgsRef<'tcx>,
}
impl<'tcx> Lowerer<'tcx> {
    pub(in crate::lower) fn closure_identity_args(
        &self,
        id: DefId,
        args: ty::GenericArgsRef<'tcx>,
        span: Span,
    ) -> Result<ty::GenericArgsRef<'tcx>, String> {
        let g = self.tcx.generics_of(id);
        if g.parent_count > 64 || g.own_params.len() != 3 || g.count() != args.len() {
            return Err(self.error(
                span,
                "closure generic inventory differs from parent plus three compiler slots",
            ));
        }
        self.callable_charge(span, g.count() + 4)?;
        for index in 0..g.parent_count {
            if !matches!(
                g.param_at(index, self.tcx).kind,
                ty::GenericParamDefKind::Lifetime
            ) || args[index].as_region().is_none()
            {
                return Err(self.error(
                    span,
                    "closure source type/const parent parameters are unsupported",
                ));
            }
        }
        // Pinned rustc generics_of.rs deliberately marks its dummy compiler
        // parameters synthetic:false; names are not an identity boundary.
        for (offset, p) in g.own_params.iter().enumerate() {
            if p.def_id != id
                || p.index as usize != g.parent_count + offset
                || p.pure_wrt_drop
                || !matches!(
                    p.kind,
                    ty::GenericParamDefKind::Type {
                        has_default: false,
                        synthetic: false
                    }
                )
            {
                return Err(self.error(
                    span,
                    "closure compiler suffix metadata differs from pinned inventory",
                ));
            }
        }
        Ok(ty::GenericArgs::identity_for_item(self.tcx, id))
    }
    pub(in crate::lower) fn closure_promoted_context(
        &self,
        v: mir::UnevaluatedConst<'tcx>,
        span: Span,
    ) -> Result<Option<Context<'tcx>>, String> {
        if v.promoted.is_none() || !v.def.is_local() || self.tcx.def_kind(v.def) != DefKind::Closure
        {
            return Ok(None);
        }
        if v.args.len() > 67 {
            return Err(self.error(
                span,
                "closure promoted metadata exceeds bounded parent/synthetic arguments",
            ));
        }
        self.callable_charge(span, v.args.len() + 1)?;
        let plans = self.zero_closures.borrow();
        let Some(plan) = plans.get(&v.def) else {
            return Ok(None);
        };
        let ty::Closure(id, actual) = plan.original.kind() else {
            unreachable!()
        };
        if *id != v.def {
            return Ok(None);
        }
        let identity = self.closure_identity_args(*id, actual, span)?;
        let context = Context {
            def: *id,
            args: actual,
        };
        // Source MIR must carry exact identity args. Only an explicitly active
        // body instantiated by this route may refer recursively with actuals.
        if v.args == identity
            || (v.args == *actual && self.active_zero_promoted.borrow().last() == Some(&context))
        {
            Ok(Some(context))
        } else {
            Ok(None)
        }
    }
    #[cfg(test)]
    pub(in crate::lower) fn zero_closure_promoted_args(
        &self,
        v: mir::UnevaluatedConst<'tcx>,
        span: Span,
    ) -> Result<bool, String> {
        Ok(self.closure_promoted_context(v, span)?.is_some())
    }
    pub(in crate::lower) fn instantiate_closure_promoted(
        &self,
        body: &'tcx Body<'tcx>,
        promoted: mir::Promoted,
        c: Context<'tcx>,
        span: Span,
    ) -> Result<&'tcx Body<'tcx>, String> {
        let key = (c.def, c.args, promoted);
        if let Some(body) = self.zero_promoted.borrow().get(&key) {
            return Ok(body);
        }
        self.preflight_promoted_clone(body, c.args, span)?;
        // Original arena MIR is unchanged. Only this checked metadata context
        // is substituted; no normalization query or additional CTFE is used.
        let instantiated = ty::EarlyBinder::bind(self.tcx, body.clone())
            .instantiate(self.tcx, c.args)
            .skip_norm_wip();
        let body = self.tcx.arena.alloc(instantiated);
        self.zero_promoted.borrow_mut().insert(key, body);
        Ok(body)
    }
    pub(in crate::lower) fn preflight_promoted_clone(
        &self,
        body: &Body<'tcx>,
        args: ty::GenericArgsRef<'tcx>,
        span: Span,
    ) -> Result<(), String> {
        if body.arg_count != 0
            || body.local_decls.len() > 2048
            || body.basic_blocks.len() > 10_000
            || body.coroutine.is_some()
            || body.injection_phase.is_some()
            || body.coverage_info_hi.is_some()
            || body.function_coverage_info.is_some()
        {
            return Err(self.error(
                span,
                "unsupported closure promoted body or original MIR limits",
            ));
        }
        let statements = body
            .basic_blocks
            .iter()
            .try_fold(0usize, |n, b| n.checked_add(b.statements.len()))
            .unwrap_or(usize::MAX);
        if statements > 8192 {
            return Err(self.error(span, "closure promoted statement bound exceeds 8,192"));
        }
        let roots = statements
            .saturating_add(body.basic_blocks.len())
            .saturating_add(body.local_decls.len())
            .saturating_add(body.source_scopes.len())
            .saturating_add(body.user_type_annotations.len())
            .saturating_add(body.var_debug_info.len())
            .saturating_add(body.required_consts.as_ref().map_or(0, Vec::len))
            .saturating_add(body.mentioned_items.as_ref().map_or(0, Vec::len));
        self.callable_charge(span, roots)?;
        let mut mir = CloneWork {
            work: 0,
            limit: super::super::MAX_EXPRESSION_WORK.saturating_sub(self.expression_work.get()),
        };
        mir.visit_body(body);
        self.callable_charge(span, mir.work)?;
        self.callable_charge(span, args.len() + 1)?;
        let mut types = Types {
            nodes: 0,
            depth: 0,
            bad: false,
            parameters: vec![0; args.len()],
            allow_params: true,
            limit: super::super::MAX_EXPRESSION_WORK.saturating_sub(self.expression_work.get()),
        };
        if body.visit_with(&mut types).is_break() || types.bad {
            return Err(self.error(span,"closure promoted type preflight exceeds depth32/nodes20,000 or has unexpected parameters"));
        }
        self.callable_charge(span, types.nodes)?;
        let mut projected = 0usize;
        for (index, arg) in args.iter().enumerate() {
            let mut actual = Types {
                nodes: 0,
                depth: 0,
                bad: false,
                parameters: vec![],
                allow_params: false,
                limit: super::super::MAX_EXPRESSION_WORK.saturating_sub(self.expression_work.get()),
            };
            if arg.visit_with(&mut actual).is_break() || actual.bad {
                return Err(self.error(
                    span,
                    "closure actual metadata exceeds bounded type preflight",
                ));
            }
            self.callable_charge(span, actual.nodes + 1)?;
            projected =
                projected.saturating_add(types.parameters[index].saturating_mul(actual.nodes));
        }
        self.callable_charge(span, projected)?;
        Ok(())
    }
}
struct CloneWork {
    work: usize,
    limit: usize,
}
impl CloneWork {
    fn add(&mut self, n: usize) -> bool {
        self.work = self.work.saturating_add(n);
        self.work <= self.limit
    }
}
impl<'tcx> Visitor<'tcx> for CloneWork {
    fn visit_basic_block_data(&mut self, b: mir::BasicBlock, d: &mir::BasicBlockData<'tcx>) {
        if self.add(d.statements.len() + d.after_last_stmt_debuginfos.len() + 1) {
            self.super_basic_block_data(b, d);
        }
    }
    fn visit_statement(&mut self, s: &mir::Statement<'tcx>, l: Location) {
        if self.add(s.debuginfos.len() + 1) {
            self.super_statement(s, l);
        }
    }
    fn visit_statement_debuginfo(&mut self, d: &mir::StmtDebugInfo<'tcx>, l: Location) {
        if self.add(1) {
            self.super_statement_debuginfo(d, l);
        }
    }

    fn visit_ty(&mut self, _: Ty<'tcx>, _: mir::visit::TyContext) {}
    fn visit_args(&mut self, args: &ty::GenericArgsRef<'tcx>, _: Location) {
        self.add(args.len());
    }
    fn visit_place(&mut self, p: &Place<'tcx>, _: PlaceContext, _: Location) {
        self.add(p.projection.len() + 1);
    }
    fn visit_user_type_projection(&mut self, p: &mir::UserTypeProjection) {
        self.add(p.projs.len() + 1);
    }
    fn visit_local_decl(&mut self, l: Local, d: &mir::LocalDecl<'tcx>) {
        if self.add(d.user_ty.as_ref().map_or(0, |p| p.contents.len()) + 1) {
            self.super_local_decl(l, d);
        }
    }
    fn visit_var_debug_info(&mut self, v: &mir::VarDebugInfo<'tcx>) {
        if self.add(v.composite.as_ref().map_or(0, |p| p.projection.len()) + 1) {
            self.super_var_debug_info(v);
        }
    }
    fn visit_rvalue(&mut self, v: &Rvalue<'tcx>, l: Location) {
        if self.add(match v {
            Rvalue::Aggregate(_, ops) => ops.len() + 1,
            _ => 1,
        }) {
            self.super_rvalue(v, l);
        }
    }
    fn visit_terminator(&mut self, t: &mir::Terminator<'tcx>, l: Location) {
        if self.add(match &t.kind {
            mir::TerminatorKind::Call { args, .. } => args.len() + 1,
            mir::TerminatorKind::SwitchInt { targets, .. } => targets.all_targets().len() + 1,
            mir::TerminatorKind::InlineAsm { .. } => self.limit.saturating_add(1),
            _ => 1,
        }) {
            self.super_terminator(t, l);
        }
    }
}
struct Types {
    nodes: usize,
    depth: usize,
    bad: bool,
    parameters: Vec<usize>,
    allow_params: bool,
    limit: usize,
}
impl Types {
    fn count(&mut self, n: usize) -> ControlFlow<()> {
        self.nodes = self.nodes.saturating_add(n);
        if self.nodes > 20_000 || self.nodes > self.limit || self.depth >= 32 {
            self.bad = true;
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }
}
impl<'tcx> TypeVisitor<ty::TyCtxt<'tcx>> for Types {
    type Result = ControlFlow<()>;
    fn visit_ty(&mut self, t: Ty<'tcx>) -> Self::Result {
        self.count(1)?;
        if let ty::Param(p) = t.kind() {
            if !self.allow_params
                || p.index as usize >= self.parameters.len()
                || p.index as usize + 3 < self.parameters.len()
            {
                self.bad = true;
                return ControlFlow::Break(());
            }
            self.parameters[p.index as usize] += 1;
        }
        self.depth += 1;
        let r = t.super_visit_with(self);
        self.depth -= 1;
        r
    }
    fn visit_const(&mut self, c: ty::Const<'tcx>) -> Self::Result {
        self.count(1)?;
        if matches!(c.kind(), ty::ConstKind::Param(_)) {
            self.bad = true;
            return ControlFlow::Break(());
        }
        self.depth += 1;
        let r = c.super_visit_with(self);
        self.depth -= 1;
        r
    }
    fn visit_region(&mut self, _: ty::Region<'tcx>) -> Self::Result {
        self.count(1)
    }
    fn visit_binder<T: TypeVisitable<ty::TyCtxt<'tcx>>>(
        &mut self,
        b: &ty::Binder<'tcx, T>,
    ) -> Self::Result {
        if b.bound_vars().len() > 64 {
            self.bad = true;
            return ControlFlow::Break(());
        }
        self.count(b.bound_vars().len() + 1)?;
        self.depth += 1;
        let r = b.super_visit_with(self);
        self.depth -= 1;
        r
    }
}
