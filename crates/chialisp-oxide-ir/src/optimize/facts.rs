//! Forward success refinements over immutable values.
//!
//! Facts describe only a continuation reached AFTER an expression returns.
//! They never justify removing an earlier guard from a later projection. Calls
//! have no summaries: if one diverges, no later rewritten expression executes.
use crate::{BinaryOp, Expr, Parameter, Program, Type};
use anyhow::{Result, ensure};
use std::{collections::BTreeMap, rc::Rc};

type Identity = usize;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Atom,
    Pair,
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Predicate {
    identity: Identity,
    pair_when_true: bool,
}
impl Predicate {
    fn inverted(self) -> Self {
        Self {
            pair_when_true: !self.pair_when_true,
            ..self
        }
    }
}
#[derive(Clone)]
struct Tag {
    predicate: Predicate,
    when_true: String,
    when_false: String,
}
#[derive(Clone, Default)]
struct Value {
    identity: Option<Identity>,
    shape: Option<Shape>,
    boolean: Option<bool>,
    integer: Option<String>,
    predicate: Option<Predicate>,
    tag: Option<Tag>,
    tuple: Option<Vec<Rc<Value>>>,
    pair: Option<(Rc<Value>, Rc<Value>)>,
}
#[derive(Clone, Default)]
struct Context {
    facts: BTreeMap<Identity, Shape>,
    locals: BTreeMap<usize, Rc<Value>>,
    arguments: Vec<Rc<Value>>,
    successful: bool,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Projection {
    First,
    Rest,
    Field(usize),
}
#[derive(Default)]
struct Analysis {
    next_identity: usize,
    projections: BTreeMap<(Identity, Projection), Identity>,
    work: usize,
}
impl Analysis {
    fn charge(&mut self, amount: usize) -> Result<()> {
        self.work = self.work.saturating_add(amount);
        ensure!(
            self.work <= 500_000,
            "forward shape refinement work budget exceeded"
        );
        Ok(())
    }
    fn fresh(&mut self) -> Result<Identity> {
        self.charge(1)?;
        let result = self.next_identity;
        self.next_identity += 1;
        Ok(result)
    }
    fn projected(&mut self, identity: Identity, projection: Projection) -> Result<Identity> {
        self.charge(1)?;
        if let Some(identity) = self.projections.get(&(identity, projection)) {
            return Ok(*identity);
        }
        let child = self.fresh()?;
        self.projections.insert((identity, projection), child);
        Ok(child)
    }
    fn fork(&mut self, context: &Context) -> Result<Context> {
        // Charge before allocating/copying branch contexts. Immutable metadata
        // is shared, not recursively cloned (including aliased tuple values).
        self.charge(1 + context.facts.len() + context.locals.len() + context.arguments.len())?;
        Ok(context.clone())
    }
    fn typed(&mut self, ty: &Type, identity: Identity) -> Result<Rc<Value>> {
        self.charge(1)?;
        let shape = match ty {
            Type::Tree => None,
            Type::Tuple { items } if !items.is_empty() => Some(Shape::Pair),
            _ => Some(Shape::Atom),
        };
        let tuple = if let Type::Tuple { items } = ty {
            let mut fields = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                let child = self.projected(identity, Projection::Field(index))?;
                fields.push(self.typed(item, child)?);
            }
            Some(fields)
        } else {
            None
        };
        Ok(Rc::new(Value {
            identity: Some(identity),
            shape,
            tuple,
            ..Value::default()
        }))
    }
    fn scope(&mut self, parameters: &[Parameter]) -> Result<Context> {
        let mut arguments = Vec::with_capacity(parameters.len());
        for parameter in parameters {
            let identity = self.fresh()?;
            arguments.push(self.typed(&parameter.ty, identity)?);
        }
        Ok(Context {
            arguments,
            successful: true,
            ..Context::default()
        })
    }
    fn assume(&mut self, context: &mut Context, predicate: Predicate, value: bool) -> Result<()> {
        self.charge(1)?;
        let shape = if value == predicate.pair_when_true {
            Shape::Pair
        } else {
            Shape::Atom
        };
        if context
            .facts
            .get(&predicate.identity)
            .is_some_and(|known| *known != shape)
        {
            context.successful = false;
        }
        context.facts.insert(predicate.identity, shape);
        Ok(())
    }
    fn require_shape(&mut self, context: &mut Context, value: &Value, shape: Shape) -> Result<()> {
        self.charge(1)?;
        if known_shape(value, context).is_some_and(|known| known != shape) {
            context.successful = false;
        }
        if let Some(identity) = value.identity {
            context.facts.insert(identity, shape);
        }
        Ok(())
    }
    fn merge(&mut self, output: &mut Context, left: &Context, right: &Context) -> Result<()> {
        self.charge(1 + left.facts.len() + right.facts.len())?;
        output.facts = match (left.successful, right.successful) {
            (true, false) => left.facts.clone(),
            (false, true) => right.facts.clone(),
            (true, true) => left
                .facts
                .iter()
                .filter(|(id, shape)| right.facts.get(*id) == Some(*shape))
                .map(|(id, shape)| (*id, *shape))
                .collect(),
            (false, false) => BTreeMap::new(),
        };
        output.successful = left.successful || right.successful;
        Ok(())
    }
    fn expression(
        &mut self,
        expression: &Expr,
        context: &mut Context,
    ) -> Result<(Expr, Rc<Value>)> {
        self.charge(1)?;
        let mut value = Value::default();
        let result = match expression {
            Expr::Arg { index } => {
                return Ok((expression.clone(), context.arguments[*index].clone()));
            }
            Expr::Local { local_id } => {
                return Ok((expression.clone(), context.locals[local_id].clone()));
            }
            Expr::Let {
                local_id,
                ty,
                value: initializer,
                body,
            } => {
                let (initializer, initial) = self.expression(initializer, context)?;
                let identity = match initial.identity {
                    Some(id) => id,
                    None => self.fresh()?,
                };
                self.charge(initial.tuple.as_ref().map_or(1, |items| 1 + items.len()))?;
                let mut bound = initial.as_ref().clone();
                bound.identity = Some(identity);
                if bound.shape.is_none() && !matches!(ty, Type::Tree) {
                    // Only the declared result type is used; no assumptions
                    // about whether a Call ever returns are made.
                    let typed = self.typed(ty, identity)?;
                    bound.shape = typed.shape;
                    if bound.tuple.is_none() {
                        bound.tuple = typed.tuple.clone();
                    }
                }
                if let Some(shape) = bound.shape {
                    self.require_shape(context, &bound, shape)?;
                }
                context.locals.insert(*local_id, Rc::new(bound));
                let (body, info) = self.expression(body, context)?;
                context.locals.remove(local_id);
                return Ok((
                    Expr::Let {
                        local_id: *local_id,
                        ty: ty.clone(),
                        value: Box::new(initializer),
                        body: Box::new(body),
                    },
                    info,
                ));
            }
            Expr::Sequence { first, then_value } => {
                let (first, _) = self.expression(first, context)?;
                let (then_value, info) = self.expression(then_value, context)?;
                return Ok((
                    Expr::Sequence {
                        first: Box::new(first),
                        then_value: Box::new(then_value),
                    },
                    info,
                ));
            }
            Expr::If {
                condition,
                then_value,
                else_value,
            } => {
                let (condition, condition_info) = self.expression(condition, context)?;
                if let Some(selected) = known_boolean(&condition_info, context) {
                    let (branch, info) =
                        self.expression(if selected { then_value } else { else_value }, context)?;
                    return Ok((preserve(condition, branch), info));
                }
                let mut then_context = self.fork(context)?;
                let mut else_context = self.fork(context)?;
                if let Some(predicate) = condition_info.predicate {
                    self.assume(&mut then_context, predicate, true)?;
                    self.assume(&mut else_context, predicate, false)?;
                }
                let (then_value, left) = self.expression(then_value, &mut then_context)?;
                let (else_value, right) = self.expression(else_value, &mut else_context)?;
                self.charge(
                    1 + left.tuple.as_ref().map_or(0, Vec::len)
                        + right.tuple.as_ref().map_or(0, Vec::len),
                )?;
                value = merge_value(&left, &right, &then_context, &else_context);
                if let (Some(predicate), Some(a), Some(b)) =
                    (condition_info.predicate, &left.integer, &right.integer)
                {
                    value.tag = Some(Tag {
                        predicate,
                        when_true: a.clone(),
                        when_false: b.clone(),
                    });
                }
                self.merge(context, &then_context, &else_context)?;
                Expr::If {
                    condition: Box::new(condition),
                    then_value: Box::new(then_value),
                    else_value: Box::new(else_value),
                }
            }
            Expr::TreeIsPair { value: operand } => {
                let (operand, info) = self.expression(operand, context)?;
                if let Some(shape) = known_shape(&info, context) {
                    let selected = shape == Shape::Pair;
                    return Ok((
                        preserve(operand, Expr::Bool { value: selected }),
                        Rc::new(Value {
                            shape: Some(Shape::Atom),
                            boolean: Some(selected),
                            ..Value::default()
                        }),
                    ));
                }
                value.shape = Some(Shape::Atom);
                value.predicate = info.identity.map(|identity| Predicate {
                    identity,
                    pair_when_true: true,
                });
                Expr::TreeIsPair {
                    value: Box::new(operand),
                }
            }
            Expr::TreeFirst { value: operand } | Expr::TreeRest { value: operand } => {
                let (operand, info) = self.expression(operand, context)?;
                self.require_shape(context, &info, Shape::Pair)?;
                let first = matches!(expression, Expr::TreeFirst { .. });
                let output = if let Some((a, b)) = &info.pair {
                    if first { a.clone() } else { b.clone() }
                } else {
                    Rc::new(Value {
                        identity: match info.identity {
                            Some(id) => Some(self.projected(
                                id,
                                if first {
                                    Projection::First
                                } else {
                                    Projection::Rest
                                },
                            )?),
                            None => None,
                        },
                        ..Value::default()
                    })
                };
                let result = if first {
                    Expr::TreeFirst {
                        value: Box::new(operand),
                    }
                } else {
                    Expr::TreeRest {
                        value: Box::new(operand),
                    }
                };
                return Ok((result, output));
            }
            Expr::TreeAtomValue { value: operand } => {
                let (operand, info) = self.expression(operand, context)?;
                self.require_shape(context, &info, Shape::Atom)?;
                // Keep the checked Tree->Atom cast. A Tree/Bytes Local is not
                // an Atom-typed expression even when its underlying bytes match.
                value.identity = info.identity;
                value.shape = Some(Shape::Atom);
                Expr::TreeAtomValue {
                    value: Box::new(operand),
                }
            }
            Expr::TreeAtom { value: operand } => {
                let (operand, info) = self.expression(operand, context)?;
                value.identity = info.identity;
                value.shape = Some(Shape::Atom);
                Expr::TreeAtom {
                    value: Box::new(operand),
                }
            }
            Expr::TreePair { first, rest } => {
                let (first, a) = self.expression(first, context)?;
                let (rest, b) = self.expression(rest, context)?;
                value.shape = Some(Shape::Pair);
                value.pair = Some((a, b));
                Expr::TreePair {
                    first: Box::new(first),
                    rest: Box::new(rest),
                }
            }
            Expr::Tuple { items } => {
                let mut output = Vec::with_capacity(items.len());
                let mut fields = Vec::with_capacity(items.len());
                for item in items {
                    let (item, info) = self.expression(item, context)?;
                    output.push(item);
                    fields.push(info);
                }
                value.shape = Some(if items.is_empty() {
                    Shape::Atom
                } else {
                    Shape::Pair
                });
                value.tuple = Some(fields);
                Expr::Tuple { items: output }
            }
            Expr::Field {
                value: operand,
                index,
            } => {
                let (operand, info) = self.expression(operand, context)?;
                let output = if let Some(items) = &info.tuple {
                    items[*index].clone()
                } else {
                    Rc::new(Value {
                        identity: match info.identity {
                            Some(id) => Some(self.projected(id, Projection::Field(*index))?),
                            None => None,
                        },
                        ..Value::default()
                    })
                };
                return Ok((
                    Expr::Field {
                        value: Box::new(operand),
                        index: *index,
                    },
                    output,
                ));
            }
            Expr::Not { value: operand } => {
                let (operand, info) = self.expression(operand, context)?;
                value.shape = Some(Shape::Atom);
                value.boolean = known_boolean(&info, context).map(|value| !value);
                value.predicate = info.predicate.map(Predicate::inverted);
                Expr::Not {
                    value: Box::new(operand),
                }
            }
            Expr::Binary {
                kind,
                left,
                right,
                ty,
            } => {
                let (left, a) = self.expression(left, context)?;
                let (right, b) = self.expression(right, context)?;
                value.shape = Some(Shape::Atom);
                if matches!(kind, BinaryOp::Eq | BinaryOp::Ne) {
                    comparison(&mut value, &a, &b, *kind == BinaryOp::Eq, context);
                }
                Expr::Binary {
                    kind: *kind,
                    left: Box::new(left),
                    right: Box::new(right),
                    ty: ty.clone(),
                }
            }
            Expr::Call {
                function_id,
                arguments,
            } => {
                let mut output = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    output.push(self.expression(argument, context)?.0);
                }
                // No function summary, expression interning, or return fact.
                Expr::Call {
                    function_id: *function_id,
                    arguments: output,
                }
            }
            Expr::Sha256 { parts } => {
                let mut output = Vec::with_capacity(parts.len());
                for part in parts {
                    output.push(self.expression(part, context)?.0);
                }
                value.shape = Some(Shape::Atom);
                Expr::Sha256 { parts: output }
            }
            Expr::AtomIsEmpty { value: operand } => {
                let (operand, _) = self.expression(operand, context)?;
                value.shape = Some(Shape::Atom);
                Expr::AtomIsEmpty {
                    value: Box::new(operand),
                }
            }
            Expr::Bool { value: boolean } => {
                value.shape = Some(Shape::Atom);
                value.boolean = Some(*boolean);
                expression.clone()
            }
            Expr::Int { value: integer, .. } => {
                value.shape = Some(Shape::Atom);
                // Decimal spelling is not numeric identity. Keep source IR
                // unchanged, but compare tags using the validated integer's
                // canonical value (e.g. 01/+1 and -0/0 are equal).
                value.integer = Some(integer.parse::<num_bigint::BigInt>()?.to_string());
                expression.clone()
            }
            Expr::Atom { .. } | Expr::Bytes { .. } | Expr::Unit => {
                value.shape = Some(Shape::Atom);
                expression.clone()
            }
            Expr::Reject { .. } => {
                context.successful = false;
                expression.clone()
            }
        };
        Ok((result, Rc::new(value)))
    }
}

fn known_shape(value: &Value, context: &Context) -> Option<Shape> {
    value
        .identity
        .and_then(|id| context.facts.get(&id).copied())
        .or(value.shape)
}
fn known_boolean(value: &Value, context: &Context) -> Option<bool> {
    value.boolean.or_else(|| {
        value.predicate.and_then(|predicate| {
            context
                .facts
                .get(&predicate.identity)
                .map(|shape| (*shape == Shape::Pair) == predicate.pair_when_true)
        })
    })
}
fn preserve(first: Expr, then_value: Expr) -> Expr {
    // Validated leaves have no evaluation effects. Everything else remains in
    // exactly its old position, even a known-shape Call/Pair/checked projection.
    if matches!(
        first,
        Expr::Arg { .. }
            | Expr::Local { .. }
            | Expr::Atom { .. }
            | Expr::Bytes { .. }
            | Expr::Int { .. }
            | Expr::Bool { .. }
            | Expr::Unit
    ) {
        then_value
    } else {
        Expr::Sequence {
            first: Box::new(first),
            then_value: Box::new(then_value),
        }
    }
}
fn merge_value(left: &Rc<Value>, right: &Rc<Value>, a: &Context, b: &Context) -> Value {
    if !a.successful {
        return right.as_ref().clone();
    }
    if !b.successful {
        return left.as_ref().clone();
    }
    Value {
        identity: left.identity.filter(|id| right.identity == Some(*id)),
        shape: known_shape(left, a).filter(|shape| known_shape(right, b) == Some(*shape)),
        boolean: known_boolean(left, a).filter(|value| known_boolean(right, b) == Some(*value)),
        integer: left
            .integer
            .as_ref()
            .filter(|value| right.integer.as_ref() == Some(*value))
            .cloned(),
        predicate: left
            .predicate
            .filter(|value| right.predicate == Some(*value)),
        ..Value::default()
    }
}
fn comparison(output: &mut Value, left: &Value, right: &Value, equal: bool, context: &Context) {
    if let (Some(a), Some(b)) = (known_boolean(left, context), known_boolean(right, context)) {
        output.boolean = Some((a == b) == equal);
    }
    for (value, literal) in [(left, right), (right, left)] {
        if let (Some(predicate), Some(boolean)) = (value.predicate, known_boolean(literal, context))
        {
            output.predicate = Some(if boolean == equal {
                predicate
            } else {
                predicate.inverted()
            });
        }
        if let (Some(tag), Some(integer)) = (&value.tag, &literal.integer) {
            let a = (tag.when_true == *integer) == equal;
            let b = (tag.when_false == *integer) == equal;
            if a == b {
                output.boolean = Some(a);
            } else {
                output.predicate = Some(if a {
                    tag.predicate
                } else {
                    tag.predicate.inverted()
                });
            }
        }
    }
}

pub(super) fn optimize(program: &Program) -> Result<Program> {
    // Validate before any recursive walk or clone. Invalid/dead IR cannot gain
    // admission because a branch happens to become removable under facts.
    program.validate()?;
    let mut analysis = Analysis::default();
    let mut result = program.clone();
    let mut context = analysis.scope(&program.parameters)?;
    result.body = analysis.expression(&program.body, &mut context)?.0;
    for (source, output) in program.functions.iter().zip(&mut result.functions) {
        // Fresh argument/local scopes; caller facts are never imported here.
        let mut context = analysis.scope(&source.parameters)?;
        output.body = analysis.expression(&source.body, &mut context)?.0;
    }
    result.validate()?;
    Ok(result)
}
