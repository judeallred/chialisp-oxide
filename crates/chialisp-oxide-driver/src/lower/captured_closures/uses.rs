use super::*;
use rustc_middle::mir::{
    BorrowKind, Location, StatementKind, TerminatorKind,
    visit::{PlaceContext, Visitor},
};
struct Scan<'a, 'tcx> {
    roots: &'a Uses,
    receiver: Option<&'a Plan<'tcx>>,
    work: usize,
    bad: bool,
}
impl<'tcx> Visitor<'tcx> for Scan<'_, 'tcx> {
    fn visit_place(&mut self, place: &Place<'tcx>, context: PlaceContext, _: Location) {
        self.work = self.work.saturating_add(place.projection.len() + 1);
        if let Some(plan) = self.receiver {
            if place.local == Local::from_usize(1) {
                let valid = matches!(&place.projection[..],[ProjectionElem::Deref,ProjectionElem::Field(index,ty),..] if plan.captures.get(index.index()).copied()==Some(*ty));
                self.bad |= !valid || !matches!(context, PlaceContext::NonMutatingUse(_));
            }
        } else {
            self.bad |= self.roots.definitions.contains_key(&place.local)
                || self.roots.references.contains_key(&place.local);
        }
    }
    fn visit_rvalue(&mut self, value: &Rvalue<'tcx>, location: Location) {
        self.work = self.work.saturating_add(match value {
            Rvalue::Aggregate(_, ops) => ops.len() + 1,
            _ => 1,
        });
        if self.work <= super::super::MAX_EXPRESSION_WORK && !self.bad {
            self.super_rvalue(value, location);
        }
    }
    fn visit_terminator(&mut self, value: &mir::Terminator<'tcx>, location: Location) {
        self.work = self.work.saturating_add(match &value.kind {
            TerminatorKind::Call { args, .. } => args.len() + 1,
            _ => 1,
        });
        if self.work <= super::super::MAX_EXPRESSION_WORK && !self.bad {
            self.super_terminator(value, location);
        }
    }
}
fn plain(operand: &Operand<'_>) -> Option<Local> {
    match operand {
        Operand::Copy(p) | Operand::Move(p) if p.projection.is_empty() => Some(p.local),
        _ => None,
    }
}
impl<'tcx> Lowerer<'tcx> {
    pub(super) fn captured_uses(&self, body: &'tcx Body<'tcx>) -> Result<Rc<Uses>, String> {
        let key = body as *const Body<'tcx> as usize;
        if let Some(old) = self.captured_use_plans.borrow().get(&key) {
            return Ok(Rc::clone(old));
        }
        if self.captured_closures.borrow().is_empty() {
            return Ok(Rc::new(Uses::default()));
        }
        if body.local_decls.len() > 2048 {
            return Err(self.error(body.span, "capture use metadata exceeds2048 locals"));
        }
        self.callable_charge(body.span, body.local_decls.len().saturating_mul(4))?;
        let mut uses = Uses::default();
        let mut ref_types = HashMap::new();
        for (local, decl) in body.local_decls.iter_enumerated() {
            match decl.ty.kind() {
                ty::Closure(id, _) if self.captured_plan(*id).is_some() => {
                    uses.definitions.insert(local, *id);
                }
                ty::Ref(_, inner, ty::Mutability::Not) => {
                    if let ty::Closure(id, _) = inner.kind()
                        && self.captured_plan(*id).is_some()
                    {
                        ref_types.insert(local, *id);
                    }
                }
                _ => (),
            }
        }
        let mut assignments = vec![0u8; body.local_decls.len()];
        let mut constructors = HashMap::new();
        for data in body.basic_blocks.iter() {
            self.callable_charge(body.span, data.statements.len() + 1)?;
            for statement in &data.statements {
                let StatementKind::Assign(a) = &statement.kind else {
                    continue;
                };
                let (destination, value) = &**a;
                if let Some(definition) = uses.definitions.get(&destination.local) {
                    assignments[destination.local.index()] =
                        assignments[destination.local.index()].saturating_add(1);
                    if assignments[destination.local.index()] != 1
                        || !destination.projection.is_empty()
                    {
                        return Err(self.error(statement.source_info.span,"captured closure locals require one immutable definition; reassignment and conditional joins are unsupported"));
                    }
                    let plan = self
                        .captured_plan(*definition)
                        .expect("capture definition plan");
                    if destination.ty(&body.local_decls, self.tcx).ty != plan.original {
                        return Err(self.error(
                            statement.source_info.span,
                            "capture local type differs from its paired environment plan",
                        ));
                    }
                    match value {
                        Rvalue::Aggregate(kind,ops) if matches!(&**kind,mir::AggregateKind::Closure(id,args) if id==definition && *args==plan.args)=>{
                            if constructors.insert(*definition,()).is_some(){return Err(self.error(statement.source_info.span,"multiple constructions of one captured closure type are unsupported"));}
                            if ops.len()!=plan.captures.len(){return Err(self.error(statement.source_info.span,"capture aggregate field count mismatch"));}
                            for (op,expected) in ops.iter().zip(&plan.captures){if op.ty(&body.local_decls,self.tcx)!=*expected{return Err(self.error(statement.source_info.span,"capture aggregate original field type mismatch"));}}
                        }
                        Rvalue::Use(op,_) if plain(op).and_then(|l|uses.definitions.get(&l).copied())==Some(*definition) && op.ty(&body.local_decls,self.tcx)==plan.original=>(),
                        _=>return Err(self.error(statement.source_info.span,"capture values may only be constructed or copied as paired code/environment locals")),
                    }
                } else if let Some(definition) = ref_types.get(&destination.local) {
                    assignments[destination.local.index()] =
                        assignments[destination.local.index()].saturating_add(1);
                    if assignments[destination.local.index()] != 1
                        || !destination.projection.is_empty()
                    {
                        return Err(self.error(
                            statement.source_info.span,
                            "capture receiver reference requires one closed compiler definition",
                        ));
                    }
                    let Rvalue::Ref(_, BorrowKind::Shared, source) = value else {
                        return Err(self.error(
                            statement.source_info.span,
                            "capture receivers only permit exact compiler immutable autoref",
                        ));
                    };
                    if !source.projection.is_empty()
                        || uses.definitions.get(&source.local) != Some(definition)
                    {
                        return Err(self.error(
                            statement.source_info.span,
                            "capture receiver reference changes closure/environment identity",
                        ));
                    }
                    uses.references.insert(destination.local, source.local);
                }
            }
        }
        let mut scan = Scan {
            roots: &uses,
            receiver: None,
            work: 0,
            bad: false,
        };
        for (block, data) in body.basic_blocks.iter_enumerated() {
            for (i, statement) in data.statements.iter().enumerate() {
                let location = Location {
                    block,
                    statement_index: i,
                };
                match &statement.kind {
                    StatementKind::StorageLive(_)
                    | StatementKind::StorageDead(_)
                    | StatementKind::Nop => (),
                    StatementKind::Assign(a) if uses.definitions.contains_key(&a.0.local) => {
                        if let Rvalue::Aggregate(_, ops) = &a.1 {
                            for op in ops {
                                scan.visit_operand(op, location);
                            }
                        }
                    }
                    StatementKind::Assign(a) if uses.references.contains_key(&a.0.local) => (),
                    _ => scan.visit_statement(statement, location),
                }
                if scan.bad || scan.work > super::super::MAX_EXPRESSION_WORK {
                    return Err(self.error(statement.source_info.span,"capture values or receiver references escape their closed immutable construction/copy/call uses"));
                }
            }
            let location = Location {
                block,
                statement_index: data.statements.len(),
            };
            if let TerminatorKind::Call {
                func,
                args,
                destination,
                ..
            } = &data.terminator().kind
                && let Some(definition) = self.captured_call(
                    body,
                    func,
                    args,
                    *destination,
                    data.terminator().source_info.span,
                )?
            {
                let receiver = plain(&args[0].node)
                    .and_then(|r| uses.references.get(&r))
                    .and_then(|s| uses.definitions.get(s));
                if receiver != Some(&definition) {
                    return Err(self.error(
                        data.terminator().source_info.span,
                        "capture call receiver/code/environment identities differ",
                    ));
                }
                scan.visit_operand(func, location);
                scan.visit_operand(&args[1].node, location);
                scan.visit_place(
                    destination,
                    PlaceContext::MutatingUse(mir::visit::MutatingUseContext::Call),
                    location,
                );
            } else {
                scan.visit_terminator(data.terminator(), location);
            }
            if scan.bad || scan.work > super::super::MAX_EXPRESSION_WORK {
                return Err(self.error(
                    body.span,
                    "capture value/receiver escapes through a call or terminator",
                ));
            }
        }
        self.callable_charge(
            body.span,
            scan.work + uses.definitions.len() + uses.references.len(),
        )?;
        let uses = Rc::new(uses);
        let mut cache = self.captured_use_plans.borrow_mut();
        if cache.len() >= 256 {
            return Err(self.error(body.span, "capture body use plans exceed256"));
        }
        cache.insert(key, Rc::clone(&uses));
        Ok(uses)
    }
    pub(super) fn captured_receiver_uses(
        &self,
        body: &Body<'tcx>,
        plan: &Plan<'tcx>,
    ) -> Result<(), String> {
        let empty = Uses::default();
        let mut scan = Scan {
            roots: &empty,
            receiver: Some(plan),
            work: 0,
            bad: false,
        };
        for (block, data) in body.basic_blocks.iter_enumerated() {
            self.callable_charge(body.span, data.statements.len() + 1)?;
            for (i, statement) in data.statements.iter().enumerate() {
                scan.visit_statement(
                    statement,
                    Location {
                        block,
                        statement_index: i,
                    },
                );
            }
            scan.visit_terminator(
                data.terminator(),
                Location {
                    block,
                    statement_index: data.statements.len(),
                },
            );
            if scan.bad || scan.work > super::super::MAX_EXPRESSION_WORK {
                return Err(self.error(
                    body.span,
                    "capture helper receiver permits only original immutable field reads",
                ));
            }
        }
        self.callable_charge(body.span, scan.work)
    }
}
