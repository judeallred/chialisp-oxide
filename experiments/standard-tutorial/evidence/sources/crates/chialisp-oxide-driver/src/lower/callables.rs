//! Internal immutable local function-pointer values. This does not admit a
//! callable ABI, closure captures, Fn traits, machine addresses or foreign code.
use super::Lowerer;
use chialisp_oxide_ir::{Expr, Type};
use rustc_hir::{Safety, def::DefKind, def_id::DefId};
use rustc_middle::{
    mir::{Body, CastKind, Rvalue, StatementKind, TerminatorKind},
    ty::{self, Ty},
};
use rustc_span::Span;

struct SignatureScope<'a>(&'a std::cell::Cell<bool>);
impl Drop for SignatureScope<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

impl<'tcx> Lowerer<'tcx> {
    pub(super) fn callable_charge(&self, span: Span, amount: usize) -> Result<(), String> {
        let work = self.expression_work.get().saturating_add(amount);
        if work > super::MAX_EXPRESSION_WORK {
            return Err(self.error(span, "callable metadata/copy work limit exceeded"));
        }
        self.expression_work.set(work);
        Ok(())
    }
    pub(super) fn callable_type(
        &self,
        rust_ty: Ty<'tcx>,
        span: Span,
        depth: usize,
    ) -> Result<(Type, usize), String> {
        if !matches!(rust_ty.kind(), ty::FnPtr(..)) {
            return Err(self.error(span, "expected an actual Rust function-pointer type"));
        }
        if self.callable_signature_active.replace(true) {
            return Err(self.error(span, "callable signatures cannot contain another callable"));
        }
        let _scope = SignatureScope(&self.callable_signature_active);
        self.callable_charge(span, 1)?;
        let binder = rust_ty.fn_sig(self.tcx);
        if binder.bound_vars().len() <= 64 {
            self.callable_charge(span, binder.bound_vars().len())?;
        }
        if binder.bound_vars().len() > 64
            || binder
                .bound_vars()
                .iter()
                .any(|var| !matches!(var, ty::BoundVariableKind::Region(_)))
        {
            return Err(self.error(
                span,
                "callable binders may contain only bounded lifetime parameters",
            ));
        }
        let signature = binder.skip_binder();
        if signature.abi() != rustc_abi::ExternAbi::Rust
            || signature.safety().is_unsafe()
            || signature.c_variadic()
            || signature.splatted().is_some()
        {
            return Err(self.error(
                span,
                "callables require safe non-variadic ordinary Rust signatures",
            ));
        }
        if signature.inputs().len() > 64 {
            return Err(self.error(span, "callables are limited to 64 parameters"));
        }
        let mut parameters = Vec::new();
        let mut nodes = 1usize;
        for input in signature.inputs() {
            let (input, count) = self.lower_type(*input, span, depth + 1)?;
            if matches!(input, Type::Callable { .. }) {
                return Err(self.error(
                    span,
                    "callable target signatures cannot contain another callable",
                ));
            }
            nodes = nodes.saturating_add(count);
            if nodes > 4096 {
                return Err(self.error(
                    span,
                    "callable signature type expansion exceeds 4,096 nodes",
                ));
            }
            parameters.push(input);
        }
        let (result, count) = self.lower_type(signature.output(), span, depth + 1)?;
        if matches!(result, Type::Callable { .. }) || nodes.saturating_add(count) > 4096 {
            return Err(self.error(span, "callable results must be bounded non-callable values"));
        }
        Ok((
            Type::Callable {
                parameters,
                result: Box::new(result),
            },
            nodes + count,
        ))
    }

    /// Inspect the real reification coercion and FnDef before any constant
    /// evaluation. The target's full body is separately discovered/validated.
    pub(super) fn reified_callable(
        &self,
        body: &Body<'tcx>,
        value: &Rvalue<'tcx>,
        span: Span,
    ) -> Result<Option<(DefId, Type, usize)>, String> {
        let Rvalue::Cast(CastKind::PointerCoercion(coercion, _), operand, target) = value else {
            return Ok(None);
        };
        if !matches!(target.kind(), ty::FnPtr(..)) {
            return Ok(None);
        }
        if *coercion == ty::adjustment::PointerCoercion::ClosureFnPointer(Safety::Safe) {
            self.check_zero_closure_operand(body, operand, span)?;
            return self
                .zero_closure_coercion(operand.ty(&body.local_decls, self.tcx), *target, span)
                .map(Some);
        }
        if *coercion != ty::adjustment::PointerCoercion::ReifyFnPointer(Safety::Safe) {
            return Err(self.error(span, "only safe local function-item or zero-environment closure coercions are supported; other pointer conversions are unsupported"));
        }
        let source = operand.ty(&body.local_decls, self.tcx);
        let ty::FnDef(id, args) = source.kind() else {
            return Err(self.error(
                span,
                "function-pointer reification requires an actual function item",
            ));
        };
        if !id.is_local() || self.tcx.def_kind(*id) != DefKind::Fn {
            return Err(self.error(
                span,
                "function-pointer targets must be ordinary local function items",
            ));
        }
        let generic_count = self.tcx.generics_of(*id).count();
        if generic_count > 64 || args.skip_binder().len() > 64 {
            return Err(self.error(
                span,
                "function-pointer generic metadata exceeds 64 parameters",
            ));
        }
        self.callable_charge(
            span,
            generic_count
                .saturating_add(args.skip_binder().len())
                .saturating_add(1),
        )?;
        if !self.lifetime_parameters_only(*id)
            || args
                .skip_binder()
                .iter()
                .any(|arg| arg.as_region().is_none())
        {
            return Err(self.error(
                span,
                "generic type/const function-pointer targets are unsupported",
            ));
        }
        let item_binder = self.tcx.fn_sig(*id).instantiate_identity().skip_norm_wip();
        if item_binder.bound_vars().len() <= 64 {
            self.callable_charge(span, item_binder.bound_vars().len())?;
        }
        if item_binder.bound_vars().len() > 64
            || item_binder
                .bound_vars()
                .iter()
                .any(|var| !matches!(var, ty::BoundVariableKind::Region(_)))
        {
            return Err(self.error(
                span,
                "function-item binders may contain only bounded lifetimes",
            ));
        }
        let item_header = item_binder.skip_binder();
        let pointer_header = target.fn_sig(self.tcx).skip_binder();
        if item_header.abi() != rustc_abi::ExternAbi::Rust
            || item_header.safety().is_unsafe()
            || item_header.c_variadic()
            || item_header.splatted().is_some()
            || item_header.abi() != pointer_header.abi()
            || item_header.safety() != pointer_header.safety()
            || item_header.c_variadic() != pointer_header.c_variadic()
            || item_header.splatted() != pointer_header.splatted()
        {
            return Err(self.error(
                span,
                "function-item ABI/safety/variadic metadata is outside the closed callable profile",
            ));
        }
        let (logical, size) = self.callable_type(*target, span, 0)?;
        // Both binders were checked above through the actual item/pointer
        // signatures. Instantiate only their admitted lifetime binders before
        // comparing original nominal Rust input/result types.
        let item_signature = self.tcx.instantiate_bound_regions_with_erased(item_binder);
        let pointer_signature = self
            .tcx
            .instantiate_bound_regions_with_erased(target.fn_sig(self.tcx));
        self.callable_charge(span, item_signature.inputs().len().saturating_add(1))?;
        if item_signature.abi() != pointer_signature.abi()
            || item_signature.safety() != pointer_signature.safety()
            || item_signature.c_variadic() != pointer_signature.c_variadic()
            || item_signature.splatted() != pointer_signature.splatted()
            || item_signature.inputs().len() != pointer_signature.inputs().len()
        {
            return Err(self.error(span, "function item and function-pointer signatures differ"));
        }
        for (source, target) in item_signature
            .inputs()
            .iter()
            .copied()
            .chain(std::iter::once(item_signature.output()))
            .zip(
                pointer_signature
                    .inputs()
                    .iter()
                    .copied()
                    .chain(std::iter::once(pointer_signature.output())),
            )
        {
            // Original types have been bounded/validated above. Lifetime erasure
            // cannot make different nominal Rust parameter types interchangeable.
            if self.tcx.erase_and_anonymize_regions(source)
                != self.tcx.erase_and_anonymize_regions(target)
            {
                return Err(self.error(
                    span,
                    "function-pointer coercion changes an original Rust parameter or result type",
                ));
            }
        }
        Ok(Some((*id, logical, size)))
    }

    /// Charge a complete new-capability scan only for bodies with FnPtr locals.
    /// An unused uninitialized declaration does not select a new IR route.
    pub(super) fn callable_references(
        &self,
        definition: DefId,
        body: &Body<'tcx>,
    ) -> Result<Vec<(DefId, Type, usize)>, String> {
        if !body
            .local_decls
            .iter()
            .any(|local| matches!(local.ty.kind(), ty::FnPtr(..)))
        {
            return Ok(vec![]);
        }
        let mut references = Vec::new();
        let mut active = body
            .args_iter()
            .any(|local| matches!(body.local_decls[local].ty.kind(), ty::FnPtr(..)));
        for data in body.basic_blocks.iter() {
            self.callable_charge(body.span, 1)?;
            for statement in &data.statements {
                self.expression_work
                    .set(self.expression_work.get().saturating_add(1));
                if self.expression_work.get() > super::MAX_EXPRESSION_WORK {
                    return Err(self.error(body.span, "callable discovery work limit exceeded"));
                }
                if let StatementKind::Assign(assignment) = &statement.kind
                    && let Some((id, signature, size)) =
                        self.reified_callable(body, &assignment.1, statement.source_info.span)?
                {
                    active = true;
                    self.callable_charge(body.span, references.len())?;
                    if !references.iter().any(|(existing, _, _)| *existing == id) {
                        if references.len() >= 256 {
                            return Err(self.error(
                                body.span,
                                "callable target discovery exceeds 256 functions",
                            ));
                        }
                        references.push((id, signature, size));
                    }
                }
            }
            if let TerminatorKind::Call { func, .. } = &data.terminator().kind {
                active |= matches!(func.ty(&body.local_decls, self.tcx).kind(), ty::FnPtr(..));
            }
        }
        if active {
            if !definition.is_local() {
                return Err(self.error(
                    body.span,
                    "callable helper values are restricted to local source bodies",
                ));
            }
            for declaration in body.local_decls.iter() {
                if matches!(declaration.ty.kind(), ty::FnPtr(..)) {
                    self.callable_type(declaration.ty, declaration.source_info.span, 0)?;
                }
            }
        }
        self.validate_closure_locals(body)?;
        Ok(references)
    }

    pub(super) fn callable_value(
        &self,
        body: &Body<'tcx>,
        value: &Rvalue<'tcx>,
        span: Span,
    ) -> Result<Option<Expr>, String> {
        let Some((id, _, _)) = self.reified_callable(body, value, span)? else {
            return Ok(None);
        };
        if !self.constants.borrow().is_empty() {
            return Err(self.error(
                span,
                "function pointers are unsupported in compile-time constants",
            ));
        }
        let function_id = self
            .callable_ids
            .borrow()
            .get(&id)
            .copied()
            .ok_or_else(|| {
                self.error(
                    span,
                    "function pointer requires a complete strict runtime function table",
                )
            })?;
        Ok(Some(Expr::FunctionRef { function_id }))
    }
}

/// Optimized MIR clears diagnostic LocalDecl::local_info even for some local
/// owners. Immutability is therefore a conservative whole-owner typed-HIR rule,
/// never a debug-name/span correspondence between HIR and MIR locals.
struct BindingScan<'a, 'tcx> {
    lower: &'a Lowerer<'tcx>,
    types: Option<&'tcx ty::TypeckResults<'tcx>>,
    nodes: usize,
    depth: usize,
    error: Option<String>,
}
impl BindingScan<'_, '_> {
    fn enter(&mut self, span: Span) -> bool {
        if self.error.is_some() {
            return false;
        }
        if self.nodes >= 10_000 || self.depth >= 64 {
            self.error = Some(self.lower.error(
                span,
                "callable owner HIR walk exceeds 10,000 nodes or depth 64",
            ));
            return false;
        }
        if let Err(error) = self.lower.callable_charge(span, 1) {
            self.error = Some(error);
            return false;
        }
        self.nodes += 1;
        self.depth += 1;
        true
    }
}
impl<'tcx> rustc_hir::intravisit::Visitor<'tcx> for BindingScan<'_, 'tcx> {
    type Result = std::ops::ControlFlow<()>;
    // Separate nested owners are independently admitted by DefId. Type/path
    // syntax cannot contain a runtime binding of the current owner.
    fn visit_ty(&mut self, node: &'tcx rustc_hir::Ty<'tcx, rustc_hir::AmbigArg>) -> Self::Result {
        if !self.enter(node.span) {
            return std::ops::ControlFlow::Break(());
        }
        self.depth -= 1;
        std::ops::ControlFlow::Continue(())
    }
    fn visit_path(&mut self, node: &rustc_hir::Path<'tcx>, _id: rustc_hir::HirId) -> Self::Result {
        if !self.enter(node.span) {
            return std::ops::ControlFlow::Break(());
        }
        self.depth -= 1;
        std::ops::ControlFlow::Continue(())
    }
    fn visit_expr(&mut self, node: &'tcx rustc_hir::Expr<'tcx>) -> Self::Result {
        if !self.enter(node.span) {
            return std::ops::ControlFlow::Break(());
        }
        if let Some(types) = self.types
            && !self.lower.captured_closures.borrow().is_empty()
            && matches!(node.kind, rustc_hir::ExprKind::AddrOf(..))
            && matches!(types.node_type_opt(node.hir_id).map(|t|t.kind()),Some(ty::Ref(_,inner,_)) if matches!(inner.kind(),ty::Closure(..)))
        {
            self.error=Some(self.lower.error(node.span,"explicit source references to closure values are unsupported; only compiler call autorefs are admitted"));
        }
        let result = if self.error.is_some() {
            std::ops::ControlFlow::Break(())
        } else {
            rustc_hir::intravisit::walk_expr(self, node)
        };
        self.depth -= 1;
        result
    }
    fn visit_pat(&mut self, node: &'tcx rustc_hir::Pat<'tcx>) -> Self::Result {
        if !self.enter(node.span) {
            return std::ops::ControlFlow::Break(());
        }
        if let (Some(types), rustc_hir::PatKind::Binding(mode, binding, _, _)) =
            (self.types, &node.kind)
            && mode.1.is_mut()
        {
            match types
                .node_type_opt(*binding)
                .or_else(|| types.node_type_opt(node.hir_id))
            {
                Some(ty)
                    if matches!(ty.kind(), ty::FnPtr(..))
                        || (matches!(ty.kind(), ty::Closure(..))
                            && (!self.lower.zero_closures.borrow().is_empty()
                                || !self.lower.captured_closures.borrow().is_empty())) =>
                {
                    self.error = Some(self.lower.error(
                        node.span,
                        "callable source bindings must be immutable (whole-owner HIR rule)",
                    ))
                }
                None => {
                    self.error = Some(self.lower.error(
                        node.span,
                        "callable owner binding lacks checked HIR type metadata",
                    ))
                }
                _ => (),
            }
        }
        let result = if self.error.is_some() {
            std::ops::ControlFlow::Break(())
        } else {
            rustc_hir::intravisit::walk_pat(self, node)
        };
        self.depth -= 1;
        result
    }
    fn visit_block(&mut self, node: &'tcx rustc_hir::Block<'tcx>) -> Self::Result {
        if !self.enter(node.span) {
            return std::ops::ControlFlow::Break(());
        }
        let result = rustc_hir::intravisit::walk_block(self, node);
        self.depth -= 1;
        result
    }
    fn visit_stmt(&mut self, node: &'tcx rustc_hir::Stmt<'tcx>) -> Self::Result {
        if !self.enter(node.span) {
            return std::ops::ControlFlow::Break(());
        }
        let result = rustc_hir::intravisit::walk_stmt(self, node);
        self.depth -= 1;
        result
    }
}
impl<'tcx> Lowerer<'tcx> {
    pub(super) fn preflight_callable_owner(
        &self,
        definition: DefId,
        span: Span,
    ) -> Result<(), String> {
        use rustc_hir::intravisit::Visitor;
        let local = definition
            .as_local()
            .ok_or_else(|| self.error(span, "callable owner must be local"))?;
        let body = self
            .tcx
            .hir_maybe_body_owned_by(local)
            .ok_or_else(|| self.error(span, "callable owner lacks an actual HIR body"))?;
        let mut preflight = BindingScan {
            lower: self,
            types: None,
            nodes: 0,
            depth: 0,
            error: None,
        };
        let _ = preflight.visit_body(body);
        preflight.error.map_or(Ok(()), Err)
    }
    pub(super) fn immutable_callable_owner(
        &self,
        definition: DefId,
        span: Span,
    ) -> Result<(), String> {
        use rustc_hir::intravisit::Visitor;
        let local = definition
            .as_local()
            .ok_or_else(|| self.error(span, "callable owner must be local"))?;
        let hir_body = self
            .tcx
            .hir_maybe_body_owned_by(local)
            .ok_or_else(|| self.error(span, "callable owner lacks an actual local HIR body"))?;
        let mut preflight = BindingScan {
            lower: self,
            types: None,
            nodes: 0,
            depth: 0,
            error: None,
        };
        let _ = preflight.visit_body(hir_body);
        if let Some(error) = preflight.error {
            return Err(error);
        }
        // The owner was type/borrow checked by rustc before our callback. Charge
        // its bounded shape before obtaining the cached typecheck result.
        self.callable_charge(span, preflight.nodes.saturating_add(1))?;
        let types = self.tcx.typeck(local);
        let mut scan = BindingScan {
            lower: self,
            types: Some(types),
            nodes: 0,
            depth: 0,
            error: None,
        };
        let _ = scan.visit_body(hir_body);
        match scan.error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}
