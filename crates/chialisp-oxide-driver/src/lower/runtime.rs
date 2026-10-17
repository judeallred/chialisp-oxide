//! Closed runtime functions, selected only for reachable recursive call graphs.
//!
//! This does not change compile-time constant lowering: borrowed/promoted MIR
//! still uses the legacy acyclic substitution path and cannot capture a Local.
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
};

use chialisp_oxide_ir::{BinaryOp, Expr, Function, Parameter, Type};
use rustc_hir::def_id::DefId;
use rustc_middle::{
    mir::{
        BasicBlock, Body, Local, Location, Place, Rvalue, START_BLOCK, StatementKind,
        TerminatorKind,
    },
    ty,
};
use rustc_span::Span;

use super::{BlockDepth, Lowerer, MAX_EXPRESSION_DEPTH, empty_atoms};

const MAX_DISCOVERED_FUNCTIONS: usize = 256;
const MAX_RUNTIME_FUNCTIONS: usize = 64;
const MAX_RUNTIME_BINDINGS: usize = 2_048;

struct Runtime<'a> {
    ids: &'a HashMap<DefId, usize>,
    next_local: Cell<usize>,
}

struct Binding {
    local_id: usize,
    ty: Type,
    value: Expr,
}

impl<'tcx> Lowerer<'tcx> {
    /// Discover actual DefIds and their finite source graph. Acyclic exports
    /// deliberately return an empty table and retain their old exact artifacts.
    pub fn runtime_functions(&self, entry: DefId) -> Result<Vec<Function>, String> {
        let mut functions = vec![entry];
        let mut ids = HashMap::from([(entry, 0)]);
        let mut edges: Vec<Vec<usize>> = vec![vec![]];
        let mut graph_blocks = 0usize;
        let mut current = 0;
        while current < functions.len() {
            let body = self.function_body(functions[current])?;
            self.runtime_signature(body)?;
            let mut pending = vec![START_BLOCK];
            let mut visited = HashSet::new();
            while let Some(block) = pending.pop() {
                if !visited.insert(block) {
                    continue;
                }
                graph_blocks += 1;
                if graph_blocks > 10_000 {
                    return Err(
                        self.error(body.span, "call-graph discovery exceeded 10,000 MIR blocks")
                    );
                }
                let terminator = body.basic_blocks[block].terminator();
                let span = terminator.source_info.span;
                if let Some(target) = self.owned_plan(body)?.targets.get(&block) {
                    pending.push(*target);
                    continue;
                }
                match &terminator.kind {
                    TerminatorKind::Return | TerminatorKind::Unreachable => {}
                    TerminatorKind::Goto { target } | TerminatorKind::Assert { target, .. } => {
                        pending.push(*target)
                    }
                    TerminatorKind::SwitchInt { targets, .. } => {
                        pending.push(targets.otherwise());
                        pending.extend(targets.iter().map(|(_, block)| block));
                    }
                    TerminatorKind::Call {
                        func,
                        args,
                        destination,
                        target,
                        ..
                    } => {
                        let ty::FnDef(id, generic_args) =
                            func.ty(&body.local_decls, self.tcx).kind()
                        else {
                            return Err(self.error(span, "indirect function calls are unsupported"));
                        };
                        let name = self.tcx.def_path_str(*id);
                        if self.panic_function(*id) {
                            continue;
                        }
                        if self.box_constructor(body, func, args, *destination, span)? {
                            pending.push(
                                target.ok_or_else(|| self.error(span, "Box::new must return"))?,
                            );
                            continue;
                        }
                        if self
                            .byte_comparison(body, func, args, *destination, span)?
                            .is_some()
                        {
                            pending.push(
                                target.ok_or_else(|| {
                                    self.error(span, "byte comparison must return")
                                })?,
                            );
                            continue;
                        }
                        if generic_args
                            .skip_binder()
                            .iter()
                            .any(|arg| arg.as_region().is_none())
                        {
                            return Err(
                                self.error(span, format!("generic call `{name}` is unsupported"))
                            );
                        }
                        if !id.is_local() && !self.allowed_external(*id) {
                            return Err(self.error(
                                span,
                                format!(
                                    "external call `{name}` is unsupported; use local pure helpers"
                                ),
                            ));
                        }
                        if args.len() > 64 {
                            return Err(self.error(span, "calls are limited to 64 arguments"));
                        }
                        let callee = if let Some(index) = ids.get(id) {
                            *index
                        } else {
                            if functions.len() >= MAX_DISCOVERED_FUNCTIONS {
                                return Err(
                                    self.error(span, "call-graph discovery exceeded 256 functions")
                                );
                            }
                            let index = functions.len();
                            functions.push(*id);
                            ids.insert(*id, index);
                            edges.push(vec![]);
                            index
                        };
                        edges[current].push(callee);
                        pending.push(target.ok_or_else(|| {
                            self.error(span, "diverging helper calls are unsupported")
                        })?);
                    }
                    other => {
                        if let Some(target) = self.owned_plan(body)?.targets.get(&block) {
                            pending.push(*target);
                            continue;
                        }
                        return Err(
                            self.error(span, format!("unsupported MIR terminator: {other:?}"))
                        );
                    }
                }
            }
            current += 1;
        }
        // Kahn's algorithm detects self/mutual cycles without recursively
        // traversing the source graph on the host stack.
        let mut incoming = vec![0usize; functions.len()];
        for children in &edges {
            for child in children {
                incoming[*child] += 1;
            }
        }
        let mut ready = incoming
            .iter()
            .enumerate()
            .filter_map(|(id, count)| (*count == 0).then_some(id))
            .collect::<Vec<_>>();
        let mut removed = 0;
        while let Some(id) = ready.pop() {
            removed += 1;
            for child in &edges[id] {
                incoming[*child] -= 1;
                if incoming[*child] == 0 {
                    ready.push(*child);
                }
            }
        }
        if removed == functions.len() {
            return Ok(vec![]);
        }
        if functions.len() > MAX_RUNTIME_FUNCTIONS {
            return Err(self.error(
                self.tcx.def_span(entry),
                "runtime call graphs are limited to 64 functions",
            ));
        }
        let mut lowered = Vec::new();
        for (id, definition) in functions.into_iter().enumerate() {
            let body = self.function_body(definition)?;
            let (parameters, result) = self.runtime_signature(body)?;
            let mut locals = super::Environment::new(body.local_decls.len());
            for (index, local) in body.args_iter().enumerate() {
                locals[local.index()] = Some(Expr::Arg { index });
            }
            let runtime = Runtime {
                ids: &ids,
                next_local: Cell::new(0),
            };
            let patterns = self.empty_atom_patterns(body)?;
            let expression = self.runtime_block(
                body,
                START_BLOCK,
                locals,
                &mut vec![],
                &runtime,
                empty_atoms::State::new(&patterns),
            )?;
            let name = self.tcx.def_path_str(definition);
            lowered.push(Function {
                id,
                name: if definition.is_local() {
                    format!("{}::{name}", self.tcx.crate_name(definition.krate))
                } else {
                    name
                },
                parameters,
                result,
                body: expression,
            });
        }
        Ok(lowered)
    }

    fn runtime_signature(&self, body: &Body<'tcx>) -> Result<(Vec<Parameter>, Type), String> {
        if body.arg_count > 64 || body.local_decls.len() > 2_048 {
            return Err(self.error(
                body.span,
                "function exceeds 64 parameters or 2,048 MIR locals",
            ));
        }
        let parameters = body
            .args_iter()
            .enumerate()
            .map(|(index, local)| {
                Ok(Parameter {
                    name: format!("arg{index}"),
                    ty: self.ty(body.local_decls[local].ty, body.span)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok((parameters, self.ty(body.return_ty(), body.span)?))
    }

    fn runtime_bind(
        &self,
        assignment: (Place<'tcx>, Type, Expr),
        locals: &mut super::Environment,
        bindings: &mut Vec<Binding>,
        runtime: &Runtime<'_>,
        span: Span,
    ) -> Result<(), String> {
        let (place, ty, value) = assignment;
        if !place.projection.is_empty() {
            return Err(self.error(span, "projected assignment is unsupported in runtime functions; construct and assign a complete tuple/newtype"));
        }
        let value = self.runtime_value(value, ty, bindings, runtime, span)?;
        locals[place.local.index()] = Some(value);
        Ok(())
    }

    fn runtime_value(
        &self,
        value: Expr,
        ty: Type,
        bindings: &mut Vec<Binding>,
        runtime: &Runtime<'_>,
        span: Span,
    ) -> Result<Expr, String> {
        self.check_expression(&value, span)?;
        // These are already evaluated values or total literals. Their defining
        // call/arithmetic Let remains in scope, including when the alias is
        // discarded. Never infer totality for a runtime call or projection.
        if matches!(
            value,
            Expr::Arg { .. }
                | Expr::Local { .. }
                | Expr::Unit
                | Expr::Bool { .. }
                | Expr::Int { .. }
                | Expr::Bytes { .. }
                | Expr::Atom { .. }
        ) {
            return Ok(value);
        }
        let id = runtime.next_local.get();
        if id >= MAX_RUNTIME_BINDINGS {
            return Err(self.error(span, "runtime function exceeds 2,048 lexical bindings"));
        }
        runtime.next_local.set(id + 1);
        bindings.push(Binding {
            local_id: id,
            ty,
            value,
        });
        Ok(Expr::Local { local_id: id })
    }

    fn runtime_block(
        &self,
        body: &Body<'tcx>,
        block: BasicBlock,
        mut locals: super::Environment,
        path: &mut Vec<BasicBlock>,
        runtime: &Runtime<'_>,
        mut patterns: empty_atoms::State<'_, 'tcx>,
    ) -> Result<Expr, String> {
        let data = &body.basic_blocks[block];
        let span = data.terminator().source_info.span;
        if self.active_blocks.get() >= MAX_EXPRESSION_DEPTH {
            return Err(self.error(span, "active control-flow depth limit exceeded (64 blocks)"));
        }
        self.active_blocks.set(self.active_blocks.get() + 1);
        let _depth = BlockDepth(&self.active_blocks);
        self.blocks.set(self.blocks.get() + 1);
        if self.blocks.get() > 10_000 {
            return Err(self.error(span, "MIR expansion exceeded 10,000 blocks"));
        }
        if path.contains(&block) {
            return Err(self.error(
                span,
                "loops are unsupported; the puzzle control flow must be acyclic",
            ));
        }
        path.push(block);
        let mut bindings = vec![];
        let owned = self.owned_plan(body)?;
        for (statement_index, statement) in data.statements.iter().enumerate() {
            let statement_span = statement.source_info.span;
            match &statement.kind {
                StatementKind::Assign(assignment) => {
                    let (place, rvalue) = &**assignment;
                    if let Some(action) = owned.actions.get(&Location {
                        block,
                        statement_index,
                    }) {
                        if let super::owned::Action::Capture { pointer, source } = action {
                            let value = self.place(body, *source, &locals, statement_span)?;
                            let value = self.runtime_value(
                                value,
                                Type::Tree,
                                &mut bindings,
                                runtime,
                                statement_span,
                            )?;
                            locals.boxes.insert(*pointer, std::rc::Rc::new(value));
                        }
                        continue;
                    }
                    if let Some(action) = patterns.patterns.action(Location {
                        block,
                        statement_index,
                    }) {
                        if let empty_atoms::Action::Capture {
                            destination,
                            source,
                        } = action
                        {
                            let value = self.place(body, source, &locals, statement_span)?;
                            let value = self.runtime_value(
                                value,
                                Type::Atom,
                                &mut bindings,
                                runtime,
                                statement_span,
                            )?;
                            patterns.values.insert(destination, std::rc::Rc::new(value));
                        } else if let Some(value) = self.empty_atom_action(
                            body,
                            action,
                            &locals,
                            &mut patterns.values,
                            statement_span,
                        )? {
                            self.runtime_bind(
                                (*place, Type::Bool, value),
                                &mut locals,
                                &mut bindings,
                                runtime,
                                statement_span,
                            )?;
                        }
                        continue;
                    }
                    if !place.projection.is_empty() {
                        return Err(self.error(statement_span, "projected assignment is unsupported in runtime functions; construct and assign a complete tuple/newtype"));
                    }
                    let ty = if matches!(rvalue, Rvalue::Discriminant(_)) {
                        Type::Int {
                            bits: 8,
                            signed: false,
                        }
                    } else {
                        self.ty(body.local_decls[place.local].ty, statement_span)?
                    };
                    let value = self.rvalue(body, rvalue, &locals, statement_span)?;
                    self.runtime_bind(
                        (*place, ty, value),
                        &mut locals,
                        &mut bindings,
                        runtime,
                        statement_span,
                    )?;
                }
                StatementKind::StorageLive(_)
                | StatementKind::StorageDead(_)
                | StatementKind::Nop
                | StatementKind::ConstEvalCounter
                | StatementKind::PlaceMention(_) => {}
                other => {
                    return Err(self.error(
                        statement_span,
                        format!("unsupported MIR statement: {other:?}"),
                    ));
                }
            }
        }
        let result = if let Some(target) = owned.targets.get(&block) {
            self.runtime_block(body, *target, locals, path, runtime, patterns)?
        } else {
            match &data.terminator().kind {
                TerminatorKind::Return => {
                    self.place(body, Place::from(Local::from_usize(0)), &locals, span)?
                }
                TerminatorKind::Goto { target } => {
                    self.runtime_block(body, *target, locals, path, runtime, patterns)?
                }
                TerminatorKind::SwitchInt { discr, targets } => {
                    let value = self.operand(body, discr, &locals, span)?;
                    let tree = self.tree_discriminant(body, discr, &locals, span)?;
                    let ty = if tree.is_some() {
                        Type::Int {
                            bits: 8,
                            signed: false,
                        }
                    } else {
                        self.ty(discr.ty(&body.local_decls, self.tcx), span)?
                    };
                    let mut result = self.runtime_block(
                        body,
                        targets.otherwise(),
                        self.clone_locals(&locals, span)?,
                        &mut path.clone(),
                        runtime,
                        patterns.clone(),
                    )?;
                    for (bits, target) in targets.iter().collect::<Vec<_>>().into_iter().rev() {
                        let condition = if let Some((is_pair, shape)) = &tree {
                            if bits == shape.pair.index() as u128 {
                                is_pair.clone()
                            } else if bits == shape.atom.index() as u128 {
                                Expr::Not {
                                    value: Box::new(is_pair.clone()),
                                }
                            } else {
                                Expr::Bool { value: false }
                            }
                        } else {
                            Expr::Binary {
                                kind: BinaryOp::Eq,
                                left: Box::new(value.clone()),
                                right: Box::new(self.scalar(bits, &ty, span)?),
                                ty: ty.clone(),
                            }
                        };
                        result = Expr::If {
                            condition: Box::new(condition),
                            then_value: Box::new(self.runtime_block(
                                body,
                                target,
                                self.clone_locals(&locals, span)?,
                                &mut path.clone(),
                                runtime,
                                patterns.clone(),
                            )?),
                            else_value: Box::new(result),
                        };
                        self.check_expression(&result, span)?;
                    }
                    result
                }
                TerminatorKind::Assert {
                    cond,
                    expected,
                    target,
                    ..
                } => {
                    let mut condition = self.operand(body, cond, &locals, span)?;
                    if !expected {
                        condition = Expr::Not {
                            value: Box::new(condition),
                        };
                    }
                    Expr::If {
                        condition: Box::new(condition),
                        then_value: Box::new(
                            self.runtime_block(body, *target, locals, path, runtime, patterns)?,
                        ),
                        else_value: Box::new(Expr::Reject {
                            message: self.error(span, "Rust assertion failed"),
                        }),
                    }
                }
                TerminatorKind::Call {
                    func,
                    args,
                    destination,
                    target,
                    ..
                } => {
                    let ty::FnDef(id, _) = func.ty(&body.local_decls, self.tcx).kind() else {
                        return Err(self.error(span, "indirect function calls are unsupported"));
                    };
                    if self.panic_function(*id) {
                        Expr::Reject {
                            message: self.error(span, "Rust panic/assertion rejects the puzzle"),
                        }
                    } else if self.box_constructor(body, func, args, *destination, span)? {
                        let value = self.operand(body, &args[0].node, &locals, args[0].span)?;
                        self.runtime_bind(
                            (*destination, Type::Tree, value),
                            &mut locals,
                            &mut bindings,
                            runtime,
                            span,
                        )?;
                        self.runtime_block(
                            body,
                            target.ok_or_else(|| self.error(span, "Box::new must return"))?,
                            locals,
                            path,
                            runtime,
                            patterns,
                        )?
                    } else if let Some(comparison) =
                        self.byte_comparison(body, func, args, *destination, span)?
                    {
                        let left = self.operand(body, &args[0].node, &locals, args[0].span)?;
                        let right = self.operand(body, &args[1].node, &locals, args[1].span)?;
                        let value = comparison.expression(left, right);
                        self.runtime_bind(
                            (*destination, Type::Bool, value),
                            &mut locals,
                            &mut bindings,
                            runtime,
                            span,
                        )?;
                        self.runtime_block(
                            body,
                            target
                                .ok_or_else(|| self.error(span, "byte comparison must return"))?,
                            locals,
                            path,
                            runtime,
                            patterns,
                        )?
                    } else {
                        let function_id = runtime.ids.get(id).copied().ok_or_else(|| {
                            self.error(
                                span,
                                "runtime call is absent from the validated function table",
                            )
                        })?;
                        let arguments = args
                            .iter()
                            .map(|arg| self.operand(body, &arg.node, &locals, arg.span))
                            .collect::<Result<Vec<_>, _>>()?;
                        let value = Expr::Call {
                            function_id,
                            arguments,
                        };
                        let ty = self.ty(body.local_decls[destination.local].ty, span)?;
                        self.runtime_bind(
                            (*destination, ty, value),
                            &mut locals,
                            &mut bindings,
                            runtime,
                            span,
                        )?;
                        let target = target.ok_or_else(|| {
                            self.error(span, "diverging helper calls are unsupported")
                        })?;
                        self.runtime_block(body, target, locals, path, runtime, patterns)?
                    }
                }
                TerminatorKind::Unreachable => Expr::Reject {
                    message: self.error(span, "unreachable Rust path"),
                },
                other => {
                    return Err(self.error(span, format!("unsupported MIR terminator: {other:?}")));
                }
            }
        };
        path.pop();
        let mut result = result;
        for binding in bindings.into_iter().rev() {
            result = Expr::Let {
                local_id: binding.local_id,
                ty: binding.ty,
                value: Box::new(binding.value),
                body: Box::new(result),
            };
            self.check_expression(&result, span)?;
        }
        self.check_expression(&result, span)?;
        Ok(result)
    }
}
