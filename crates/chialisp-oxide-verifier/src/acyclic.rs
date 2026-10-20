//! Explicit scalar call-frame model, separate from the legacy proof APIs.
//!
//! Every table function is modeled once by nonrecursive SMT defined functions
//! for success and each scalar result field. Calls apply these definitions to
//! symbolic values; no source IR is inlined or bounded call trace unrolled.
//! The domain contains typed scalar/tuple values after ABI decoding. This does
//! not model failure order, cost, resource behavior, cryptography, or prove
//! rustc/MIR/IR/CLVM preservation. Recursion is rejected, including unused SCCs.
use crate::{
    Definition, Evaluated, Obligation, Predicate, Queries, SolverConfig, Specification, Term,
    Translation, Value, Verification, bounds, conjunction, default_value, numeral, solve,
};
use anyhow::{Context, Result, bail, ensure};
use chialisp_oxide_ir::{BinaryOp, Expr, Function, Program, Type};

const MAX_FUNCTIONS: usize = 16;
const MAX_NODES: usize = 1024;
const MAX_DEPTH: usize = 32;
const MAX_TYPE_NODES: usize = 128;
const MAX_SCALARS: usize = 16;
const MAX_SPEC_NODES: usize = 128;
const MAX_TEXT: usize = 16 * 1024 * 1024;
const MAX_WORK: usize = 100_000;
// With <=16 scalar fields, <=16 formal scalar inputs, <=128 type nodes and
// <=64-byte numeral tokens, one primitive/frame operation allocates less than
// this reservation (including captured definitions and copied tuple metadata).
const STEP_BYTES: usize = 64 * 1024;

#[derive(Default)]
struct Budget {
    work: usize,
    bytes: usize,
}
impl Budget {
    fn reserve(&mut self, work: usize, bytes: usize) -> Result<()> {
        self.work = self
            .work
            .checked_add(work)
            .context("acyclic model work overflow")?;
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .context("acyclic model text overflow")?;
        ensure!(
            self.work <= MAX_WORK && self.bytes <= MAX_TEXT,
            "acyclic scalar model work/text budget exceeded"
        );
        Ok(())
    }
    fn step(&mut self) -> Result<()> {
        self.reserve(1, STEP_BYTES)
    }
}
fn scalar_type(ty: &Type) -> Result<(usize, usize)> {
    let mut pending = vec![(ty, 1usize)];
    let mut nodes = 0;
    let mut scalars = 0;
    while let Some((ty, depth)) = pending.pop() {
        nodes += 1;
        ensure!(
            nodes <= MAX_TYPE_NODES && depth <= 16,
            "acyclic scalar model type budget exceeded"
        );
        match ty {
            Type::Int { .. } | Type::Bool => scalars += 1,
            Type::Unit => (),
            Type::Tuple { items } => {
                ensure!(
                    items.len() <= MAX_TYPE_NODES.saturating_sub(nodes + pending.len()),
                    "acyclic scalar model type budget exceeded"
                );
                pending.extend(items.iter().map(|ty| (ty, depth + 1)));
            }
            _ => bail!("acyclic scalar model does not model byte, atom, or raw-tree types"),
        }
        ensure!(
            scalars <= MAX_SCALARS,
            "acyclic scalar model scalar-field budget exceeded"
        );
    }
    Ok((nodes, scalars))
}
// Check every intermediate value, including tuples immediately projected or
// discarded. Whole IR validation and the iterative node/depth scan run first.
// Every cloned type is already bounded; tuple totals are checked before push.
fn copied_type(ty: &Type, budget: &mut Budget) -> Result<Type> {
    let (nodes, _) = scalar_type(ty)?;
    budget.reserve(1, nodes * std::mem::size_of::<Type>())?;
    Ok(ty.clone())
}
fn type_depth(ty: &Type) -> usize {
    match ty {
        Type::Tuple { items } => 1 + items.iter().map(type_depth).max().unwrap_or(0),
        _ => 1,
    }
}
fn value_shapes(
    program: &Program,
    expression: &Expr,
    parameters: &[chialisp_oxide_ir::Parameter],
    locals: &mut Vec<(usize, Type)>,
    budget: &mut Budget,
) -> Result<Option<Type>> {
    budget.reserve(1, std::mem::size_of::<Type>())?;
    Ok(match expression {
        Expr::Arg { index } => Some(copied_type(&parameters[*index].ty, budget)?),
        Expr::Local { local_id } => Some(copied_type(
            &locals
                .iter()
                .rev()
                .find(|(id, _)| id == local_id)
                .context("missing preflight lexical local")?
                .1,
            budget,
        )?),
        Expr::Unit => Some(Type::Unit),
        Expr::Bool { .. } => Some(Type::Bool),
        Expr::Int { ty, .. } => Some(copied_type(ty, budget)?),
        Expr::Reject { .. } => None,
        Expr::Let {
            local_id,
            ty,
            value,
            body,
        } => {
            value_shapes(program, value, parameters, locals, budget)?;
            let ty = copied_type(ty, budget)?;
            budget.reserve(1, std::mem::size_of::<(usize, Type)>())?;
            locals.push((*local_id, ty));
            let result = value_shapes(program, body, parameters, locals, budget);
            locals.pop();
            result?
        }
        Expr::Call {
            function_id,
            arguments,
        } => {
            for argument in arguments {
                value_shapes(program, argument, parameters, locals, budget)?;
            }
            Some(copied_type(
                &program.functions[*function_id].result,
                budget,
            )?)
        }
        Expr::Tuple { items } => {
            ensure!(
                items.len() < MAX_TYPE_NODES,
                "acyclic scalar model intermediate value/type budget exceeded"
            );
            budget.reserve(1, items.len() * std::mem::size_of::<Type>())?;
            let mut types = Vec::with_capacity(items.len());
            let mut nodes = 1;
            let mut scalars = 0;
            let mut bottom = false;
            for item in items {
                let Some(ty) = value_shapes(program, item, parameters, locals, budget)? else {
                    bottom = true;
                    continue;
                };
                let (n, f) = scalar_type(&ty)?;
                nodes += n;
                scalars += f;
                ensure!(
                    nodes <= MAX_TYPE_NODES && scalars <= MAX_SCALARS && type_depth(&ty) < 16,
                    "acyclic scalar model intermediate value/type budget exceeded"
                );
                types.push(ty);
            }
            if bottom {
                None
            } else {
                Some(Type::Tuple { items: types })
            }
        }
        Expr::Field { value, index } => {
            match value_shapes(program, value, parameters, locals, budget)? {
                Some(Type::Tuple { items }) => Some(copied_type(&items[*index], budget)?),
                None => None,
                _ => bail!("invalid preflight tuple projection"),
            }
        }
        Expr::Sequence { first, then_value } => {
            value_shapes(program, first, parameters, locals, budget)?;
            value_shapes(program, then_value, parameters, locals, budget)?
        }
        Expr::Not { value } => {
            value_shapes(program, value, parameters, locals, budget)?;
            Some(Type::Bool)
        }
        Expr::If {
            condition,
            then_value,
            else_value,
        } => {
            value_shapes(program, condition, parameters, locals, budget)?;
            let a = value_shapes(program, then_value, parameters, locals, budget)?;
            let b = value_shapes(program, else_value, parameters, locals, budget)?;
            a.or(b)
        }
        Expr::Binary {
            kind,
            ty,
            left,
            right,
        } => {
            value_shapes(program, left, parameters, locals, budget)?;
            value_shapes(program, right, parameters, locals, budget)?;
            Some(
                if matches!(
                    kind,
                    BinaryOp::Eq
                        | BinaryOp::Ne
                        | BinaryOp::Lt
                        | BinaryOp::Le
                        | BinaryOp::Gt
                        | BinaryOp::Ge
                ) {
                    Type::Bool
                } else {
                    copied_type(ty, budget)?
                },
            )
        }
        _ => unreachable!("complete scalar admission ran first"),
    })
}
fn children<'a>(
    expr: &'a Expr,
    pending: &mut Vec<(&'a Expr, usize)>,
    depth: usize,
    visited: usize,
) -> Result<()> {
    let count = match expr {
        Expr::Tuple { items }
        | Expr::Call {
            arguments: items, ..
        } => items.len(),
        Expr::Binary { .. } | Expr::Sequence { .. } | Expr::Let { .. } => 2,
        Expr::If { .. } => 3,
        Expr::Field { .. } | Expr::Not { .. } => 1,
        _ => 0,
    };
    ensure!(
        count <= MAX_NODES.saturating_sub(visited + pending.len()),
        "acyclic scalar model node budget exceeded"
    );
    match expr {
        Expr::Tuple { items }
        | Expr::Call {
            arguments: items, ..
        } => pending.extend(items.iter().map(|e| (e, depth + 1))),
        Expr::Binary {
            left: a, right: b, ..
        }
        | Expr::Sequence {
            first: a,
            then_value: b,
        }
        | Expr::Let {
            value: a, body: b, ..
        } => pending.extend([(a.as_ref(), depth + 1), (b.as_ref(), depth + 1)]),
        Expr::If {
            condition,
            then_value,
            else_value,
        } => pending.extend([
            (condition.as_ref(), depth + 1),
            (then_value.as_ref(), depth + 1),
            (else_value.as_ref(), depth + 1),
        ]),
        Expr::Field { value, .. } | Expr::Not { value } => pending.push((value, depth + 1)),
        _ => (),
    }
    Ok(())
}
fn preflight(program: &Program, budget: &mut Budget) -> Result<Vec<usize>> {
    // Validation of the WHOLE IR precedes graph construction or model admission.
    program.validate().context("invalid IR")?;
    ensure!(
        !program.uses_pairs(),
        "acyclic scalar does not model typed wire pairs"
    );
    ensure!(
        !program.uses_sha256(),
        "acyclic scalar does not model SHA256 semantics"
    );
    ensure!(
        !program.uses_apply(),
        "acyclic scalar does not model dynamic application"
    );
    ensure!(
        program.environment != chialisp_oxide_ir::Environment::Raw,
        "acyclic scalar model does not model whole raw environments"
    );
    ensure!(
        program.functions.len() <= MAX_FUNCTIONS && program.parameters.len() <= 16,
        "acyclic scalar model function/parameter budget exceeded"
    );
    let mut parameter_fields = 0;
    let mut parameter_nodes = 0;
    for p in &program.parameters {
        let (nodes, fields) = scalar_type(&p.ty)?;
        parameter_fields += fields;
        parameter_nodes += nodes;
    }
    ensure!(
        parameter_fields <= MAX_SCALARS && parameter_nodes <= MAX_TYPE_NODES,
        "acyclic scalar model input-field/type budget exceeded"
    );
    scalar_type(&program.result)?;
    budget.reserve(MAX_FUNCTIONS * MAX_FUNCTIONS, MAX_FUNCTIONS * MAX_FUNCTIONS)?;
    let mut edges = [[false; MAX_FUNCTIONS]; MAX_FUNCTIONS];
    let mut visited = 0;
    for (owner, body) in std::iter::once((None, &program.body)).chain(
        program
            .functions
            .iter()
            .enumerate()
            .map(|(i, f)| (Some(i), &f.body)),
    ) {
        if let Some(i) = owner {
            let f = &program.functions[i];
            ensure!(
                f.parameters.len() <= 16,
                "acyclic scalar model parameter budget exceeded"
            );
            let mut fields = 0;
            let mut nodes = 0;
            for p in &f.parameters {
                let (n, f) = scalar_type(&p.ty)?;
                fields += f;
                nodes += n;
            }
            ensure!(
                fields <= MAX_SCALARS && nodes <= MAX_TYPE_NODES,
                "acyclic scalar model function-input field/type budget exceeded"
            );
            scalar_type(&f.result)?;
        }
        let mut pending = vec![(body, 1)];
        while let Some((expr, depth)) = pending.pop() {
            visited += 1;
            ensure!(
                visited <= MAX_NODES && depth <= MAX_DEPTH,
                "acyclic scalar model node/depth budget exceeded"
            );
            budget.reserve(1, 0)?;
            match expr {
                Expr::Int { value, ty } => {
                    ensure!(
                        value.len() <= 64,
                        "acyclic scalar model numeral budget exceeded"
                    );
                    scalar_type(ty)?;
                }
                Expr::Let { ty, .. } => {
                    scalar_type(ty)?;
                }
                Expr::Binary { kind, ty, .. } => {
                    scalar_type(ty)?;
                    ensure!(
                        matches!(
                            kind,
                            BinaryOp::Eq
                                | BinaryOp::Ne
                                | BinaryOp::Lt
                                | BinaryOp::Le
                                | BinaryOp::Gt
                                | BinaryOp::Ge
                                | BinaryOp::Add
                                | BinaryOp::Sub
                                | BinaryOp::Mul
                                | BinaryOp::Bitand
                                | BinaryOp::Bitor
                                | BinaryOp::Bitxor
                        ),
                        "acyclic scalar model does not model this binary operation"
                    );
                }
                Expr::Call { function_id, .. } => {
                    if let Some(owner) = owner {
                        edges[owner][*function_id] = true;
                    }
                }
                Expr::Unit
                | Expr::Bool { .. }
                | Expr::Arg { .. }
                | Expr::Local { .. }
                | Expr::Reject { .. }
                | Expr::Tuple { .. }
                | Expr::Field { .. }
                | Expr::Sequence { .. }
                | Expr::Not { .. }
                | Expr::If { .. } => (),
                _ => bail!(
                    "acyclic scalar model does not model SHA256, bytes, atoms or raw-tree expressions"
                ),
            }
            children(expr, &mut pending, depth, visited)?;
        }
    }
    value_shapes(
        program,
        &program.body,
        &program.parameters,
        &mut Vec::new(),
        budget,
    )?;
    for function in &program.functions {
        value_shapes(
            program,
            &function.body,
            &function.parameters,
            &mut Vec::new(),
            budget,
        )?;
    }
    // Callee-first topological order, including every unused function. A
    // blocked remainder is a recursive SCC; no bounded recursion is admitted.
    let mut done = [false; MAX_FUNCTIONS];
    let mut order = Vec::with_capacity(program.functions.len());
    while order.len() < program.functions.len() {
        budget.reserve(MAX_FUNCTIONS * MAX_FUNCTIONS, 0)?;
        let next = (0..program.functions.len())
            .find(|i| !done[*i] && (0..program.functions.len()).all(|j| !edges[*i][j] || done[j]));
        let Some(next) = next else {
            bail!("acyclic scalar model rejects recursive function graphs, including unused SCCs")
        };
        done[next] = true;
        order.push(next);
    }
    // Bound source-body call expansion before constructing any summaries.
    // This is not a bound on expansion of all success/output SMT summaries or
    // solver memory; generation reservations and solver timeout are separate.
    let mut weights = [0usize; MAX_FUNCTIONS];
    for owner in order.iter().copied().map(Some).chain(std::iter::once(None)) {
        let body = owner.map_or(&program.body, |i| &program.functions[i].body);
        let mut pending = vec![(body, 1)];
        let mut weight = 0usize;
        let mut visits = 0;
        while let Some((expr, depth)) = pending.pop() {
            visits += 1;
            budget.reserve(1, 0)?;
            let extra = if let Expr::Call { function_id, .. } = expr {
                weights[*function_id]
            } else {
                0
            };
            weight = weight
                .checked_add(1 + extra)
                .context("acyclic model dependency work overflow")?;
            ensure!(
                weight <= MAX_WORK,
                "acyclic scalar model expanded dependency-work budget exceeded"
            );
            children(expr, &mut pending, depth, visits)?;
        }
        if let Some(owner) = owner {
            weights[owner] = weight;
        }
    }
    Ok(order)
}
fn specification_budget(spec: &Specification, budget: &mut Budget) -> Result<()> {
    enum Node<'a> {
        Predicate(&'a Predicate),
        Term(&'a Term),
    }
    let obligation = match &spec.obligation {
        Obligation::OnSuccess { property } => property,
        Obligation::AcceptanceIff { predicate } => predicate,
    };
    let mut pending = vec![
        (Node::Predicate(&spec.assumptions), 1usize),
        (Node::Predicate(obligation), 1),
    ];
    let mut count = 0;
    while let Some((node, depth)) = pending.pop() {
        count += 1;
        ensure!(
            count <= MAX_SPEC_NODES && depth <= 16,
            "acyclic scalar model specification budget exceeded"
        );
        match node {
            Node::Predicate(Predicate::Constant { .. }) | Node::Term(Term::Bool { .. }) => (),
            Node::Predicate(Predicate::Compare { left, right, .. }) => pending.extend([
                (Node::Term(left), depth + 1),
                (Node::Term(right), depth + 1),
            ]),
            Node::Predicate(Predicate::All { items } | Predicate::Any { items }) => {
                ensure!(
                    items.len() <= MAX_SPEC_NODES.saturating_sub(count + pending.len()),
                    "acyclic scalar model specification budget exceeded"
                );
                pending.extend(items.iter().map(|p| (Node::Predicate(p), depth + 1)));
            }
            Node::Predicate(Predicate::Not { value }) => {
                pending.push((Node::Predicate(value), depth + 1))
            }
            Node::Term(Term::Input { fields, .. } | Term::Output { fields }) => ensure!(
                fields.len() <= 16,
                "acyclic scalar model specification field-depth budget exceeded"
            ),
            Node::Term(Term::Int { value }) => ensure!(
                value.len() <= 64,
                "acyclic scalar model specification numeral budget exceeded"
            ),
            Node::Term(
                Term::Add { left, right } | Term::Sub { left, right } | Term::Mul { left, right },
            ) => pending.extend([
                (Node::Term(left), depth + 1),
                (Node::Term(right), depth + 1),
            ]),
        }
    }
    // Reserve before existing scalar specification construction/copying. The
    // bounded tree has <=128 nodes, depth<=16 and <=64-byte numeral tokens.
    budget.reserve(count, count * 8192)
}
fn flatten<'a>(value: &'a Value, out: &mut Vec<(&'static str, &'a str)>) {
    match value {
        Value::Int(s) => out.push(("Int", s)),
        Value::Bool(s) => out.push(("Bool", s)),
        Value::Tuple(items) => {
            for item in items {
                flatten(item, out);
            }
        }
        Value::Unit => (),
    }
}
fn formal(ty: &Type, name: &str) -> Value {
    match ty {
        Type::Int { .. } => Value::Int(name.into()),
        Type::Bool => Value::Bool(name.into()),
        Type::Tuple { items } => Value::Tuple(
            items
                .iter()
                .enumerate()
                .map(|(i, ty)| formal(ty, &format!("{name}_{i}")))
                .collect(),
        ),
        Type::Unit => Value::Unit,
        _ => unreachable!("complete scalar admission ran first"),
    }
}
fn call_value(ty: &Type, id: usize, args: &str, index: &mut usize, tr: &mut Translation) -> Value {
    match ty {
        Type::Int { .. } | Type::Bool => {
            let term = if args.is_empty() {
                format!("f_{id}_value_{}", *index)
            } else {
                format!("(f_{id}_value_{} {args})", *index)
            };
            *index += 1;
            // Capture each returned scalar now. Later aliases/calls contain
            // only bounded symbols, never recursively copied call terms.
            if matches!(ty, Type::Bool) {
                Value::Bool(tr.define("Bool", &term))
            } else {
                Value::Int(tr.define("Int", &term))
            }
        }
        Type::Tuple { items } => Value::Tuple(
            items
                .iter()
                .map(|ty| call_value(ty, id, args, index, tr))
                .collect(),
        ),
        Type::Unit => Value::Unit,
        _ => unreachable!("complete scalar admission ran first"),
    }
}
struct Frame {
    arguments: Vec<Value>,
    locals: Vec<(usize, Value)>,
}
struct Model<'a> {
    program: &'a Program,
    tr: Translation,
    budget: &'a mut Budget,
}
impl Model<'_> {
    fn expression(&mut self, e: &Expr, frame: &mut Frame) -> Result<Evaluated> {
        self.budget.step()?;
        let (success, value) = match e {
            Expr::Arg { index } => ("true".into(), Some(frame.arguments[*index].clone())),
            Expr::Local { local_id } => (
                "true".into(),
                Some(
                    frame
                        .locals
                        .iter()
                        .rev()
                        .find(|(id, _)| id == local_id)
                        .context("missing acyclic lexical local")?
                        .1
                        .clone(),
                ),
            ),
            Expr::Unit => ("true".into(), Some(Value::Unit)),
            Expr::Bool { value } => ("true".into(), Some(Value::Bool(value.to_string()))),
            Expr::Int { value, .. } => ("true".into(), Some(Value::Int(numeral(value)?))),
            Expr::Reject { .. } => ("false".into(), None),
            Expr::Let {
                local_id,
                ty,
                value,
                body,
            } => {
                let initial = self.expression(value, frame)?;
                let value = match initial.value {
                    Some(v) => v,
                    None => default_value(ty)?,
                };
                frame.locals.push((*local_id, value));
                let result = self.expression(body, frame)?;
                frame.locals.pop();
                (
                    format!("(and {} {})", initial.success, result.success),
                    result.value,
                )
            }
            Expr::Call {
                function_id,
                arguments,
            } => {
                let function = &self.program.functions[*function_id];
                let mut values = Vec::with_capacity(arguments.len());
                let mut successes = Vec::with_capacity(arguments.len() + 1);
                for (argument, parameter) in arguments.iter().zip(&function.parameters) {
                    let evaluated = self.expression(argument, frame)?;
                    successes.push(evaluated.success);
                    values.push(match evaluated.value {
                        Some(v) => v,
                        None => default_value(&parameter.ty)?,
                    });
                }
                let mut fields = Vec::new();
                for value in &values {
                    flatten(value, &mut fields);
                }
                let args = fields
                    .iter()
                    .map(|(_, value)| *value)
                    .collect::<Vec<_>>()
                    .join(" ");
                successes.push(if args.is_empty() {
                    format!("f_{function_id}_success")
                } else {
                    format!("(f_{function_id}_success {args})")
                });
                (
                    conjunction(&successes),
                    Some(call_value(
                        &function.result,
                        *function_id,
                        &args,
                        &mut 0,
                        &mut self.tr,
                    )),
                )
            }
            Expr::Tuple { items } => {
                let mut values = Vec::with_capacity(items.len());
                let mut successes = Vec::with_capacity(items.len());
                for item in items {
                    let value = self.expression(item, frame)?;
                    successes.push(value.success);
                    values.push(
                        value
                            .value
                            .context("diverging tuple item needs a typed value")?,
                    );
                }
                (conjunction(&successes), Some(Value::Tuple(values)))
            }
            Expr::Field { value, index } => {
                let value = self.expression(value, frame)?;
                (
                    value.success,
                    Some(
                        value
                            .value
                            .context("diverging tuple projection")?
                            .field(&[*index])?,
                    ),
                )
            }
            Expr::Sequence { first, then_value } => {
                let a = self.expression(first, frame)?;
                let b = self.expression(then_value, frame)?;
                (format!("(and {} {})", a.success, b.success), b.value)
            }
            Expr::Not { value } => {
                let value = self.expression(value, frame)?;
                let boolean = value.value.unwrap_or(Value::Bool("false".into()));
                (
                    value.success,
                    Some(Value::Bool(
                        self.tr
                            .define("Bool", &format!("(not {})", boolean.boolean()?)),
                    )),
                )
            }
            Expr::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.expression(condition, frame)?;
                let boolean = condition.value.unwrap_or(Value::Bool("false".into()));
                let c = boolean.boolean()?;
                let a = self.expression(then_value, frame)?;
                let b = self.expression(else_value, frame)?;
                let success = format!(
                    "(and {} (ite {c} {} {}))",
                    condition.success, a.success, b.success
                );
                let value = match (a.value, b.value) {
                    (Some(a), Some(b)) => Some(self.tr.select(c, a, b)?),
                    (a, b) => a.or(b),
                };
                (success, value)
            }
            Expr::Binary {
                kind,
                ty,
                left,
                right,
            } => {
                let a = self.expression(left, frame)?;
                let b = self.expression(right, frame)?;
                return self.tr.binary_values(*kind, ty, a, b);
            }
            _ => unreachable!("complete scalar admission ran first"),
        };
        Ok(Evaluated {
            success: self.tr.define("Bool", &success),
            value,
        })
    }
}
fn lexical_body(
    definitions: &[Definition],
    final_value: &str,
    budget: &mut Budget,
) -> Result<String> {
    budget.reserve(1, final_value.len())?;
    let mut body = final_value.to_owned();
    for definition in definitions.iter().rev() {
        let size = body
            .len()
            .checked_add(definition.name.len())
            .and_then(|n| n.checked_add(definition.expression.len()))
            .and_then(|n| n.checked_add(20))
            .context("acyclic SMT body size overflow")?;
        budget.reserve(1, size)?;
        body = format!(
            "(let (({} {})) {body})",
            definition.name, definition.expression
        );
    }
    Ok(body)
}
fn formal_domain(ty: &Type, value: &Value, out: &mut Vec<String>) -> Result<()> {
    match (ty, value) {
        (Type::Int { .. }, Value::Int(name)) => {
            let (min, max) = bounds(ty)?;
            out.push(format!("(and (<= {min} {name}) (<= {name} {max}))"));
        }
        (Type::Tuple { items }, Value::Tuple(values)) => {
            for (ty, value) in items.iter().zip(values) {
                formal_domain(ty, value, out)?;
            }
        }
        (Type::Bool, Value::Bool(_)) | (Type::Unit, Value::Unit) => (),
        _ => bail!("acyclic formal type/value mismatch"),
    }
    Ok(())
}
fn function_definition(
    program: &Program,
    function: &Function,
    budget: &mut Budget,
) -> Result<String> {
    budget.step()?;
    let arguments = function
        .parameters
        .iter()
        .enumerate()
        .map(|(i, p)| formal(&p.ty, &format!("p_{i}")))
        .collect::<Vec<_>>();
    let mut domain = Vec::new();
    for (parameter, argument) in function.parameters.iter().zip(&arguments) {
        formal_domain(&parameter.ty, argument, &mut domain)?;
    }
    let mut parameters = Vec::new();
    for argument in &arguments {
        flatten(argument, &mut parameters);
    }
    let signature = parameters
        .iter()
        .map(|(sort, name)| format!("({name} {sort})"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut model = Model {
        program,
        tr: Translation {
            definitions: Some(Vec::new()),
            ..Translation::default()
        },
        budget,
    };
    let execution = model.expression(
        &function.body,
        &mut Frame {
            arguments,
            locals: Vec::new(),
        },
    )?;
    let value = execution.value.unwrap_or(default_value(&function.result)?);
    let definitions = model
        .tr
        .definitions
        .context("missing acyclic definition capture")?;
    domain.push(execution.success);
    let success = lexical_body(&definitions, &conjunction(&domain), model.budget)?;
    model
        .budget
        .reserve(1, success.len() + signature.len() + 128)?;
    let mut result = format!(
        "(define-fun f_{}_success ({signature}) Bool {success})\n",
        function.id
    );
    let mut fields = Vec::new();
    flatten(&value, &mut fields);
    for (index, (sort, term)) in fields.into_iter().enumerate() {
        let body = lexical_body(&definitions, term, model.budget)?;
        model
            .budget
            .reserve(1, body.len() + signature.len() + 128)?;
        result.push_str(&format!(
            "(define-fun f_{}_value_{index} ({signature}) {sort} {body})\n",
            function.id
        ));
    }
    Ok(result)
}

/// Model nonrecursive scalar calls explicitly. Existing scalar/tree/byte APIs
/// continue rejecting every runtime node. Inputs are typed values after ABI;
/// success and output only are modeled, never failure order or VM resources.
pub fn emit_acyclic_scalar(program: &Program, specification: &Specification) -> Result<Queries> {
    let mut budget = Budget::default();
    let order = preflight(program, &mut budget)?;
    specification_budget(specification, &mut budget)?;
    let mut definitions = String::new();
    for id in order {
        let definition = function_definition(program, &program.functions[id], &mut budget)?;
        budget.reserve(1, definitions.len() + definition.len())?;
        definitions.push_str(&definition);
    }
    budget.step()?;
    let mut tr = Translation {
        bitvectors: true,
        ..Translation::default()
    };
    for (index, parameter) in program.parameters.iter().enumerate() {
        let argument = tr.input(&parameter.ty, &format!("arg_{index}"))?;
        tr.arguments.push(argument);
    }
    let arguments = tr.arguments.clone();
    let mut model = Model {
        program,
        tr,
        budget: &mut budget,
    };
    let execution = model.expression(
        &program.body,
        &mut Frame {
            arguments,
            locals: Vec::new(),
        },
    )?;
    let output = execution.value.unwrap_or(default_value(&program.result)?);
    let assumptions = model.tr.predicate(&specification.assumptions, None)?;
    let counterexample = match &specification.obligation {
        Obligation::OnSuccess { property } => format!(
            "(and {} (not {}))",
            execution.success,
            model.tr.predicate(property, Some(&output))?
        ),
        Obligation::AcceptanceIff { predicate } => format!(
            "(not (= {} {}))",
            execution.success,
            model.tr.predicate(predicate, None)?
        ),
    };
    let size = definitions.len()
        + model.tr.text.len()
        + assumptions.len()
        + counterexample.len()
        + execution.success.len()
        + 512;
    model.budget.reserve(
        1,
        size.checked_mul(3).context("acyclic query size overflow")?,
    )?;
    let base = format!(
        "; Explicit acyclic scalar call-frame model; typed domain after ABI.\n; Acceptance/output only; no failure-order, cost/resource or compiler-preservation proof.\n(set-logic ALL)\n{definitions}{}(assert {assumptions})\n",
        model.tr.text
    );
    Ok(Queries {
        nonvacuity: format!("{base}(assert {})\n(check-sat)\n", execution.success),
        counterexample: format!("{base}(assert {counterexample})\n(check-sat)\n"),
        parameters: program.parameters.iter().map(|p| p.ty.clone()).collect(),
        symbols: model.tr.symbols,
    })
}

/// Fail closed on graph/type/model limits or solver failure. No recursive trace
/// is unrolled and no default proof API admits runtime IR through this entry.
pub fn verify_acyclic_scalar(
    program: &Program,
    specification: &Specification,
    config: &SolverConfig,
) -> Result<Verification> {
    let queries = emit_acyclic_scalar(program, specification)?;
    let nonvacuity = solve(config, &queries, &queries.nonvacuity)?;
    if let Some(counterexample) = solve(config, &queries, &queries.counterexample)? {
        Ok(Verification::Refuted {
            counterexample,
            nonvacuity,
        })
    } else if let Some(nonvacuity) = nonvacuity {
        Ok(Verification::Proven { nonvacuity })
    } else {
        Ok(Verification::Vacuous)
    }
}
