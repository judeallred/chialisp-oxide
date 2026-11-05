//! Closed literal indexing of by-value fixed arrays. Numeric usize metadata is
//! compiler-owned and never becomes a logical Rust usize value or a new ABI.
use super::Lowerer;
use rustc_middle::{
    mir::{
        self, BasicBlock, BinOp, Body, Local, Location, Operand, Place, PlaceRef, Rvalue,
        StatementKind, TerminatorKind,
        visit::{NonMutatingUseContext, PlaceContext, Visitor},
    },
    ty::{self, Ty},
};
use rustc_span::Span;
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

#[derive(Default)]
pub(super) struct Plan {
    pub indices: HashMap<Local, usize>,
    pub has_initializers: bool,
    sources: Vec<Source>,
    pub skip: HashSet<Location>,
    pub targets: HashMap<BasicBlock, BasicBlock>,
}
enum Source {
    Constant,
    Place(Local, bool),
    Unknown,
}
impl Source {
    fn from_operand(operand: &Operand<'_>) -> Self {
        match operand {
            Operand::Constant(_) => Self::Constant,
            Operand::Copy(p) | Operand::Move(p) => Self::Place(p.local, !p.projection.is_empty()),
            _ => Self::Unknown,
        }
    }
    fn computed(&self, total: &[bool]) -> bool {
        match self {
            Self::Constant => false,
            Self::Place(local, false) => !total[local.index()],
            _ => true,
        }
    }
}
impl Plan {
    pub(super) fn computed_initializers(&self, total: &[bool]) -> bool {
        self.sources.iter().any(|source| source.computed(total))
    }
}
pub(super) fn computed_operand(operand: &Operand<'_>, total: &[bool]) -> bool {
    Source::from_operand(operand).computed(total)
}
struct IndexUse<'tcx> {
    local: Local,
    element: Ty<'tcx>,
    length: u64,
}
struct Scan<'a, 'tcx> {
    lower: &'a Lowerer<'tcx>,
    body: &'tcx Body<'tcx>,
    work: usize,
    limit: usize,
    counts: Vec<usize>,
    indices: Vec<IndexUse<'tcx>>,
    error: Option<String>,
    has_initializers: bool,
    sources: Vec<Source>,
}
impl<'tcx> Visitor<'tcx> for Scan<'_, 'tcx> {
    fn visit_operand(&mut self, operand: &Operand<'tcx>, location: Location) {
        self.work = self.work.saturating_add(1);
        if self.work <= self.limit && self.error.is_none() {
            self.super_operand(operand, location);
        }
    }
    fn visit_local(&mut self, local: Local, context: PlaceContext, _location: Location) {
        self.work = self.work.saturating_add(1);
        if !matches!(context, PlaceContext::NonUse(_)) && self.work <= self.limit {
            self.counts[local.index()] = self.counts[local.index()].saturating_add(1);
        }
    }
    fn visit_place(&mut self, place: &Place<'tcx>, context: PlaceContext, location: Location) {
        self.work = self.work.saturating_add(place.projection.len() + 1);
        if self.work > self.limit || self.error.is_some() {
            return;
        }
        for (position, projection) in place.projection.iter().enumerate() {
            if let mir::ProjectionElem::Index(index) = projection {
                if !matches!(
                    context,
                    PlaceContext::NonMutatingUse(
                        NonMutatingUseContext::Copy | NonMutatingUseContext::Move
                    )
                ) {
                    self.error=Some(self.lower.error(self.body.source_info(location).span,"fixed-array indexing supports by-value reads only; indexed mutation and references are unsupported"));
                    return;
                }
                let original = PlaceRef {
                    local: place.local,
                    projection: &place.projection[..position],
                }
                .ty(&self.body.local_decls, self.lower.tcx)
                .ty;
                let ty::Array(element, length) = original.kind() else {
                    self.error = Some(self.lower.error(
                        self.body.source_info(location).span,
                        "unsupported indexing: static indexing requires an original fixed-array type",
                    ));
                    return;
                };
                let Some(length) = length.try_to_target_usize(self.lower.tcx) else {
                    self.error = Some(self.lower.error(
                        self.body.span,
                        "static array indexing requires a known length",
                    ));
                    return;
                };
                if length > 256 || self.indices.len() >= 4096 {
                    self.error=Some(self.lower.error(self.body.span,"ordinary static indexing requires a fixed-value array of at most 256 elements; byte arrays retain their atom ABI"));
                    return;
                }
                self.indices.push(IndexUse {
                    local: index,
                    element: *element,
                    length,
                });
            }
        }
        self.super_place(place, context, location);
    }
    fn visit_statement(&mut self, statement: &mir::Statement<'tcx>, location: Location) {
        self.work = self.work.saturating_add(1);
        if self.work <= self.limit && self.error.is_none() {
            self.super_statement(statement, location);
        }
    }
    fn visit_terminator(&mut self, terminator: &mir::Terminator<'tcx>, location: Location) {
        self.work = self.work.saturating_add(1);
        if self.work <= self.limit && self.error.is_none() {
            self.super_terminator(terminator, location);
        }
    }
    fn visit_rvalue(&mut self, value: &Rvalue<'tcx>, location: Location) {
        self.work = self.work.saturating_add(match value {
            Rvalue::Aggregate(_, values) => values.len() + 1,
            _ => 1,
        });
        if self.work > self.limit || self.error.is_some() {
            return;
        }
        match value {
            Rvalue::Aggregate(kind, values) if matches!(&**kind,mir::AggregateKind::Array(element) if *element!=self.lower.tcx.types.u8) =>
            {
                self.has_initializers |= !values.is_empty();
                self.sources.extend(values.iter().map(Source::from_operand));
            }
            Rvalue::Repeat(operand, _)
                if operand.ty(&self.body.local_decls, self.lower.tcx)
                    != self.lower.tcx.types.u8 =>
            {
                self.has_initializers = true;
                self.sources.push(Source::from_operand(operand));
            }
            _ => (),
        }
        if self.work <= self.limit && self.error.is_none() {
            self.super_rvalue(value, location);
        }
    }
    fn visit_basic_block_data(&mut self, block: BasicBlock, data: &mir::BasicBlockData<'tcx>) {
        self.work = self.work.saturating_add(1);
        if self.work <= self.limit && self.error.is_none() {
            self.super_basic_block_data(block, data);
        }
    }
}
fn local<'tcx>(operand: &Operand<'tcx>) -> Option<Local> {
    match operand {
        Operand::Copy(p) | Operand::Move(p) if p.projection.is_empty() => Some(p.local),
        _ => None,
    }
}
impl<'tcx> Lowerer<'tcx> {
    fn array_literal(&self, body: &Body<'tcx>, operand: &Operand<'tcx>) -> Option<u64> {
        let Operand::Constant(value) = operand else {
            return None;
        };
        if value.const_.ty() != self.tcx.types.usize
            || operand.ty(&body.local_decls, self.tcx) != self.tcx.types.usize
        {
            return None;
        }
        // Both MIR values and type-level value leaves are already evaluated.
        // Unevaluated constants, expressions and provenance never enter this
        // metadata route; do not trigger CTFE merely to recognize an index.
        let scalar = value.const_.try_to_scalar()?.try_to_scalar_int().ok()?;
        if scalar.size() != self.tcx.data_layout.pointer_size() {
            return None;
        }
        Some(scalar.to_target_usize(self.tcx))
    }
    /// New Index admission is narrower than existing array construction. Walk
    /// ORIGINAL element types, including empty nested arrays, before erasure.
    fn static_array_element(&self, element: Ty<'tcx>, span: Span) -> Result<(), String> {
        let mut pending = vec![(element, 0usize)];
        let mut nodes = 0usize;
        while let Some((current, depth)) = pending.pop() {
            nodes += 1;
            let work = self.expression_work.get().saturating_add(1);
            self.expression_work.set(work);
            if nodes > 4096 || depth >= 32 || work > super::MAX_EXPRESSION_WORK {
                return Err(self.error(span, "static array element type/work limit exceeded"));
            }
            let mut children = Vec::new();
            match current.kind(){
                ty::Bool=>(),ty::Int(k) if k.bit_width().is_some()=>(),ty::Uint(k) if k.bit_width().is_some()=>(),
                ty::Tuple(fields)=>{
                    if fields.len()>256||fields.len()>4096usize.saturating_sub(nodes+pending.len()){return Err(self.error(span,"static array element expansion limit exceeded"));}
                    children.extend(fields.iter());
                }
                ty::Array(inner,len)=>{
                    let length=len.try_to_target_usize(self.tcx).filter(|n|*n<=256).ok_or_else(||self.error(span,"static array element arrays require bounded lengths"))? as usize;
                    let expanded=length.max(1);
                    if expanded>4096usize.saturating_sub(nodes+pending.len()){return Err(self.error(span,"static array element expansion limit exceeded"));}
                    children.extend(std::iter::repeat_n(*inner,expanded));
                }
                ty::Adt(..)=>{
                    if self.optional_shape(current,span)?.is_some()||self.cons_fields(current,span)?.is_some()||self.tree_shape(current,span)?.is_some(){return Err(self.error(span,"static indexing does not admit Option, Cons or tree element types"));}
                    if let Some(fields)=self.named_struct_fields(current,span)?{
                        if fields.len()>4096usize.saturating_sub(nodes+pending.len()){return Err(self.error(span,"static array element expansion limit exceeded"));}
                        children=fields;
                    }else{children.push(self.transparent_inner(current,span)?.ok_or_else(||self.error(span,"unsupported static array element container"))?);}
                }
                _=>return Err(self.error(span,"static indexing requires drop-free fixed values; pointers, references and general containers are unsupported")),
            }
            pending.extend(children.into_iter().map(|t| (t, depth + 1)));
        }
        if element.needs_drop(self.tcx, ty::TypingEnv::fully_monomorphized()) {
            return Err(self.error(span, "static array elements must be drop-free"));
        }
        Ok(())
    }
    /// Compute only after discovering an array-producing source graph. Pure
    /// argument/constant copy chains keep legacy artifacts; computed arguments,
    /// reassigned scratch slots and calls require immutable runtime captures.
    pub(super) fn array_total_origins(&self, body: &Body<'tcx>) -> Result<Vec<bool>, String> {
        let count = body.local_decls.len();
        let statements = body
            .basic_blocks
            .iter()
            .try_fold(0usize, |n, b| n.checked_add(b.statements.len()))
            .ok_or_else(|| self.error(body.span, "array source statement overflow"))?;
        let work = self
            .expression_work
            .get()
            .saturating_add(count.saturating_mul(8))
            .saturating_add(statements.saturating_mul(2))
            .saturating_add(body.basic_blocks.len());
        if count > 2048 || work > super::MAX_EXPRESSION_WORK {
            return Err(self.error(body.span, "array source origin work limit exceeded"));
        }
        self.expression_work.set(work);
        let mut writes = vec![0usize; count];
        let mut aliases = vec![None; count];
        let mut constants = vec![false; count];
        for data in body.basic_blocks.iter() {
            for statement in &data.statements {
                if let StatementKind::Assign(assignment) = &statement.kind {
                    let (place, value) = &**assignment;
                    let index = place.local.index();
                    writes[index] += 1;
                    if place.projection.is_empty()
                        && let Rvalue::Use(operand, _) = value
                    {
                        match operand {
                            Operand::Constant(_) => constants[index] = true,
                            Operand::Copy(source) | Operand::Move(source)
                                if source.projection.is_empty() =>
                            {
                                aliases[index] = Some(source.local.index())
                            }
                            _ => (),
                        }
                    }
                }
            }
            if let TerminatorKind::Call { destination, .. } = &data.terminator().kind {
                writes[destination.local.index()] += 1;
            }
        }
        let mut total = vec![false; count];
        let mut dependents = vec![Vec::new(); count];
        let mut ready = Vec::new();
        for index in 0..count {
            if (index > 0 && index <= body.arg_count && writes[index] == 0)
                || (writes[index] == 1 && constants[index])
            {
                total[index] = true;
                ready.push(index);
            }
            if writes[index] == 1
                && let Some(source) = aliases[index]
            {
                dependents[source].push(index);
            }
        }
        while let Some(source) = ready.pop() {
            for &target in &dependents[source] {
                if !total[target] {
                    total[target] = true;
                    ready.push(target);
                }
            }
        }
        Ok(total)
    }
    pub(super) fn array_plan(&self, body: &'tcx Body<'tcx>) -> Result<Rc<Plan>, String> {
        // Identity belongs to this immutable Tcxt arena only; never persist it.
        let work = self.expression_work.get().saturating_add(1);
        self.expression_work.set(work);
        if work > super::MAX_EXPRESSION_WORK {
            return Err(self.error(body.span, "array metadata lookup work limit exceeded"));
        }
        let key = body as *const Body<'tcx> as usize;
        if let Some(plan) = self.array_plans.borrow().get(&key) {
            return Ok(Rc::clone(plan));
        }
        if self.array_plans.borrow().len() >= 10000 {
            return Err(self.error(body.span, "array metadata body limit exceeded"));
        }
        let plan = Rc::new(self.compute_array_plan(body)?);
        let work = self.expression_work.get().saturating_add(1);
        self.expression_work.set(work);
        if work > super::MAX_EXPRESSION_WORK {
            return Err(self.error(body.span, "array metadata insertion work limit exceeded"));
        }
        self.array_plans.borrow_mut().insert(key, Rc::clone(&plan));
        Ok(plan)
    }
    fn compute_array_plan(&self, body: &'tcx Body<'tcx>) -> Result<Plan, String> {
        // Existing MIR/body limits also bound metadata storage before allocation.
        if body.local_decls.len() > 2048 {
            return Err(self.error(body.span, "array metadata local limit exceeded"));
        }
        let headers = body
            .local_decls
            .len()
            .saturating_add(body.basic_blocks.len());
        let remaining = super::MAX_EXPRESSION_WORK.saturating_sub(self.expression_work.get());
        if headers > remaining {
            return Err(self.error(body.span, "array metadata preflight work limit exceeded"));
        }
        self.expression_work
            .set(self.expression_work.get() + headers);
        let remaining = super::MAX_EXPRESSION_WORK.saturating_sub(self.expression_work.get());
        let mut scan = Scan {
            lower: self,
            body,
            work: 0,
            limit: remaining,
            counts: vec![0; body.local_decls.len()],
            indices: vec![],
            error: None,
            has_initializers: false,
            sources: vec![],
        };
        for (block, data) in body.basic_blocks.iter_enumerated() {
            scan.visit_basic_block_data(block, data);
        }
        self.expression_work
            .set(self.expression_work.get().saturating_add(scan.work));
        if scan.work > remaining {
            return Err(self.error(body.span, "array metadata scan work limit exceeded"));
        }
        if let Some(error) = scan.error {
            return Err(error);
        }
        if scan.indices.is_empty() {
            return Ok(Plan {
                has_initializers: scan.has_initializers,
                sources: scan.sources,
                ..Plan::default()
            });
        }
        // Only actual new Index metadata needs the closed 64-block profile.
        if body.basic_blocks.len() > 64 {
            return Err(self.error(
                body.span,
                "static array indexing is limited to 64 MIR blocks",
            ));
        }
        let work = self
            .expression_work
            .get()
            .saturating_add(body.local_decls.len().saturating_mul(2));
        if work > super::MAX_EXPRESSION_WORK {
            return Err(self.error(body.span, "array metadata definition work limit exceeded"));
        }
        self.expression_work.set(work);
        let mut definitions = vec![None; body.local_decls.len()];
        let mut writes = vec![0usize; body.local_decls.len()];
        let statements = body
            .basic_blocks
            .iter()
            .try_fold(0usize, |n, b| n.checked_add(b.statements.len()))
            .ok_or_else(|| self.error(body.span, "array metadata statement overflow"))?;
        let work = self.expression_work.get().saturating_add(statements);
        if work > super::MAX_EXPRESSION_WORK {
            return Err(self.error(body.span, "array metadata statement work limit exceeded"));
        }
        self.expression_work.set(work);
        for (block, data) in body.basic_blocks.iter_enumerated() {
            for (statement_index, statement) in data.statements.iter().enumerate() {
                if let StatementKind::Assign(assignment) = &statement.kind {
                    let (place, value) = &**assignment;
                    writes[place.local.index()] += 1;
                    if place.projection.is_empty() {
                        definitions[place.local.index()] = Some((
                            Location {
                                block,
                                statement_index,
                            },
                            value,
                        ));
                    }
                }
            }
        }
        let mut plan = Plan {
            has_initializers: scan.has_initializers,
            sources: scan.sources,
            ..Plan::default()
        };
        let mut occurrences = HashMap::<Local, usize>::new();
        let mut lengths = HashMap::<Local, u64>::new();
        for index in &scan.indices {
            self.static_array_element(index.element, body.span)?;
            if body.local_decls[index.local].ty != self.tcx.types.usize
                || writes[index.local.index()] != 1
            {
                return Err(self.error(
                    body.span,
                    "static array index requires one immutable compiler-owned usize literal",
                ));
            }
            let Some((location, Rvalue::Use(value, _))) = definitions[index.local.index()] else {
                return Err(self.error(body.span, "dynamic array indices are unsupported"));
            };
            let value = self
                .array_literal(body, value)
                .filter(|n| *n < index.length)
                .ok_or_else(|| {
                    self.error(
                        body.span,
                        "dynamic or out-of-bounds array indices are unsupported",
                    )
                })?;
            if lengths
                .insert(index.local, index.length)
                .is_some_and(|old| old != index.length)
            {
                return Err(self.error(
                    body.span,
                    "static array index is reused for different array lengths",
                ));
            }
            plan.indices.insert(index.local, value as usize);
            plan.skip.insert(location);
            *occurrences.entry(index.local).or_default() += 1;
        }
        let mut guarded = HashSet::new();
        for (block, data) in body.basic_blocks.iter_enumerated() {
            let TerminatorKind::Assert {
                cond,
                expected: true,
                msg,
                target,
                ..
            } = &data.terminator().kind
            else {
                continue;
            };
            let mir::AssertMessage::BoundsCheck { len, index } = &**msg else {
                continue;
            };
            let Some(index) = local(index) else {
                continue;
            };
            let Some(length) = lengths.get(&index) else {
                continue;
            };
            let condition = local(cond).ok_or_else(|| {
                self.error(
                    body.span,
                    "static array bounds guard requires direct Boolean metadata",
                )
            })?;
            let Some((location, Rvalue::BinaryOp(BinOp::Lt, values))) =
                definitions[condition.index()]
            else {
                return Err(self.error(
                    body.span,
                    "static array bounds comparison metadata mismatch",
                ));
            };
            if body.local_decls[condition].ty != self.tcx.types.bool
                || writes[condition.index()] != 1
                || scan.counts[condition.index()] != 2
                || local(&values.0) != Some(index)
                || self.array_literal(body, &values.1) != Some(*length)
                || self.array_literal(body, len) != Some(*length)
                || !guarded.insert(index)
            {
                return Err(self.error(
                    body.span,
                    "static array bounds guard metadata mismatch or extra use",
                ));
            }
            plan.skip.insert(location);
            plan.targets.insert(block, *target);
        }
        for (index, count) in occurrences {
            if !guarded.contains(&index) || scan.counts[index.index()] != count + 3 {
                return Err(self.error(
                    body.span,
                    "static array index metadata escapes its exact guard/projection uses",
                ));
            }
        }
        Ok(plan)
    }
}
