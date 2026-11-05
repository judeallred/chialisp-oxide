//! The compiler's empty-slice pattern metadata is not a supported Rust value.
//!
//! Recognize its entire closed use chain before lowering any statement. Raw
//! pointers and usize lengths stay in a separate internal environment; only
//! an Atom capture and a final boolean empty check enter the typed IR.
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use chialisp_oxide_ir::Expr;
use rustc_middle::{
    mir::{
        BinOp, Body, Local, Location, Operand, Place, ProjectionElem, RawPtrKind, Rvalue,
        StatementKind, UnOp,
        visit::{MutatingUseContext, NonMutatingUseContext, PlaceContext, Visitor},
    },
    ty,
};
use rustc_span::Span;

use super::Lowerer;

const MAX_PATTERN_STATEMENTS: usize = 8_192;
const MAX_PATTERN_WORK: usize = 500_000;

#[derive(Clone, Copy)]
pub(super) enum Action<'tcx> {
    Capture {
        destination: Local,
        source: Place<'tcx>,
    },
    Alias {
        capture: Local,
    },
    Zero,
    Check {
        capture: Local,
        negated: bool,
    },
}

#[derive(Default)]
pub(super) struct Patterns<'tcx> {
    actions: HashMap<Location, Action<'tcx>>,
}

#[derive(Clone)]
pub(super) struct State<'a, 'tcx> {
    pub patterns: &'a Patterns<'tcx>,
    pub values: HashMap<Local, Rc<Expr>>,
}

impl<'a, 'tcx> State<'a, 'tcx> {
    pub fn new(patterns: &'a Patterns<'tcx>) -> Self {
        Self {
            patterns,
            values: HashMap::new(),
        }
    }
}

impl<'tcx> Patterns<'tcx> {
    pub fn action(&self, location: Location) -> Option<Action<'tcx>> {
        self.actions.get(&location).copied()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Pointer(Local),
    Length(Local),
    Zero,
}

#[derive(Default)]
struct Uses {
    values: Vec<(Local, PlaceContext, Location)>,
    work: usize,
}
impl Uses {
    fn charge(&mut self, amount: usize) -> bool {
        self.work = self.work.saturating_add(amount);
        self.work <= MAX_PATTERN_WORK
    }
}
impl<'tcx> Visitor<'tcx> for Uses {
    fn visit_local(&mut self, local: Local, context: PlaceContext, location: Location) {
        if self.charge(1) {
            self.values.push((local, context, location));
        }
    }
    fn visit_place(&mut self, place: &Place<'tcx>, context: PlaceContext, location: Location) {
        if self.charge(place.projection.len() + 1) {
            self.super_place(place, context, location);
        }
    }
    fn visit_rvalue(&mut self, value: &Rvalue<'tcx>, location: Location) {
        let amount = if let Rvalue::Aggregate(_, operands) = value {
            operands.len().saturating_add(1)
        } else {
            1
        };
        if self.charge(amount) {
            self.super_rvalue(value, location);
        }
    }
    fn visit_terminator(
        &mut self,
        value: &rustc_middle::mir::Terminator<'tcx>,
        location: Location,
    ) {
        let amount = if let rustc_middle::mir::TerminatorKind::Call { args, .. } = &value.kind {
            args.len().saturating_add(1)
        } else {
            1
        };
        if self.charge(amount) {
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
    pub(super) fn empty_atom_patterns(&self, body: &Body<'tcx>) -> Result<Patterns<'tcx>, String> {
        let mut statements = Vec::new();
        let mut has_metadata = false;
        let mut statement_count = 0usize;
        // Bodies without metadata retain their existing lowering route. Bound
        // the initial scan before allocating the exceptional analysis state.
        for data in body.basic_blocks.iter() {
            statement_count = statement_count.saturating_add(data.statements.len());
            if statement_count > MAX_PATTERN_STATEMENTS || body.basic_blocks.len() > 10_000 {
                return Err(self.error(
                    body.span,
                    "MIR analysis exceeds 8,192 statements or 10,000 blocks",
                ));
            }
            has_metadata |= data.statements.iter().any(|statement| {
                matches!(&statement.kind, StatementKind::Assign(assignment)
                    if matches!(assignment.1, Rvalue::RawPtr(RawPtrKind::FakeForPtrMetadata, _)
                        | Rvalue::UnaryOp(UnOp::PtrMetadata, _)))
            });
        }
        if !has_metadata {
            return Ok(Patterns::default());
        }
        if statement_count > MAX_PATTERN_STATEMENTS
            || body.basic_blocks.len() > 10_000
            || body.local_decls.len() > 2_048
        {
            return Err(self.error(
                body.span,
                "empty-slice pattern analysis exceeds its statement/block/local budget",
            ));
        }
        for (block, data) in body.basic_blocks.iter_enumerated() {
            for (statement_index, statement) in data.statements.iter().enumerate() {
                if let StatementKind::Assign(assignment) = &statement.kind {
                    statements.push((
                        Location {
                            block,
                            statement_index,
                        },
                        &**assignment,
                        statement.source_info.span,
                    ));
                }
            }
        }
        let mut uses = Uses::default();
        uses.visit_body(body);
        if uses.work > MAX_PATTERN_WORK {
            return Err(self.error(
                body.span,
                "empty-slice pattern analysis exceeds its use budget",
            ));
        }
        let mut writes = HashMap::<Local, usize>::new();
        for (local, context, _) in &uses.values {
            if matches!(context, PlaceContext::MutatingUse(_)) {
                *writes.entry(*local).or_default() += 1;
            }
        }
        let mut kinds = HashMap::new();
        let mut actions = HashMap::new();
        let mut definitions = HashMap::new();
        let mut inputs = HashMap::<Location, Vec<Local>>::new();
        let mut origins = HashSet::new();
        let byte_slice = |ty: ty::Ty<'tcx>| matches!(ty.kind(), ty::Slice(element) if *element == self.tcx.types.u8);
        for (location, (destination, value), span) in &statements {
            if let Rvalue::RawPtr(RawPtrKind::FakeForPtrMetadata, source) = value {
                let Some((reference, ProjectionElem::Deref)) = source.as_ref().last_projection()
                else {
                    return Err(self.error(
                        *span,
                        "empty-slice pattern metadata must borrow an immutable byte slice",
                    ));
                };
                let reference_ty = reference.ty(&body.local_decls, self.tcx).ty;
                let pointer_ty = body.local_decls[destination.local].ty;
                if !destination.projection.is_empty()
                    || !matches!(reference_ty.kind(), ty::Ref(_, inner, ty::Mutability::Not) if byte_slice(*inner))
                    || !matches!(pointer_ty.kind(), ty::RawPtr(inner, ty::Mutability::Not) if byte_slice(*inner))
                {
                    return Err(self.error(*span, "empty-slice pattern metadata requires immutable &[u8], not arrays or user pointers"));
                }
                kinds.insert(destination.local, Kind::Pointer(destination.local));
                origins.insert(destination.local);
                definitions.insert(destination.local, *location);
                actions.insert(
                    *location,
                    Action::Capture {
                        destination: destination.local,
                        source: *source,
                    },
                );
            } else if let Rvalue::UnaryOp(
                UnOp::PtrMetadata,
                Operand::Copy(source) | Operand::Move(source),
            ) = value
                && matches!(source.ty(&body.local_decls,self.tcx).ty.kind(),ty::Ref(_,inner,ty::Mutability::Not) if byte_slice(*inner))
            {
                // For an existing slice reference rustc emits metadata directly,
                // without creating the fake raw pointer used by tree patterns.
                if !destination.projection.is_empty()
                    || body.local_decls[destination.local].ty != self.tcx.types.usize
                {
                    return Err(self.error(
                        *span,
                        "empty-slice metadata must have a plain usize temporary",
                    ));
                }
                kinds.insert(destination.local, Kind::Length(destination.local));
                origins.insert(destination.local);
                definitions.insert(destination.local, *location);
                actions.insert(
                    *location,
                    Action::Capture {
                        destination: destination.local,
                        source: *source,
                    },
                );
            }
        }
        // Follow only typed, plain-local aliases. Unique definitions are checked
        // below, independently of source/block ordering and control-flow paths.
        let mut work = 0;
        loop {
            let previous = kinds.len();
            for (location, (destination, value), _) in &statements {
                work += 1;
                if work > MAX_PATTERN_WORK {
                    return Err(self.error(
                        body.span,
                        "empty-slice pattern analysis exceeds its work budget",
                    ));
                }
                if !destination.projection.is_empty() || kinds.contains_key(&destination.local) {
                    continue;
                }
                let (kind, action, source) = match value {
                    Rvalue::UnaryOp(UnOp::PtrMetadata, operand) => {
                        let Some(source) = plain(operand) else {
                            continue;
                        };
                        let Some(Kind::Pointer(origin)) = kinds.get(&source).copied() else {
                            continue;
                        };
                        if body.local_decls[destination.local].ty != self.tcx.types.usize {
                            continue;
                        }
                        (
                            Kind::Length(origin),
                            Action::Alias { capture: origin },
                            source,
                        )
                    }
                    Rvalue::Use(operand, _) => {
                        let Some(source) = plain(operand) else {
                            continue;
                        };
                        let Some(kind) = kinds.get(&source).copied() else {
                            continue;
                        };
                        if body.local_decls[destination.local].ty != body.local_decls[source].ty {
                            continue;
                        }
                        let capture = match kind {
                            Kind::Pointer(origin) | Kind::Length(origin) => origin,
                            Kind::Zero => continue,
                        };
                        (kind, Action::Alias { capture }, source)
                    }
                    _ => continue,
                };
                kinds.insert(destination.local, kind);
                definitions.insert(destination.local, *location);
                actions.insert(*location, action);
                inputs.insert(*location, vec![source]);
            }
            if previous == kinds.len() {
                break;
            }
        }
        let mut consumed = HashSet::new();
        for (location, (destination, value), _) in &statements {
            let Rvalue::BinaryOp(op @ (BinOp::Eq | BinOp::Ne), operands) = value else {
                continue;
            };
            if !destination.projection.is_empty()
                || body.local_decls[destination.local].ty != self.tcx.types.bool
            {
                continue;
            }
            for (length, zero) in [(&operands.0, &operands.1), (&operands.1, &operands.0)] {
                let Some(local) = plain(length) else { continue };
                let Some(Kind::Length(origin)) = kinds.get(&local).copied() else {
                    continue;
                };
                let mut zeros = Vec::new();
                if !self.pattern_zero(
                    body,
                    zero,
                    &statements,
                    &mut zeros,
                    &mut vec![],
                    &mut work,
                )? {
                    continue;
                }
                let mut reads = vec![local];
                if let Some(local) = plain(zero) {
                    reads.push(local);
                }
                for (local, definition, source) in zeros {
                    kinds.insert(local, Kind::Zero);
                    definitions.insert(local, definition);
                    actions.insert(definition, Action::Zero);
                    if let Some(source) = source {
                        inputs.insert(definition, vec![source]);
                    }
                }
                actions.insert(
                    *location,
                    Action::Check {
                        capture: origin,
                        negated: *op == BinOp::Ne,
                    },
                );
                inputs.insert(*location, reads);
                consumed.insert(origin);
                break;
            }
        }
        if consumed != origins {
            return Err(self.error(body.span, "slice pattern metadata is supported only for an exact empty-slice check (Eq/Ne zero)"));
        }
        for (local, context, location) in uses.values {
            if !kinds.contains_key(&local) {
                continue;
            }
            let valid = match context {
                PlaceContext::NonUse(_) => true,
                PlaceContext::NonMutatingUse(NonMutatingUseContext::PlaceMention) => true,
                PlaceContext::MutatingUse(MutatingUseContext::Store) => {
                    definitions.get(&local) == Some(&location) && writes.get(&local) == Some(&1)
                }
                PlaceContext::NonMutatingUse(
                    NonMutatingUseContext::Copy | NonMutatingUseContext::Move,
                ) => inputs
                    .get(&location)
                    .is_some_and(|allowed| allowed.contains(&local)),
                _ => false,
            };
            if !valid {
                return Err(self.error(body.source_info(location).span, "slice pattern metadata escapes its validated empty-slice check; general lengths and pointers are unsupported"));
            }
        }
        Ok(Patterns { actions })
    }

    fn pattern_zero(
        &self,
        body: &Body<'tcx>,
        operand: &Operand<'tcx>,
        statements: &[(Location, &(Place<'tcx>, Rvalue<'tcx>), Span)],
        zeros: &mut Vec<(Local, Location, Option<Local>)>,
        path: &mut Vec<Local>,
        work: &mut usize,
    ) -> Result<bool, String> {
        *work = work.saturating_add(statements.len().max(1));
        if *work > MAX_PATTERN_WORK || path.len() >= 64 {
            return Err(self.error(
                body.span,
                "empty-slice zero-alias analysis exceeds its work/depth budget",
            ));
        }
        if operand.ty(&body.local_decls, self.tcx) != self.tcx.types.usize {
            return Ok(false);
        }
        if let Operand::Constant(value) = operand {
            return Ok(value
                .const_
                .try_eval_bits(self.tcx, ty::TypingEnv::fully_monomorphized())
                == Some(0));
        }
        let Some(local) = plain(operand) else {
            return Ok(false);
        };
        if path.contains(&local) {
            return Ok(false);
        }
        let mut definitions = statements
            .iter()
            .filter(|(_, (place, _), _)| place.local == local);
        let Some((location, (destination, Rvalue::Use(source, _)), _)) = definitions.next() else {
            return Ok(false);
        };
        if !destination.projection.is_empty() || definitions.next().is_some() {
            return Ok(false);
        }
        path.push(local);
        let valid = self.pattern_zero(body, source, statements, zeros, path, work)?;
        path.pop();
        if valid {
            zeros.push((local, *location, plain(source)));
        }
        Ok(valid)
    }

    /// `None` denotes an internal metadata assignment, never an ordinary IR
    /// local with an invented Rust type. Capture expressions are snapshotted at
    /// their original source statement, before any subsequent local mutation.
    pub(super) fn empty_atom_action(
        &self,
        body: &Body<'tcx>,
        action: Action<'tcx>,
        locals: &super::Environment,
        metadata: &mut HashMap<Local, Rc<Expr>>,
        span: Span,
    ) -> Result<Option<Expr>, String> {
        match action {
            Action::Capture {
                destination,
                source,
            } => {
                let value = self.place(body, source, locals, span)?;
                self.check_expression(&value, span)?;
                metadata.insert(destination, Rc::new(value));
                Ok(None)
            }
            Action::Alias { capture } => {
                if !metadata.contains_key(&capture) {
                    return Err(
                        self.error(span, "empty-slice metadata reads an uninitialized capture")
                    );
                }
                Ok(None)
            }
            Action::Zero => Ok(None),
            Action::Check { capture, negated } => {
                let value = metadata
                    .get(&capture)
                    .ok_or_else(|| {
                        self.error(span, "empty-slice check reads an uninitialized capture")
                    })?
                    .as_ref()
                    .clone();
                let value = Expr::AtomIsEmpty {
                    value: Box::new(value),
                };
                Ok(Some(if negated {
                    Expr::Not {
                        value: Box::new(value),
                    }
                } else {
                    value
                }))
            }
        }
    }
}
