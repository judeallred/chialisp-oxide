//! Closed local move captures, elaborated to existing v9 code/env bindings.
//! No machine reference, callable aggregate, public closure ABI or new wire node.
use super::Lowerer;
use chialisp_oxide_ir::{Expr, Parameter, Type};
use rustc_hir::{def::DefKind, def_id::DefId};
use rustc_middle::{
    mir::{self, Body, Local, Operand, Place, ProjectionElem, Rvalue},
    ty::{self, Ty},
};
use rustc_span::Span;
use std::{collections::HashMap, rc::Rc};
mod metadata;
mod uses;

pub(super) struct Plan<'tcx> {
    pub original: Ty<'tcx>,
    pub args: ty::GenericArgsRef<'tcx>,
    pub captures: Vec<Ty<'tcx>>,
    pub inputs: Vec<Ty<'tcx>>,
    pub output: Ty<'tcx>,
    pub environment: Type,
    pub logical: Type,
    pub size: usize,
}
#[derive(Clone)]
pub(super) struct Value {
    pub definition: DefId,
    pub environment: Expr,
    pub code: Expr,
}
#[derive(Clone, Copy)]
pub(super) enum Assignment {
    Construct(DefId),
    Copy(Local),
    Reference(Local),
}
#[derive(Default)]
pub(super) struct Uses {
    pub definitions: HashMap<Local, DefId>,
    pub references: HashMap<Local, Local>,
}
impl<'tcx> Lowerer<'tcx> {
    pub(super) fn captured_plan(&self, id: DefId) -> Option<Rc<Plan<'tcx>>> {
        self.captured_closures.borrow().get(&id).cloned()
    }
    pub(super) fn captured_references(
        &self,
        body: &'tcx Body<'tcx>,
    ) -> Result<Vec<(DefId, Type, usize)>, String> {
        // A cheap metadata peek does not charge old zero-env/non-closure paths.
        let candidate = body.local_decls.iter().any(|d| {
            let ty::Closure(_, args) = d.ty.kind() else {return false};
            !matches!(args.last().and_then(|a|a.as_type()).map(|t|t.kind()),Some(ty::Tuple(items)) if items.is_empty())
        });
        if !candidate {
            return Ok(vec![]);
        }
        if self.tcx.def_kind(body.source.def_id()) == DefKind::Closure {
            return Err(self.error(body.span, "nested captured closures are unsupported"));
        }
        let mut result = Vec::new();
        for data in body.basic_blocks.iter() {
            self.callable_charge(body.span, data.statements.len() + 1)?;
            for statement in &data.statements {
                if let mir::StatementKind::Assign(a) = &statement.kind
                    && let Rvalue::Aggregate(kind, operands) = &a.1
                    && let mir::AggregateKind::Closure(id, args) = &**kind
                    && !operands.is_empty()
                {
                    self.admit_captured_closure(*id, args, statement.source_info.span)?;
                    let plan = self.captured_plan(*id).expect("admitted captured plan");
                    if operands.len() != plan.captures.len()
                        || !a.0.projection.is_empty()
                        || a.0.ty(&body.local_decls, self.tcx).ty != plan.original
                    {
                        return Err(self.error(
                            statement.source_info.span,
                            "captured closure aggregate differs from exact metadata",
                        ));
                    }
                    for (operand, expected) in operands.iter().zip(&plan.captures) {
                        if operand.ty(&body.local_decls, self.tcx) != *expected {
                            return Err(self.error(
                                statement.source_info.span,
                                "capture operand differs from original upvar type",
                            ));
                        }
                    }
                    if !result.iter().any(|(definition, _, _)| definition == id) {
                        self.callable_charge(statement.source_info.span, plan.size + 1)?;
                        result.push((*id, plan.logical.clone(), plan.size));
                    }
                }
            }
        }
        if !result.is_empty() {
            self.captured_uses(body)?;
        }
        Ok(result)
    }
    pub(super) fn captured_body(&self, id: DefId) -> Result<Option<&'tcx Body<'tcx>>, String> {
        let Some(plan) = self.captured_plan(id) else {
            return Ok(None);
        };
        let span = self.tcx.def_span(id);
        if !id.is_local()
            || self.tcx.def_kind(id) != DefKind::Closure
            || !self.tcx.is_mir_available(id)
        {
            return Err(self.error(
                span,
                "captured helper must be an available local compiler closure",
            ));
        }
        let body = self.tcx.optimized_mir(id);
        self.owned_plan(body)?;
        if body.arg_count != plan.inputs.len() + 1
            || body.arg_count > 64
            || body.local_decls.len() > 2048
            || body.spread_arg.is_some()
        {
            return Err(self.error(
                span,
                "function exceeds 64 parameters or 2,048 MIR locals (capture receiver included)",
            ));
        }
        if !matches!(body.local_decls[Local::from_usize(1)].ty.kind(),ty::Ref(_,inner,ty::Mutability::Not) if *inner==plan.original)
        {
            return Err(self.error(
                span,
                "capture receiver is not an immutable reference to its exact closure type",
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
                    "captured helper flat arguments differ from original RustCall signature",
                ));
            }
        }
        if self.tcx.erase_and_anonymize_regions(body.return_ty())
            != self.tcx.erase_and_anonymize_regions(plan.output)
        {
            return Err(self.error(
                span,
                "captured helper result differs from original RustCall signature",
            ));
        }
        self.captured_receiver_uses(body, &plan)?;
        Ok(Some(body))
    }
    pub(super) fn captured_signature(
        &self,
        body: &Body<'tcx>,
    ) -> Result<Option<(Vec<Parameter>, Type)>, String> {
        let Some(plan) = self.captured_plan(body.source.def_id()) else {
            return Ok(None);
        };
        self.callable_charge(body.span, plan.size)?;
        let Type::Callable { parameters, result } = &plan.logical else {
            unreachable!()
        };
        Ok(Some((
            parameters
                .iter()
                .enumerate()
                .map(|(i, ty)| Parameter {
                    name: format!("arg{i}"),
                    ty: ty.clone(),
                })
                .collect(),
            *result.clone(),
        )))
    }
    pub(super) fn captured_assignment(
        &self,
        body: &'tcx Body<'tcx>,
        place: Place<'tcx>,
        value: &Rvalue<'tcx>,
        span: Span,
    ) -> Result<Option<Assignment>, String> {
        if self.captured_closures.borrow().is_empty() {
            return Ok(None);
        }
        let uses = self.captured_uses(body)?;
        if let Some(id) = uses.definitions.get(&place.local) {
            if !place.projection.is_empty() {
                return Err(
                    self.error(span, "captured closure storage projections are unsupported")
                );
            }
            return match value {
                Rvalue::Aggregate(kind,_) if matches!(&**kind,mir::AggregateKind::Closure(definition,_) if definition==id)=>Ok(Some(Assignment::Construct(*id))),
                Rvalue::Use(Operand::Copy(p)|Operand::Move(p),_) if p.projection.is_empty()=>Ok(Some(Assignment::Copy(p.local))),
                _=>Err(self.error(span,"only captured closure construction and paired immutable local copies are supported")),
            };
        }
        if let Some(source) = uses.references.get(&place.local) {
            return Ok(Some(Assignment::Reference(*source)));
        }
        Ok(None)
    }
    pub(super) fn captured_projection(
        &self,
        body: &Body<'tcx>,
        place: Place<'tcx>,
        locals: &super::Environment,
        span: Span,
    ) -> Result<Option<(Expr, Ty<'tcx>, usize)>, String> {
        let Some(plan) = self.captured_plan(body.source.def_id()) else {
            return Ok(None);
        };
        if place.local != Local::from_usize(1) {
            return Ok(None);
        }
        let [
            ProjectionElem::Deref,
            ProjectionElem::Field(index, field),
            ..,
        ] = &place.projection[..]
        else {
            return Err(self.error(
                span,
                "captured receiver may only expose immutable exact capture fields",
            ));
        };
        if plan.captures.get(index.index()).copied() != Some(*field) {
            return Err(self.error(
                span,
                "captured receiver field differs from original upvar metadata",
            ));
        }
        let base = locals[place.local.index()]
            .clone()
            .ok_or_else(|| self.error(span, "capture receiver environment is uninitialized"))?;
        Ok(Some((
            Expr::Field {
                value: Box::new(base),
                index: index.index(),
            },
            *field,
            2,
        )))
    }
    pub(super) fn charge_captured_operand(
        &self,
        body: &Body<'tcx>,
        operand: &Operand<'tcx>,
        locals: &super::Environment,
        span: Span,
    ) -> Result<(), String> {
        if let Operand::Copy(place) | Operand::Move(place) = operand {
            if !place.projection.is_empty() {
                return Err(self.error(span, "capture operands require whole original root places"));
            }
            if let Some(value) = &locals[place.local.index()] {
                let payload = match value {
                    Expr::Bytes { hex } | Expr::Atom { hex } => hex.len(),
                    Expr::Int { value, .. } => value.len(),
                    _ => 0,
                };
                self.callable_charge(span, payload)?;
                self.check_expression(value, span)?;
            }
        } else if operand.ty(&body.local_decls, self.tcx).is_fn() {
            return Err(self.error(span, "captured function values are unsupported"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
