//! Bounded partial evaluation of validated typed values.
//!
//! `known` describes the value of an entire, logically total expression. It is
//! never a promise about a call, a hash, a failing projection or overflowing
//! arithmetic. Aggregate values are shared metadata, not expanded syntax.
use std::{cmp::Ordering, rc::Rc};

use crate::{BinaryOp, Expr, Function, Parameter, Program, Type, validate::bounds};
use anyhow::{Result, ensure};
use num_bigint::BigInt;

const MAX_WORK: usize = 2_000_000;
const MAX_COPIED_BYTES: usize = 4 * 1024 * 1024;
const MAX_INLINE_BYTES: usize = 64;

#[derive(Default)]
struct Budget {
    work: usize,
    copied_bytes: usize,
}
impl Budget {
    fn work(&mut self, amount: usize) -> Result<()> {
        self.work = self.work.saturating_add(amount);
        ensure!(
            self.work <= MAX_WORK,
            "constant optimization work budget exceeded"
        );
        Ok(())
    }
    fn bytes(&mut self, amount: usize) -> Result<()> {
        self.copied_bytes = self.copied_bytes.saturating_add(amount);
        ensure!(
            self.copied_bytes <= MAX_COPIED_BYTES,
            "constant optimization literal-copy budget exceeded"
        );
        Ok(())
    }
    fn text(&mut self, text: &str) -> Result<String> {
        self.bytes(text.len())?;
        Ok(text.to_owned())
    }
    fn ty(&mut self, ty: &Type) -> Result<Type> {
        // The whole input has already passed the type-depth/node limits. Charge
        // each forwarded type before cloning a potentially wide tuple.
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            self.work(1)?;
            if let Type::Tuple { items } = ty {
                self.work(items.len())?;
                pending.extend(items);
            }
        }
        Ok(ty.clone())
    }
    fn parameters(&mut self, parameters: &[Parameter]) -> Result<Vec<Parameter>> {
        parameters
            .iter()
            .map(|parameter| {
                Ok(Parameter {
                    name: self.text(&parameter.name)?,
                    ty: self.ty(&parameter.ty)?,
                })
            })
            .collect()
    }
}

/// Known aggregate trees may share subvalues exponentially. Never recursively
/// compare, serialize or materialize them; only inspect their outer constructor.
enum Known {
    Unit,
    Bool(bool),
    Int { value: BigInt, ty: Type },
    Bytes(Rc<[u8]>),
    Atom(Rc<[u8]>),
    Tuple(Vec<Rc<Known>>),
    TreeAtom(Rc<[u8]>),
    TreePair(Rc<Known>, Rc<Known>),
}
impl Known {
    fn scalar_expression(&self, budget: &mut Budget) -> Result<Option<Expr>> {
        Ok(Some(match self {
            Self::Unit => Expr::Unit,
            Self::Bool(value) => Expr::Bool { value: *value },
            Self::Int { value, ty } => {
                budget.bytes(64)?; // Validated integer widths are at most 128.
                Expr::Int {
                    value: value.to_string(),
                    ty: budget.ty(ty)?,
                }
            }
            Self::Bytes(bytes) | Self::Atom(bytes) if bytes.len() <= MAX_INLINE_BYTES => {
                budget.bytes(bytes.len() * 2)?;
                let hex = hex::encode(bytes.as_ref());
                if matches!(self, Self::Atom(_)) {
                    Expr::Atom { hex }
                } else {
                    Expr::Bytes { hex }
                }
            }
            Self::Bytes(_)
            | Self::Atom(_)
            | Self::Tuple(_)
            | Self::TreeAtom(_)
            | Self::TreePair(_, _) => return Ok(None),
        }))
    }
}

struct Reduced {
    expr: Expr,
    known: Option<Rc<Known>>,
    total: bool,
}
impl Reduced {
    fn finish(
        expr: Expr,
        known: Option<Rc<Known>>,
        total: bool,
        budget: &mut Budget,
    ) -> Result<Self> {
        debug_assert!(known.is_none() || total);
        let expr = match &known {
            Some(value) => value.scalar_expression(budget)?.unwrap_or(expr),
            None => expr,
        };
        Ok(Self { expr, known, total })
    }
}

struct Binding {
    id: usize,
    known: Option<Rc<Known>>,
}

/// Fold each function in its own argument/local scope. Never follow a call,
/// specialize functions from caller arguments, prune the table or change ABI.
pub(super) fn optimize(program: &Program) -> Result<Program> {
    program.validate()?;
    let mut budget = Budget::default();
    let body = expression(&program.body, &mut Vec::new(), &mut budget)?.expr;
    let mut functions = Vec::with_capacity(program.functions.len());
    for function in &program.functions {
        functions.push(Function {
            id: function.id,
            name: budget.text(&function.name)?,
            parameters: budget.parameters(&function.parameters)?,
            result: budget.ty(&function.result)?,
            body: expression(&function.body, &mut Vec::new(), &mut budget)?.expr,
        });
    }
    let result = Program {
        name: budget.text(&program.name)?,
        parameters: budget.parameters(&program.parameters)?,
        result: budget.ty(&program.result)?,
        body,
        environment: program.environment,
        functions,
    };
    result.validate()?;
    Ok(result)
}

fn comparison(kind: BinaryOp, order: Ordering) -> Option<bool> {
    Some(match kind {
        BinaryOp::Eq => order == Ordering::Equal,
        BinaryOp::Ne => order != Ordering::Equal,
        BinaryOp::Lt => order == Ordering::Less,
        BinaryOp::Le => order != Ordering::Greater,
        BinaryOp::Gt => order == Ordering::Greater,
        BinaryOp::Ge => order != Ordering::Less,
        BinaryOp::Add
        | BinaryOp::Sub
        | BinaryOp::Mul
        | BinaryOp::Div
        | BinaryOp::Rem
        | BinaryOp::Bitand
        | BinaryOp::Bitor
        | BinaryOp::Bitxor
        | BinaryOp::Shl
        | BinaryOp::Shr => return None,
    })
}

fn binary(
    kind: BinaryOp,
    ty: &Type,
    left: &Known,
    right: &Known,
    budget: &mut Budget,
) -> Result<Option<Rc<Known>>> {
    let value = match (ty, left, right) {
        (
            Type::Int { .. },
            Known::Int {
                value: left,
                ty: lt,
            },
            Known::Int {
                value: right,
                ty: rt,
            },
        ) if lt == ty && rt == ty => {
            if let Some(value) = comparison(kind, left.cmp(right)) {
                Known::Bool(value)
            } else {
                let value = match kind {
                    BinaryOp::Add => left + right,
                    BinaryOp::Sub => left - right,
                    BinaryOp::Mul => left * right,
                    BinaryOp::Bitand => left & right,
                    BinaryOp::Bitor => left | right,
                    BinaryOp::Bitxor => left ^ right,
                    BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
                    | BinaryOp::Div
                    | BinaryOp::Rem
                    | BinaryOp::Shl
                    | BinaryOp::Shr => return Ok(None),
                };
                let (min, max) = bounds(ty)?;
                if value < min || value > max {
                    // An overflow is an effect, not a successfully known value.
                    return Ok(None);
                }
                Known::Int {
                    value,
                    ty: budget.ty(ty)?,
                }
            }
        }
        (Type::Bool, Known::Bool(left), Known::Bool(right)) => Known::Bool(match kind {
            BinaryOp::Eq => left == right,
            BinaryOp::Ne => left != right,
            BinaryOp::Bitand => *left & *right,
            BinaryOp::Bitor => *left | *right,
            BinaryOp::Bitxor => *left ^ *right,
            BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::Rem
            | BinaryOp::Shl
            | BinaryOp::Shr => return Ok(None),
        }),
        (Type::Bytes { .. }, Known::Bytes(left), Known::Bytes(right))
        | (Type::Atom, Known::Atom(left), Known::Atom(right)) => {
            budget.work(left.len().saturating_add(right.len()))?;
            let Some(value) = comparison(kind, left.as_ref().cmp(right.as_ref())) else {
                return Ok(None);
            };
            Known::Bool(value)
        }
        _ => return Ok(None),
    };
    Ok(Some(Rc::new(value)))
}

fn expression(expr: &Expr, bindings: &mut Vec<Binding>, budget: &mut Budget) -> Result<Reduced> {
    // Input validation bounds depth to 255 and visits to 20,000 across all
    // bodies. This separate work/copy budget also charges scans/substitution.
    budget.work(1 + bindings.len())?;
    let (expr, known, total) = match expr {
        Expr::Pair { .. } | Expr::PairFirst { .. } | Expr::PairRest { .. } => {
            anyhow::bail!("typed pair frontend rewrites are not admitted")
        }
        Expr::Unit => (Expr::Unit, Some(Rc::new(Known::Unit)), true),
        Expr::Bool { value } => (
            Expr::Bool { value: *value },
            Some(Rc::new(Known::Bool(*value))),
            true,
        ),
        Expr::Int { value, ty } => {
            let known = Known::Int {
                value: value.parse()?,
                ty: budget.ty(ty)?,
            };
            // Preserve the original literal spelling when it is not substituted.
            return Ok(Reduced {
                expr: Expr::Int {
                    value: budget.text(value)?,
                    ty: budget.ty(ty)?,
                },
                known: Some(Rc::new(known)),
                total: true,
            });
        }
        Expr::Bytes { hex } | Expr::Atom { hex } => {
            budget.bytes(hex.len().saturating_add(hex.len() / 2))?;
            let bytes: Rc<[u8]> = Rc::from(hex::decode(hex)?);
            let (expr, known) = if matches!(expr, Expr::Atom { .. }) {
                (Expr::Atom { hex: hex.clone() }, Known::Atom(bytes))
            } else {
                (Expr::Bytes { hex: hex.clone() }, Known::Bytes(bytes))
            };
            return Ok(Reduced {
                expr,
                known: Some(Rc::new(known)),
                total: true,
            });
        }
        Expr::Arg { index } => (Expr::Arg { index: *index }, None, true),
        Expr::Local { local_id } => {
            let known = bindings
                .iter()
                .rev()
                .find(|binding| binding.id == *local_id)
                .and_then(|binding| binding.known.clone());
            (
                Expr::Local {
                    local_id: *local_id,
                },
                known,
                true,
            )
        }
        Expr::Reject { message } => (
            Expr::Reject {
                message: budget.text(message)?,
            },
            None,
            false,
        ),
        Expr::Let {
            local_id,
            ty,
            value,
            body,
        } => {
            let value = expression(value, bindings, budget)?;
            bindings.push(Binding {
                id: *local_id,
                known: value.known.clone(),
            });
            let body = expression(body, bindings, budget);
            bindings.pop();
            let body = body?;
            if value.total && !uses_local(&body.expr, *local_id, budget)? {
                return Ok(body);
            }
            let total = value.total && body.total;
            let known = if total { body.known } else { None };
            (
                Expr::Let {
                    local_id: *local_id,
                    ty: budget.ty(ty)?,
                    value: Box::new(value.expr),
                    body: Box::new(body.expr),
                },
                known,
                total,
            )
        }
        Expr::Sequence { first, then_value } => {
            let first = expression(first, bindings, budget)?;
            let then_value = expression(then_value, bindings, budget)?;
            if first.total {
                return Ok(then_value);
            }
            (
                Expr::Sequence {
                    first: Box::new(first.expr),
                    then_value: Box::new(then_value.expr),
                },
                None,
                false,
            )
        }
        Expr::If {
            condition,
            then_value,
            else_value,
        } => {
            let condition = expression(condition, bindings, budget)?;
            if let Some(value) = &condition.known
                && let Known::Bool(selected) = **value
            {
                // Whole-input validation already checked the dead branch.
                return expression(
                    if selected { then_value } else { else_value },
                    bindings,
                    budget,
                );
            }
            let then_value = expression(then_value, bindings, budget)?;
            let else_value = expression(else_value, bindings, budget)?;
            let total = condition.total && then_value.total && else_value.total;
            (
                Expr::If {
                    condition: Box::new(condition.expr),
                    then_value: Box::new(then_value.expr),
                    else_value: Box::new(else_value.expr),
                },
                None,
                total,
            )
        }
        Expr::Binary {
            kind,
            left,
            right,
            ty,
        } => {
            let left = expression(left, bindings, budget)?;
            let right = expression(right, bindings, budget)?;
            let known = match (&left.known, &right.known) {
                (Some(left), Some(right)) => binary(*kind, ty, left, right, budget)?,
                _ => None,
            };
            let operation_total = match kind {
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => known.is_some(),
                BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
                | BinaryOp::Bitand
                | BinaryOp::Bitor
                | BinaryOp::Bitxor => true,
                // These operations currently fail whole-input validation. Keep
                // their totality conservative if that accepted subset grows.
                BinaryOp::Div | BinaryOp::Rem | BinaryOp::Shl | BinaryOp::Shr => false,
            };
            let total = left.total && right.total && operation_total;
            (
                Expr::Binary {
                    kind: *kind,
                    left: Box::new(left.expr),
                    right: Box::new(right.expr),
                    ty: budget.ty(ty)?,
                },
                known,
                total,
            )
        }
        Expr::Not { value } => {
            let value = expression(value, bindings, budget)?;
            let known = match value.known.as_deref() {
                Some(Known::Bool(value)) => Some(Rc::new(Known::Bool(!value))),
                _ => None,
            };
            (
                Expr::Not {
                    value: Box::new(value.expr),
                },
                known,
                value.total,
            )
        }
        Expr::Tuple { items } => {
            let mut output = Vec::with_capacity(items.len());
            let mut values = Vec::with_capacity(items.len());
            let mut total = true;
            for item in items {
                let item = expression(item, bindings, budget)?;
                total &= item.total;
                if let Some(known) = item.known {
                    values.push(known);
                }
                output.push(item.expr);
            }
            let known = (values.len() == items.len()).then(|| Rc::new(Known::Tuple(values)));
            (Expr::Tuple { items: output }, known, total)
        }
        Expr::Field { value, index } => {
            let value = expression(value, bindings, budget)?;
            let known = match value.known.as_deref() {
                Some(Known::Tuple(items)) => items.get(*index).cloned(),
                Some(Known::Bytes(bytes)) => bytes.get(*index).map(|byte| {
                    Rc::new(Known::Int {
                        value: BigInt::from(*byte),
                        ty: Type::Int {
                            bits: 8,
                            signed: false,
                        },
                    })
                }),
                _ => None,
            };
            // Typed tuple/fixed-byte fields have valid indices and shapes.
            (
                Expr::Field {
                    value: Box::new(value.expr),
                    index: *index,
                },
                known,
                value.total,
            )
        }
        Expr::AtomIsEmpty { value } => {
            let value = expression(value, bindings, budget)?;
            let known = match value.known.as_deref() {
                Some(Known::Atom(bytes)) => Some(Rc::new(Known::Bool(bytes.is_empty()))),
                _ => None,
            };
            (
                Expr::AtomIsEmpty {
                    value: Box::new(value.expr),
                },
                known,
                value.total,
            )
        }
        Expr::TreeAtom { value } => {
            let value = expression(value, bindings, budget)?;
            let known = match value.known.as_deref() {
                Some(Known::Atom(bytes) | Known::Bytes(bytes)) => {
                    Some(Rc::new(Known::TreeAtom(bytes.clone())))
                }
                _ => None,
            };
            (
                Expr::TreeAtom {
                    value: Box::new(value.expr),
                },
                known,
                value.total,
            )
        }
        Expr::TreePair { first, rest } => {
            let first = expression(first, bindings, budget)?;
            let rest = expression(rest, bindings, budget)?;
            let total = first.total && rest.total;
            let known = match (&first.known, &rest.known) {
                (Some(first), Some(rest)) => {
                    Some(Rc::new(Known::TreePair(first.clone(), rest.clone())))
                }
                _ => None,
            };
            (
                Expr::TreePair {
                    first: Box::new(first.expr),
                    rest: Box::new(rest.expr),
                },
                known,
                total,
            )
        }
        Expr::TreeIsPair { value } => {
            let value = expression(value, bindings, budget)?;
            let known = match value.known.as_deref() {
                Some(Known::TreePair(_, _)) => Some(Rc::new(Known::Bool(true))),
                Some(Known::TreeAtom(_)) => Some(Rc::new(Known::Bool(false))),
                _ => None,
            };
            (
                Expr::TreeIsPair {
                    value: Box::new(value.expr),
                },
                known,
                value.total,
            )
        }
        Expr::TreeFirst { value } | Expr::TreeRest { value } | Expr::TreeAtomValue { value } => {
            let first = matches!(expr, Expr::TreeFirst { .. });
            let atom = matches!(expr, Expr::TreeAtomValue { .. });
            let value = expression(value, bindings, budget)?;
            let known = match value.known.as_deref() {
                Some(Known::TreePair(left, right)) if !atom => {
                    Some(if first { left.clone() } else { right.clone() })
                }
                Some(Known::TreeAtom(bytes)) if atom => Some(Rc::new(Known::Atom(bytes.clone()))),
                _ => None,
            };
            let total = value.total && known.is_some();
            let value = Box::new(value.expr);
            let expr = if atom {
                Expr::TreeAtomValue { value }
            } else if first {
                Expr::TreeFirst { value }
            } else {
                Expr::TreeRest { value }
            };
            (expr, known, total)
        }
        Expr::Call {
            function_id,
            arguments,
        } => {
            let mut output = Vec::with_capacity(arguments.len());
            for argument in arguments {
                output.push(expression(argument, bindings, budget)?.expr);
            }
            (
                Expr::Call {
                    function_id: *function_id,
                    arguments: output,
                },
                None,
                false,
            )
        }
        Expr::Sha256 { parts } => {
            let mut output = Vec::with_capacity(parts.len());
            for part in parts {
                output.push(expression(part, bindings, budget)?.expr);
            }
            // Hashing remains an evaluated effect, including with literal parts
            // and an unused result. Resource exhaustion is never a proof of
            // semantic rejection or an excuse to discard required hash work.
            (Expr::Sha256 { parts: output }, None, false)
        }
        Expr::Apply {
            program,
            environment,
        } => {
            let program = expression(program, bindings, budget)?.expr;
            let environment = expression(environment, bindings, budget)?.expr;
            // Application is an evaluated effect with an unknown result; it is
            // never folded, dropped or treated as total.
            (
                Expr::Apply {
                    program: Box::new(program),
                    environment: Box::new(environment),
                },
                None,
                false,
            )
        }
    };
    Reduced::finish(expr, known, total, budget)
}

fn uses_local(expr: &Expr, id: usize, budget: &mut Budget) -> Result<bool> {
    let mut pending = vec![expr];
    while let Some(expr) = pending.pop() {
        budget.work(1)?;
        match expr {
            Expr::Local { local_id } if *local_id == id => return Ok(true),
            Expr::Tuple { items }
            | Expr::Sha256 { parts: items }
            | Expr::Call {
                arguments: items, ..
            } => {
                budget.work(items.len())?;
                pending.extend(items);
            }
            Expr::Binary { left, right, .. }
            | Expr::Apply {
                program: left,
                environment: right,
            } => pending.extend([left.as_ref(), right.as_ref()]),
            Expr::Let { value, body, .. }
            | Expr::Sequence {
                first: value,
                then_value: body,
            } => pending.extend([value.as_ref(), body.as_ref()]),
            Expr::If {
                condition,
                then_value,
                else_value,
            } => pending.extend([condition.as_ref(), then_value.as_ref(), else_value.as_ref()]),
            Expr::TreePair { first, rest } | Expr::Pair { first, rest } => {
                pending.extend([first.as_ref(), rest.as_ref()])
            }
            Expr::Not { value }
            | Expr::AtomIsEmpty { value }
            | Expr::Field { value, .. }
            | Expr::TreeAtom { value }
            | Expr::TreeAtomValue { value }
            | Expr::TreeIsPair { value }
            | Expr::TreeFirst { value }
            | Expr::PairFirst { value }
            | Expr::PairRest { value }
            | Expr::TreeRest { value } => pending.push(value),
            Expr::Unit
            | Expr::Bool { .. }
            | Expr::Int { .. }
            | Expr::Bytes { .. }
            | Expr::Atom { .. }
            | Expr::Arg { .. }
            | Expr::Local { .. }
            | Expr::Reject { .. } => (),
        }
    }
    Ok(false)
}
