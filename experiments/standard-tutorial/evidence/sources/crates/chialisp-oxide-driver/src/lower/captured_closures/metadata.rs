use super::*;
use rustc_hir::{CaptureBy, ClosureKind, Constness, ExprKind, Node};
use rustc_middle::{middle::codegen_fn_attrs::CodegenFnAttrFlags, ty::UpvarCapture};
use rustc_span::Spanned;
impl<'tcx> Lowerer<'tcx> {
    pub(super) fn capture_type(
        &self,
        root: Ty<'tcx>,
        owner: DefId,
        span: Span,
    ) -> Result<(Type, usize), String> {
        let mut pending = vec![(root, 0usize)];
        let mut nodes = 0usize;
        while let Some((ty, depth)) = pending.pop() {
            nodes += 1;
            self.callable_charge(span, 1)?;
            if nodes > 4096 || depth >= 32 {
                return Err(self.error(
                    span,
                    "captured original-type profile exceeds 4096 nodes or depth32",
                ));
            }
            match ty.kind() {
                ty::Bool|ty::Int(_)|ty::Uint(_) if !matches!(ty.kind(),ty::Int(ty::IntTy::Isize)|ty::Uint(ty::UintTy::Usize))=>(),
                ty::Tuple(items)=>{
                    if items.len()>256 || items.len()>4096usize.saturating_sub(nodes+pending.len()){return Err(self.error(span,"captured tuple expansion exceeds existing bounds"));}
                    self.callable_charge(span,items.len())?;pending.extend(items.iter().map(|t|(t,depth+1)));
                }
                ty::Array(item,len)=>{
                    let length=len.try_to_target_usize(self.tcx).filter(|n|*n<=if *item==self.tcx.types.u8{4096}else{256}).ok_or_else(||self.error(span,"captured arrays require existing evaluated fixed bounds"))? as usize;
                    // Validate even an empty array's original element type.
                    let expanded=if *item==self.tcx.types.u8{1}else{length.max(1)};
                    if expanded>4096usize.saturating_sub(nodes+pending.len()){return Err(self.error(span,"captured array type expansion exceeds existing bounds"));}
                    self.callable_charge(span,expanded)?;pending.extend(std::iter::repeat_n((*item,depth+1),expanded));
                }
                ty::Ref(_,inner,ty::Mutability::Not) if self.tree_shape(*inner,span)?.is_some()=>(),
                ty::Adt(..) if self.tree_shape(ty,span)?.is_some()=>(),
                _=>return Err(self.error(span,"captured values support only closed Copy primitives, fixed tuples/arrays and actual immutable SDK Tree values; other original types are unsupported")),
            }
        }
        let env = ty::TypingEnv::post_analysis(self.tcx, owner);
        if root.needs_drop(self.tcx, env) || !self.tcx.type_is_copy_modulo_regions(env, root) {
            return Err(self.error(span, "captured values must be Copy and drop-free"));
        }
        let (logical, size) = self.lower_type(root, span, 0)?;
        self.callable_charge(span, size)?;
        Ok((logical, size))
    }
    pub(super) fn admit_captured_closure(
        &self,
        id: DefId,
        args: ty::GenericArgsRef<'tcx>,
        span: Span,
    ) -> Result<(), String> {
        if !id.is_local()
            || self.tcx.def_kind(id) != DefKind::Closure
            || args.len() > 67
            || args.len() < 3
        {
            return Err(self.error(
                span,
                "captures require bounded actual local compiler closure metadata",
            ));
        }
        if let Some(old) = self.captured_plan(id) {
            if old.args != args {
                return Err(self.error(
                    span,
                    "one capture helper has inconsistent original closure arguments",
                ));
            }
            return Ok(());
        }
        let owner = self.tcx.parent(id);
        if self.tcx.def_kind(owner) != DefKind::Fn || !self.lifetime_parameters_only(owner) {
            return Err(self.error(span,"captures require one local nongeneric function owner; nested/generic/constant owners are unsupported"));
        }
        self.preflight_callable_owner(owner, span)?;
        self.preflight_callable_owner(id, span)?;
        self.closure_identity_args(id, args, span)?;
        let Node::Expr(expression) = self.tcx.hir_node_by_def_id(id.expect_local()) else {
            return Err(self.error(
                span,
                "capture source lacks an actual closure HIR expression",
            ));
        };
        let ExprKind::Closure(source) = expression.kind else {
            return Err(self.error(
                span,
                "capture source is not an actual closure HIR expression",
            ));
        };
        if !matches!(source.capture_clause, CaptureBy::Value { .. })
            || source.kind != ClosureKind::Closure
            || source.constness != Constness::NotConst
            || !source.explicit_captures.is_empty()
        {
            return Err(self.error(span,"only ordinary explicit move closures are supported; shared/use/const/async captures are unsupported"));
        }
        let c = args.as_closure();
        if !matches!(c.tupled_upvars_ty().kind(), ty::Tuple(_))
            || c.kind_ty() != self.tcx.types.i8
            || !matches!(c.sig_as_fn_ptr_ty().kind(), ty::FnPtr(..))
        {
            return Err(self.error(span,"captured closure compiler slots require exact Fn kind, signature and upvar tuple metadata"));
        }
        let upvars = c.upvar_tys();
        if upvars.is_empty() || upvars.len() > 256 {
            return Err(self.error(
                span,
                "captured environment requires1..256 whole-root fields",
            ));
        }
        self.callable_charge(span, upvars.len() + 1)?;
        let captures = self.tcx.closure_captures(id.expect_local());
        if captures.len() != upvars.len() {
            return Err(self.error(
                span,
                "capture-place inventory differs from original upvar tuple",
            ));
        }
        let mut fields = Vec::new();
        let mut size = 1usize;
        for (place, field) in captures.iter().zip(upvars) {
            self.callable_charge(span, place.place.projections.len() + 1)?;
            if place.info.capture_kind != UpvarCapture::ByValue
                || !place.place.projections.is_empty()
                || place.place.base_ty != field
            {
                return Err(self.error(span,"only whole-root by-value captures are supported; projected/shared/mutable captures are unsupported"));
            }
            if !matches!(place.place.base,rustc_middle::hir::place::PlaceBase::Upvar(upvar) if upvar.closure_expr_id==id.expect_local())
            {
                return Err(self.error(
                    span,
                    "capture root does not belong to its exact closure owner",
                ));
            }
            let (logical, n) = self.capture_type(field, owner, span)?;
            size = size.saturating_add(n);
            fields.push(logical);
        }
        if Ty::new_closure(self.tcx, id, args)
            .needs_drop(self.tcx, ty::TypingEnv::post_analysis(self.tcx, owner))
        {
            return Err(self.error(
                span,
                "captured calls require immutable drop-free Fn; FnMut/FnOnce are unsupported",
            ));
        }
        for definition in [owner, id] {
            let attrs = self.tcx.codegen_fn_attrs(definition);
            if !attrs.target_features.is_empty() || attrs.flags.contains(CodegenFnAttrFlags::NAKED)
            {
                return Err(self.error(
                    span,
                    "capture target features and naked bodies are unsupported",
                ));
            }
        }
        let signature = c.sig();
        if signature.bound_vars().len() > 64
            || signature
                .bound_vars()
                .iter()
                .any(|v| !matches!(v, ty::BoundVariableKind::Region(_)))
        {
            return Err(self.error(
                span,
                "capture signatures permit only64 bounded lifetime binders",
            ));
        }
        self.callable_charge(span, signature.bound_vars().len() + 1)?;
        let header = signature.skip_binder();
        if header.abi() != rustc_abi::ExternAbi::RustCall
            || header.safety().is_unsafe()
            || header.c_variadic()
            || header.splatted().is_some()
            || header.inputs().len() != 1
        {
            return Err(self.error(span, "unsupported captured RustCall ABI metadata"));
        }
        let signature = self.tcx.instantiate_bound_regions_with_erased(signature);
        let ty::Tuple(inputs) = signature.inputs()[0].kind() else {
            return Err(self.error(span, "capture signature requires an outer argument tuple"));
        };
        if inputs.len() > 63 {
            return Err(self.error(
                span,
                "function exceeds 64 parameters or 2,048 MIR locals (capture receiver included)",
            ));
        }
        let environment = Type::Tuple { items: fields };
        self.callable_charge(span, size + inputs.len() + 1)?;
        let mut parameters = vec![environment.clone()];
        for input in inputs.iter() {
            let (logical, n) = self.lower_type(input, span, 0)?;
            size = size.saturating_add(n);
            if logical.contains_callable() {
                return Err(self.error(
                    span,
                    "captured closure signatures cannot contain callable values",
                ));
            }
            parameters.push(logical);
        }
        let (result, n) = self.lower_type(signature.output(), span, 0)?;
        size = size.saturating_add(n);
        if result.contains_callable() {
            return Err(self.error(span, "captured closures cannot return callable values"));
        }
        self.callable_charge(span, size + upvars.len() + inputs.len() + 1)?;
        let plan = Plan {
            original: Ty::new_closure(self.tcx, id, args),
            args,
            captures: upvars.iter().collect(),
            inputs: inputs.iter().collect(),
            output: signature.output(),
            environment,
            logical: Type::Callable {
                parameters,
                result: Box::new(result),
            },
            size,
        };
        let mut plans = self.captured_closures.borrow_mut();
        if plans.len() >= 256 {
            return Err(self.error(span, "captured closure discovery exceeds256 targets"));
        }
        plans.insert(id, Rc::new(plan));
        Ok(())
    }
    pub(in crate::lower) fn captured_call(
        &self,
        body: &Body<'tcx>,
        func: &Operand<'tcx>,
        arguments: &[Spanned<Operand<'tcx>>],
        destination: Place<'tcx>,
        span: Span,
    ) -> Result<Option<DefId>, String> {
        if self.captured_closures.borrow().is_empty() {
            return Ok(None);
        }
        let function = func.ty(&body.local_decls, self.tcx);
        let ty::FnDef(id, generic) = function.kind() else {
            return Ok(None);
        };
        let Some(trait_id) = self.tcx.lang_items().fn_trait() else {
            return Ok(None);
        };
        if id.is_local()
            || id.krate != trait_id.krate
            || self.tcx.def_kind(*id) != DefKind::AssocFn
            || self.tcx.associated_item(*id).trait_container(self.tcx) != Some(trait_id)
        {
            return Ok(None);
        }
        let generic = generic.skip_binder();
        if generic.len() != 2 || arguments.len() != 2 {
            return Err(self.error(
                span,
                "core Fn call requires exact Self/Args and receiver/tuple operands",
            ));
        }
        let Some(original) = generic[0].as_type() else {
            return Err(self.error(span, "core Fn call lacks concrete closure Self"));
        };
        let ty::Closure(definition, args) = original.kind() else {
            return Err(self.error(
                span,
                "only actual concrete captured closures support direct Fn calls",
            ));
        };
        let Some(plan) = self.captured_plan(*definition) else {
            return Err(self.error(
                span,
                "direct Fn call has no validated whole-root move environment",
            ));
        };
        self.callable_charge(span, plan.size + generic.len() + arguments.len() + 1)?;
        if original != plan.original
            || *args != plan.args
            || generic[1].as_type() != Some(Ty::new_tup(self.tcx, &plan.inputs))
        {
            return Err(self.error(
                span,
                "core Fn original Self/Args differ from captured helper metadata",
            ));
        }
        let inventory = self.tcx.associated_items(trait_id);
        if inventory.len() > 64 {
            return Err(self.error(span, "core Fn associated inventory exceeds64"));
        }
        self.callable_charge(span, inventory.len() + 1)?;
        let mut functions = inventory
            .in_definition_order()
            .filter(|item| matches!(item.kind, ty::AssocKind::Fn { .. }));
        if functions.next().map(|item| item.def_id) != Some(*id) || functions.next().is_some() {
            return Err(self.error(
                span,
                "call must resolve the actual unique core Fn associated function",
            ));
        }
        let signature = self
            .tcx
            .fn_sig(*id)
            .instantiate(self.tcx, generic)
            .skip_norm_wip();
        if signature.bound_vars().len() > 64
            || signature
                .bound_vars()
                .iter()
                .any(|v| !matches!(v, ty::BoundVariableKind::Region(_)))
        {
            return Err(self.error(span, "core Fn call has unsupported binders"));
        }
        let header = signature.skip_binder();
        if header.abi() != rustc_abi::ExternAbi::RustCall
            || header.safety().is_unsafe()
            || header.c_variadic()
            || header.inputs().len() != 2
            || header.splatted().is_some()
        {
            return Err(self.error(span, "core Fn call has unsupported RustCall signature"));
        }
        if !matches!(arguments[0].node.ty(&body.local_decls,self.tcx).kind(),ty::Ref(_,inner,ty::Mutability::Not) if *inner==original)
            || arguments[1].node.ty(&body.local_decls, self.tcx)
                != Ty::new_tup(self.tcx, &plan.inputs)
            || self
                .tcx
                .erase_and_anonymize_regions(destination.ty(&body.local_decls, self.tcx).ty)
                != self.tcx.erase_and_anonymize_regions(plan.output)
        {
            return Err(self.error(
                span,
                "core Fn call operands/result differ from exact closure ABI",
            ));
        }
        match ty::Instance::try_resolve(
            self.tcx,
            ty::TypingEnv::post_analysis(self.tcx, body.source.def_id()),
            *id,
            generic,
        ) {
            Ok(Some(instance))
                if matches!(instance.def,ty::InstanceKind::Item(target) if target==*definition)
                    && instance.args == plan.args =>
            {
                Ok(Some(*definition))
            }
            _ => Err(self.error(
                span,
                "core Fn call did not resolve to the exact local capture helper",
            )),
        }
    }
}
