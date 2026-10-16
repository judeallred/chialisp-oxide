//! IR-derived SMT obligations, not a proof of rustc extraction or CLVM lowering.
//!
//! The domain is well-typed scalar/tuple inputs, after successful ABI decoding.
//! Byte values and byte projections are rejected, including in unreachable code.
//! Rejection and checked arithmetic are modeled explicitly; `If` is lazy while
//! tuples, binary operands, projections, and `Sequence` evaluate strictly.

mod replay;
mod tree;
mod tree_witness;
pub use replay::{Replay, ReplayFailure, ReplayOutcome, classify_replay_failure, replay};
pub use tree::{
    TreeCase, TreeQueries, TreeShape, TreeSpecification, TreeTerm, TreeVerification, emit_tree,
    emit_tree_equivalence, verify_tree, verify_tree_equivalence,
};
pub use tree_witness::{TreeValue, TreeWitness, replay_tree};

use anyhow::{Context, Result, bail, ensure};
use chialisp_oxide_ir::{BinaryOp, Expr, Program, Type};
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// A separately authored mathematical specification. Arithmetic here is
/// unbounded: the program's overflow checks belong to its translated semantics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "term", rename_all = "snake_case")]
pub enum Term {
    Input { argument: usize, fields: Vec<usize> },
    Output { fields: Vec<usize> },
    Int { value: String },
    Bool { value: bool },
    Add { left: Box<Term>, right: Box<Term> },
    Sub { left: Box<Term>, right: Box<Term> },
    Mul { left: Box<Term>, right: Box<Term> },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "predicate", rename_all = "snake_case")]
pub enum Predicate {
    Constant {
        value: bool,
    },
    Compare {
        relation: Comparison,
        left: Term,
        right: Term,
    },
    All {
        items: Vec<Predicate>,
    },
    Any {
        items: Vec<Predicate>,
    },
    Not {
        value: Box<Predicate>,
    },
}

impl Default for Predicate {
    fn default() -> Self {
        Self::Constant { value: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "obligation", rename_all = "snake_case")]
pub enum Obligation {
    /// Every successful execution satisfies the independently supplied property.
    OnSuccess { property: Predicate },
    /// Acceptance exactly matches a predicate of inputs. Output terms are
    /// forbidden because a rejected execution has no observable output.
    AcceptanceIff { predicate: Predicate },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Specification {
    #[serde(default)]
    pub assumptions: Predicate,
    pub obligation: Obligation,
}

/// Canonical values which can be encoded and replayed through CLVM/native Rust.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WitnessValue {
    Int { value: String },
    Bool { value: bool },
    Tuple { items: Vec<WitnessValue> },
    Unit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Witness {
    pub arguments: Vec<WitnessValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Verification {
    /// The assumptions admit a successful execution, and no counterexample
    /// exists in this IR model. This is not an end-to-end compiler proof.
    Proven { nonvacuity: Witness },
    Refuted {
        counterexample: Witness,
        nonvacuity: Option<Witness>,
    },
    /// No successful input satisfies the assumptions. Never reported as proven.
    Vacuous,
}

pub struct SolverConfig {
    pub executable: PathBuf,
    pub timeout: Duration,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            executable: "z3".into(),
            timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug)]
pub struct Queries {
    pub nonvacuity: String,
    pub counterexample: String,
    parameters: Vec<Type>,
    symbols: Vec<String>,
}

#[derive(Clone)]
enum Value {
    Int(String),
    Bool(String),
    Tuple(Vec<Value>),
    Unit,
}

impl Value {
    fn scalar(&self) -> Result<(&str, bool)> {
        match self {
            Self::Int(v) => Ok((v, false)),
            Self::Bool(v) => Ok((v, true)),
            _ => bail!("specification terms require an integer or boolean scalar"),
        }
    }
    fn boolean(&self) -> Result<&str> {
        let (term, boolean) = self.scalar()?;
        ensure!(boolean, "expected boolean");
        Ok(term)
    }
    fn integer(&self) -> Result<&str> {
        let (term, boolean) = self.scalar()?;
        ensure!(!boolean, "expected integer");
        Ok(term)
    }
    fn field(&self, path: &[usize]) -> Result<Self> {
        if let Some((index, rest)) = path.split_first() {
            let Self::Tuple(items) = self else {
                bail!("field projection requires a tuple")
            };
            items
                .get(*index)
                .context("invalid specification field")?
                .field(rest)
        } else {
            Ok(self.clone())
        }
    }
}

struct Evaluated {
    success: String,
    value: Option<Value>,
}

#[derive(Default)]
struct Translation {
    text: String,
    serial: usize,
    arguments: Vec<Value>,
    symbols: Vec<String>,
}

fn numeral(value: &str) -> Result<String> {
    let value: BigInt = value.parse().context("invalid decimal integer")?;
    Ok(if value < BigInt::from(0) {
        format!("(- {})", -value)
    } else {
        value.to_string()
    })
}

fn bounds(ty: &Type) -> Result<(String, String)> {
    let Type::Int { bits, signed } = ty else {
        bail!("expected integer type")
    };
    ensure!(
        [8, 16, 32, 64, 128].contains(bits),
        "unsupported integer width"
    );
    let magnitude = BigInt::from(1) << usize::from(bits - u16::from(*signed));
    let minimum = if *signed {
        -&magnitude
    } else {
        BigInt::from(0)
    };
    Ok((
        numeral(&minimum.to_string())?,
        numeral(&(magnitude - 1u8).to_string())?,
    ))
}

fn conjunction(items: &[String]) -> String {
    if items.is_empty() {
        "true".into()
    } else {
        format!("(and {})", items.join(" "))
    }
}

impl Translation {
    fn define(&mut self, sort: &str, expression: &str) -> String {
        let symbol = format!("v_{}", self.serial);
        self.serial += 1;
        self.text
            .push_str(&format!("(define-fun {symbol} () {sort} {expression})\n"));
        symbol
    }
    fn input(&mut self, ty: &Type, name: &str) -> Result<Value> {
        Ok(match ty {
            Type::Int { .. } => {
                let (min, max) = bounds(ty)?;
                self.text.push_str(&format!("(declare-const {name} Int)\n(assert (and (<= {min} {name}) (<= {name} {max})))\n"));
                self.symbols.push(name.into());
                Value::Int(name.into())
            }
            Type::Bool => {
                self.text
                    .push_str(&format!("(declare-const {name} Bool)\n"));
                self.symbols.push(name.into());
                Value::Bool(name.into())
            }
            Type::Tuple { items } => Value::Tuple(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, ty)| self.input(ty, &format!("{name}_{i}")))
                    .collect::<Result<_>>()?,
            ),
            Type::Unit => Value::Unit,
            Type::Bytes { .. } => bail!("SMT byte semantics are not implemented"),
            Type::Tree | Type::Atom => bail!(
                "scalar SMT does not model raw trees/atoms; use the separate structural checker"
            ),
        })
    }
    fn select(&mut self, condition: &str, a: Value, b: Value) -> Result<Value> {
        Ok(match (a, b) {
            (Value::Int(a), Value::Int(b)) => {
                Value::Int(self.define("Int", &format!("(ite {condition} {a} {b})")))
            }
            (Value::Bool(a), Value::Bool(b)) => {
                Value::Bool(self.define("Bool", &format!("(ite {condition} {a} {b})")))
            }
            (Value::Tuple(a), Value::Tuple(b)) => {
                ensure!(a.len() == b.len(), "branch tuple arity mismatch");
                Value::Tuple(
                    a.into_iter()
                        .zip(b)
                        .map(|(a, b)| self.select(condition, a, b))
                        .collect::<Result<_>>()?,
                )
            }
            (Value::Unit, Value::Unit) => Value::Unit,
            _ => bail!("branch sort mismatch"),
        })
    }
    fn expression(&mut self, expr: &Expr) -> Result<Evaluated> {
        let (success, value) = match expr {
            Expr::Unit => ("true".into(), Some(Value::Unit)),
            Expr::Bool { value } => ("true".into(), Some(Value::Bool(value.to_string()))),
            Expr::Int { value, .. } => ("true".into(), Some(Value::Int(numeral(value)?))),
            Expr::Bytes { .. } => bail!("SMT byte semantics are not implemented"),
            Expr::Atom { .. }
            | Expr::TreeAtom { .. }
            | Expr::TreeAtomValue { .. }
            | Expr::TreePair { .. }
            | Expr::TreeIsPair { .. }
            | Expr::TreeFirst { .. }
            | Expr::TreeRest { .. } => bail!(
                "scalar SMT does not model tree expressions; use the separate structural checker"
            ),
            Expr::Arg { index } => (
                "true".into(),
                Some(
                    self.arguments
                        .get(*index)
                        .context("invalid argument")?
                        .clone(),
                ),
            ),
            Expr::Reject { .. } => ("false".into(), None),
            Expr::Tuple { items } => {
                let values = items
                    .iter()
                    .map(|item| self.expression(item))
                    .collect::<Result<Vec<_>>>()?;
                let success =
                    conjunction(&values.iter().map(|v| v.success.clone()).collect::<Vec<_>>());
                let fields = values
                    .into_iter()
                    .map(|v| v.value.context("diverging tuple field"))
                    .collect::<Result<_>>()?;
                (success, Some(Value::Tuple(fields)))
            }
            Expr::Field { value, index } => {
                let value = self.expression(value)?;
                (
                    value.success,
                    Some(
                        value
                            .value
                            .context("diverging projection")?
                            .field(&[*index])?,
                    ),
                )
            }
            Expr::Sequence { first, then_value } => {
                let first = self.expression(first)?;
                let then_value = self.expression(then_value)?;
                (
                    format!("(and {} {})", first.success, then_value.success),
                    then_value.value,
                )
            }
            Expr::Not { value } => {
                let value = self.expression(value)?;
                let term = format!("(not {})", value.value.context("diverging not")?.boolean()?);
                (value.success, Some(Value::Bool(self.define("Bool", &term))))
            }
            Expr::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.expression(condition)?;
                let c = condition
                    .value
                    .context("diverging condition")?
                    .boolean()?
                    .to_owned();
                let a = self.expression(then_value)?;
                let b = self.expression(else_value)?;
                let success = format!(
                    "(and {} (ite {c} {} {}))",
                    condition.success, a.success, b.success
                );
                let value = match (a.value, b.value) {
                    (Some(a), Some(b)) => Some(self.select(&c, a, b)?),
                    (a, b) => a.or(b),
                };
                (success, value)
            }
            Expr::Binary {
                kind,
                left,
                right,
                ty,
            } => {
                let a = self.expression(left)?;
                let b = self.expression(right)?;
                let av = a.value.context("diverging left operand")?;
                let bv = b.value.context("diverging right operand")?;
                let (l, lb) = av.scalar()?;
                let (r, rb) = bv.scalar()?;
                ensure!(lb == rb, "binary operand sort mismatch");
                let op = match kind {
                    BinaryOp::Eq | BinaryOp::Ne => "=",
                    BinaryOp::Lt => "<",
                    BinaryOp::Le => "<=",
                    BinaryOp::Gt => ">",
                    BinaryOp::Ge => ">=",
                    BinaryOp::Add => "+",
                    BinaryOp::Sub => "-",
                    BinaryOp::Mul => "*",
                    _ => bail!("unsupported SMT binary operation {kind:?}"),
                };
                let mut term = format!("({op} {l} {r})");
                if *kind == BinaryOp::Ne {
                    term = format!("(not {term})");
                }
                let arithmetic = matches!(kind, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul);
                let mut success = format!("(and {} {})", a.success, b.success);
                let value = if arithmetic {
                    ensure!(!lb, "arithmetic requires integers");
                    let symbol = self.define("Int", &term);
                    let (min, max) = bounds(ty)?;
                    success = format!("(and {success} (<= {min} {symbol}) (<= {symbol} {max}))");
                    Value::Int(symbol)
                } else {
                    Value::Bool(self.define("Bool", &term))
                };
                (success, Some(value))
            }
        };
        Ok(Evaluated {
            success: self.define("Bool", &success),
            value,
        })
    }
    fn term(&mut self, term: &Term, output: Option<&Value>) -> Result<Value> {
        Ok(match term {
            Term::Input { argument, fields } => self
                .arguments
                .get(*argument)
                .context("invalid specification argument")?
                .field(fields)?,
            Term::Output { fields } => output
                .context("output terms are not allowed in assumptions or acceptance predicates")?
                .field(fields)?,
            Term::Int { value } => Value::Int(numeral(value)?),
            Term::Bool { value } => Value::Bool(value.to_string()),
            Term::Add { left, right } | Term::Sub { left, right } | Term::Mul { left, right } => {
                let a = self.term(left, output)?;
                let b = self.term(right, output)?;
                let op = match term {
                    Term::Add { .. } => "+",
                    Term::Sub { .. } => "-",
                    _ => "*",
                };
                Value::Int(self.define("Int", &format!("({op} {} {})", a.integer()?, b.integer()?)))
            }
        })
    }
    fn predicate(&mut self, predicate: &Predicate, output: Option<&Value>) -> Result<String> {
        Ok(match predicate {
            Predicate::Constant { value } => value.to_string(),
            Predicate::Compare {
                relation,
                left,
                right,
            } => {
                let a = self.term(left, output)?;
                let b = self.term(right, output)?;
                let (l, lb) = a.scalar()?;
                let (r, rb) = b.scalar()?;
                ensure!(lb == rb, "specification sort mismatch");
                let op = match relation {
                    Comparison::Eq | Comparison::Ne => "=",
                    Comparison::Lt => "<",
                    Comparison::Le => "<=",
                    Comparison::Gt => ">",
                    Comparison::Ge => ">=",
                };
                ensure!(
                    !lb || matches!(relation, Comparison::Eq | Comparison::Ne),
                    "boolean ordering is unsupported"
                );
                let term = format!("({op} {l} {r})");
                if matches!(relation, Comparison::Ne) {
                    format!("(not {term})")
                } else {
                    term
                }
            }
            Predicate::All { items } | Predicate::Any { items } => {
                let terms = items
                    .iter()
                    .map(|p| self.predicate(p, output))
                    .collect::<Result<Vec<_>>>()?;
                if matches!(predicate, Predicate::All { .. }) {
                    conjunction(&terms)
                } else if terms.is_empty() {
                    "false".into()
                } else {
                    format!("(or {})", terms.join(" "))
                }
            }
            Predicate::Not { value } => format!("(not {})", self.predicate(value, output)?),
        })
    }
}

fn default_value(ty: &Type) -> Result<Value> {
    Ok(match ty {
        Type::Int { .. } => Value::Int("0".into()),
        Type::Bool => Value::Bool("false".into()),
        Type::Tuple { items } => {
            Value::Tuple(items.iter().map(default_value).collect::<Result<_>>()?)
        }
        Type::Unit => Value::Unit,
        Type::Bytes { .. } => bail!("SMT byte semantics are not implemented"),
        Type::Tree | Type::Atom => {
            bail!("scalar SMT does not model raw trees/atoms; use the separate structural checker")
        }
    })
}

/// Generate standalone SMT-LIB2 queries from the same typed IR as the backend.
/// No puzzle logic is copied into a separate handwritten SMT model.
pub fn emit(program: &Program, specification: &Specification) -> Result<Queries> {
    program.validate().context("invalid IR")?;
    let mut tr = Translation::default();
    for (index, parameter) in program.parameters.iter().enumerate() {
        let argument = tr.input(&parameter.ty, &format!("arg_{index}"))?;
        tr.arguments.push(argument);
    }
    // Validate supported result types even if every execution rejects.
    let fallback = default_value(&program.result)?;
    let execution = tr.expression(&program.body)?;
    let output = execution.value.unwrap_or(fallback);
    let assumptions = tr.predicate(&specification.assumptions, None)?;
    let counterexample = match &specification.obligation {
        Obligation::OnSuccess { property } => format!(
            "(and {} (not {}))",
            execution.success,
            tr.predicate(property, Some(&output))?
        ),
        Obligation::AcceptanceIff { predicate } => format!(
            "(not (= {} {}))",
            execution.success,
            tr.predicate(predicate, None)?
        ),
    };
    let base = format!(
        "; IR-model only; typed canonical input domain, no byte/ABI/compiler proof.\n(set-logic QF_NIA)\n{}(assert {assumptions})\n",
        tr.text
    );
    Ok(Queries {
        nonvacuity: format!("{base}(assert {})\n(check-sat)\n", execution.success),
        counterexample: format!("{base}(assert {counterexample})\n(check-sat)\n"),
        parameters: program.parameters.iter().map(|p| p.ty.clone()).collect(),
        symbols: tr.symbols,
    })
}

fn run(config: &SolverConfig, query: &str, values: &[String]) -> Result<String> {
    ensure!(!config.timeout.is_zero(), "solver timeout must be positive");
    let millis = config.timeout.as_millis().min(u128::from(u32::MAX));
    let mut child = Command::new(&config.executable)
        .args(["-in", "-smt2"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("start solver {}", config.executable.display()))?;
    let mut script = format!("(set-option :timeout {millis})\n{query}");
    if !values.is_empty() {
        script.push_str(&format!("(get-value ({}))\n", values.join(" ")));
    }
    // Write from another thread: even an unresponsive solver cannot indefinitely
    // block the timeout monitor on a full stdin pipe.
    let mut stdin = child.stdin.take().context("solver stdin missing")?;
    let writer = std::thread::spawn(move || stdin.write_all(script.as_bytes()));
    let deadline = Instant::now() + config.timeout + Duration::from_millis(1000);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = writer.join();
            bail!("solver wall-clock timeout; no proof produced");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    writer
        .join()
        .map_err(|_| anyhow::anyhow!("solver input thread failed"))??;
    let result = child.wait_with_output()?;
    let stdout = String::from_utf8(result.stdout)?;
    let stderr = String::from_utf8(result.stderr)?;
    ensure!(
        result.status.success() && stderr.is_empty() && !stdout.contains("(error"),
        "solver error: {stdout}\n{stderr}"
    );
    Ok(stdout)
}

fn status(stdout: &str) -> Result<bool> {
    match stdout.lines().next() {
        Some("sat") => Ok(true),
        Some("unsat") => Ok(false),
        _ => bail!("solver returned unknown or unexpected output; no proof produced: {stdout}"),
    }
}

#[derive(Debug)]
enum SExpression {
    Atom(String),
    List(Vec<SExpression>),
}

fn parse_values(text: &str) -> Result<BTreeMap<String, SExpression>> {
    ensure!(text.len() <= 2_097_152, "solver model text too large");
    fn parse(tokens: &[String], index: &mut usize, depth: usize) -> Result<SExpression> {
        ensure!(depth <= 256, "solver model nesting limit");
        let token = tokens.get(*index).context("truncated solver value")?;
        *index += 1;
        if token == "(" {
            let mut items = vec![];
            while tokens.get(*index).is_some_and(|t| t != ")") {
                items.push(parse(tokens, index, depth + 1)?);
            }
            ensure!(
                tokens.get(*index).is_some_and(|t| t == ")"),
                "missing solver closing parenthesis"
            );
            *index += 1;
            Ok(SExpression::List(items))
        } else {
            ensure!(token != ")", "unexpected solver closing parenthesis");
            Ok(SExpression::Atom(token.clone()))
        }
    }
    let tokens = text
        .replace('(', " ( ")
        .replace(')', " ) ")
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ensure!(tokens.len() <= 100_000, "solver model token limit");
    let mut index = 0;
    let SExpression::List(values) = parse(&tokens, &mut index, 0)? else {
        bail!("expected solver value list")
    };
    ensure!(index == tokens.len(), "unexpected trailing solver values");
    let mut result = BTreeMap::new();
    for value in values {
        let SExpression::List(mut pair) = value else {
            bail!("expected solver value pair")
        };
        ensure!(pair.len() == 2, "invalid solver value pair");
        let value = pair.pop().context("missing solver value")?;
        let Some(SExpression::Atom(name)) = pair.pop() else {
            bail!("invalid solver symbol")
        };
        ensure!(
            result.insert(name, value).is_none(),
            "duplicate solver symbol"
        );
    }
    Ok(result)
}

fn witness_value(
    ty: &Type,
    name: &str,
    values: &BTreeMap<String, SExpression>,
) -> Result<WitnessValue> {
    Ok(match ty {
        Type::Int { .. } => {
            let value = match values.get(name).context("missing integer witness")? {
                SExpression::Atom(n) => n.parse::<BigInt>()?,
                SExpression::List(items) => {
                    let [SExpression::Atom(op), SExpression::Atom(n)] = items.as_slice() else {
                        bail!("invalid signed witness")
                    };
                    ensure!(op == "-", "invalid signed witness operator");
                    -n.parse::<BigInt>()?
                }
            };
            let (min, max) = bounds(ty)?;
            // Reparse bounds independently of SMT syntax.
            let parse_bound = |s: String| -> Result<BigInt> {
                if let Some(n) = s.strip_prefix("(- ").and_then(|v| v.strip_suffix(')')) {
                    Ok(-n.parse::<BigInt>()?)
                } else {
                    Ok(s.parse()?)
                }
            };
            ensure!(
                value >= parse_bound(min)? && value <= parse_bound(max)?,
                "solver witness out of input bounds"
            );
            WitnessValue::Int {
                value: value.to_string(),
            }
        }
        Type::Bool => {
            let Some(SExpression::Atom(value)) = values.get(name) else {
                bail!("missing boolean witness")
            };
            ensure!(
                value == "true" || value == "false",
                "invalid boolean witness"
            );
            WitnessValue::Bool {
                value: value == "true",
            }
        }
        Type::Tuple { items } => WitnessValue::Tuple {
            items: items
                .iter()
                .enumerate()
                .map(|(i, ty)| witness_value(ty, &format!("{name}_{i}"), values))
                .collect::<Result<_>>()?,
        },
        Type::Unit => WitnessValue::Unit,
        Type::Bytes { .. } => bail!("SMT byte semantics are not implemented"),
        Type::Tree | Type::Atom => {
            bail!("scalar SMT does not model raw trees/atoms; use the separate structural checker")
        }
    })
}

fn solve(config: &SolverConfig, queries: &Queries, query: &str) -> Result<Option<Witness>> {
    let output = run(config, query, &[])?;
    ensure!(
        output.lines().count() == 1,
        "unexpected solver status output: {output}"
    );
    if !status(&output)? {
        return Ok(None);
    }
    let values = if queries.symbols.is_empty() {
        BTreeMap::new()
    } else {
        let output = run(config, query, &queries.symbols)?;
        ensure!(
            status(&output)?,
            "solver changed satisfiability while obtaining witness"
        );
        parse_values(output.split_once('\n').context("missing solver model")?.1)?
    };
    Ok(Some(Witness {
        arguments: queries
            .parameters
            .iter()
            .enumerate()
            .map(|(i, ty)| witness_value(ty, &format!("arg_{i}"), &values))
            .collect::<Result<_>>()?,
    }))
}

/// Fail closed on solver absence, unknown, malformed output, or timeout.
pub fn verify(
    program: &Program,
    specification: &Specification,
    config: &SolverConfig,
) -> Result<Verification> {
    let queries = emit(program, specification)?;
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

#[cfg(test)]
mod solver_output_tests {
    use super::*;
    #[test]
    fn unknown_and_garbage_are_errors() {
        assert!(status("unknown\n").is_err());
        assert!(status("(error \"broken\")\n").is_err());
        assert!(parse_values("((arg_0 1)) extra").is_err());
        assert!(parse_values("((arg_0 1) (arg_0 2))").is_err());
    }
}
