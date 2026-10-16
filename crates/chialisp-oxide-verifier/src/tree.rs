//! A separate structural IR model, not an extension of scalar arithmetic proofs.
use anyhow::{Context, Result, bail, ensure};
use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Program, Type};
use serde::{Deserialize, Serialize};

use crate::tree_witness::decode_hex;
use crate::{
    SExpression, SolverConfig, TreeValue, TreeWitness, parse_values, require_expression_model, run,
    status,
};

const SEQUENCE: &str = "(Seq (_ BitVec 8))";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeShape {
    Atom,
    Pair,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "term", rename_all = "snake_case", deny_unknown_fields)]
pub enum TreeTerm {
    Input,
    Atom {
        hex: String,
    },
    First {
        value: Box<TreeTerm>,
    },
    Rest {
        value: Box<TreeTerm>,
    },
    Pair {
        first: Box<TreeTerm>,
        rest: Box<TreeTerm>,
    },
}

/// Independently supplied acceptance shape and successful output tree. There
/// are no implicit integer interpretations or cryptographic assumptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeSpecification {
    pub acceptance: TreeShape,
    pub output: TreeTerm,
}

#[derive(Debug, Serialize)]
pub struct TreeQueries {
    pub atom_nonvacuity: String,
    pub atom_counterexample: String,
    pub pair_nonvacuity: String,
    pub pair_counterexample: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeCase {
    pub input_shape: TreeShape,
    /// A satisfiable domain witness; atom-case rejection is intentional.
    pub domain_nonvacuity: TreeWitness,
    pub counterexample: Option<TreeWitness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TreeVerification {
    Proven { cases: Vec<TreeCase> },
    Refuted { cases: Vec<TreeCase> },
}

enum Value {
    Tree(String),
    Atom(String),
    Bool(String),
}
impl Value {
    fn tree(&self) -> Result<&str> {
        if let Self::Tree(value) = self {
            Ok(value)
        } else {
            bail!("expected structural tree")
        }
    }
    fn atom(&self) -> Result<&str> {
        if let Self::Atom(value) = self {
            Ok(value)
        } else {
            bail!("expected structural byte atom")
        }
    }
    fn boolean(&self) -> Result<&str> {
        if let Self::Bool(value) = self {
            Ok(value)
        } else {
            bail!("expected structural boolean")
        }
    }
    fn sort(&self) -> &str {
        match self {
            Self::Tree(_) => "Tree",
            Self::Atom(_) => SEQUENCE,
            Self::Bool(_) => "Bool",
        }
    }
    fn term(&self) -> &str {
        match self {
            Self::Tree(value) | Self::Atom(value) | Self::Bool(value) => value,
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
}

fn sequence(hex: &str) -> Result<String> {
    let bytes = decode_hex(hex)?;
    ensure!(
        bytes.len() <= 4096,
        "structural literal exceeds IR atom bound"
    );
    Ok(match bytes.as_slice() {
        [] => format!("(as seq.empty {SEQUENCE})"),
        [byte] => format!("(seq.unit #x{byte:02x})"),
        _ => format!(
            "(seq.++ {})",
            bytes
                .iter()
                .map(|byte| format!("(seq.unit #x{byte:02x})"))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    })
}

impl Translation {
    fn define(&mut self, sort: &str, term: &str) -> String {
        let name = format!("tree_v_{}", self.serial);
        self.serial += 1;
        self.text
            .push_str(&format!("(define-fun {name} () {sort} {term})\n"));
        name
    }
    fn select(&mut self, condition: &str, a: Value, b: Value) -> Result<Value> {
        ensure!(a.sort() == b.sort(), "structural branch sort mismatch");
        let symbol = self.define(
            a.sort(),
            &format!("(ite {condition} {} {})", a.term(), b.term()),
        );
        Ok(match a {
            Value::Tree(_) => Value::Tree(symbol),
            Value::Atom(_) => Value::Atom(symbol),
            Value::Bool(_) => Value::Bool(symbol),
        })
    }
    fn expression(&mut self, expr: &Expr) -> Result<Evaluated> {
        let (success, value) = match expr {
            Expr::Call { .. } | Expr::Let { .. } | Expr::Local { .. } => {
                bail!("structural SMT does not model runtime functions or local bindings")
            }
            Expr::Arg { index: 0 } => ("true".into(), Some(Value::Tree("env".into()))),
            Expr::Atom { hex } | Expr::Bytes { hex } => {
                ("true".into(), Some(Value::Atom(sequence(hex)?)))
            }
            Expr::Bool { value } => ("true".into(), Some(Value::Bool(value.to_string()))),
            Expr::Reject { .. } => ("false".into(), None),
            Expr::TreeAtom { value } => {
                let value = self.expression(value)?;
                let atom = value.value.context("diverging atom construction")?;
                (
                    value.success,
                    Some(Value::Tree(
                        self.define("Tree", &format!("(atom {})", atom.atom()?)),
                    )),
                )
            }
            Expr::TreePair { first, rest } => {
                let first = self.expression(first)?;
                let rest = self.expression(rest)?;
                let a = first.value.context("diverging first construction")?;
                let b = rest.value.context("diverging rest construction")?;
                (
                    format!("(and {} {})", first.success, rest.success),
                    Some(Value::Tree(self.define(
                        "Tree",
                        &format!("(pair {} {})", a.tree()?, b.tree()?),
                    ))),
                )
            }
            Expr::TreeIsPair { value } => {
                let value = self.expression(value)?;
                let tree = value.value.context("diverging shape test")?;
                (
                    value.success,
                    Some(Value::Bool(
                        self.define("Bool", &format!("((_ is pair) {})", tree.tree()?)),
                    )),
                )
            }
            Expr::TreeFirst { value }
            | Expr::TreeRest { value }
            | Expr::TreeAtomValue { value } => {
                let value = self.expression(value)?;
                let tree = value.value.context("diverging tree projection")?;
                let (constructor, selector) = match expr {
                    Expr::TreeFirst { .. } => ("pair", "pair_first"),
                    Expr::TreeRest { .. } => ("pair", "pair_rest"),
                    _ => ("atom", "atom_bytes"),
                };
                let success = format!(
                    "(and {} ((_ is {constructor}) {}))",
                    value.success,
                    tree.tree()?
                );
                let term = format!("({selector} {})", tree.tree()?);
                let output = if constructor == "atom" {
                    Value::Atom(self.define(SEQUENCE, &term))
                } else {
                    Value::Tree(self.define("Tree", &term))
                };
                (success, Some(output))
            }
            Expr::Not { value } => {
                let value = self.expression(value)?;
                let boolean = value.value.context("diverging structural not")?;
                (
                    value.success,
                    Some(Value::Bool(
                        self.define("Bool", &format!("(not {})", boolean.boolean()?)),
                    )),
                )
            }
            Expr::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.expression(condition)?;
                let boolean = condition.value.context("diverging structural condition")?;
                let c = boolean.boolean()?;
                let a = self.expression(then_value)?;
                let b = self.expression(else_value)?;
                let success = format!(
                    "(and {} (ite {c} {} {}))",
                    condition.success, a.success, b.success
                );
                let value = match (a.value, b.value) {
                    (Some(a), Some(b)) => Some(self.select(c, a, b)?),
                    (a, b) => a.or(b),
                };
                (success, value)
            }
            Expr::Sequence { first, then_value } => {
                let first = self.expression(first)?;
                let then_value = self.expression(then_value)?;
                (
                    format!("(and {} {})", first.success, then_value.success),
                    then_value.value,
                )
            }
            Expr::Binary {
                kind: BinaryOp::Eq | BinaryOp::Ne,
                left,
                right,
                ..
            } => {
                let a = self.expression(left)?;
                let b = self.expression(right)?;
                let av = a.value.context("diverging structural equality")?;
                let bv = b.value.context("diverging structural equality")?;
                ensure!(
                    av.sort() == bv.sort() && av.sort() != "Tree",
                    "structural equality only models atom bytes and booleans"
                );
                let term = format!("(= {} {})", av.term(), bv.term());
                let term = if matches!(
                    expr,
                    Expr::Binary {
                        kind: BinaryOp::Ne,
                        ..
                    }
                ) {
                    format!("(not {term})")
                } else {
                    term
                };
                (
                    format!("(and {} {})", a.success, b.success),
                    Some(Value::Bool(self.define("Bool", &term))),
                )
            }
            _ => bail!("unsupported expression in structural tree model: {expr:?}"),
        };
        Ok(Evaluated {
            success: self.define("Bool", &success),
            value,
        })
    }
    fn specification(&mut self, term: &TreeTerm, depth: usize) -> Result<(String, String)> {
        ensure!(depth <= 64, "tree specification nesting limit");
        Ok(match term {
            TreeTerm::Input => ("true".into(), "env".into()),
            TreeTerm::Atom { hex } => ("true".into(), format!("(atom {})", sequence(hex)?)),
            TreeTerm::First { value } | TreeTerm::Rest { value } => {
                let (success, value) = self.specification(value, depth + 1)?;
                let selector = if matches!(term, TreeTerm::First { .. }) {
                    "pair_first"
                } else {
                    "pair_rest"
                };
                (
                    format!("(and {success} ((_ is pair) {value}))"),
                    format!("({selector} {value})"),
                )
            }
            TreeTerm::Pair { first, rest } => {
                let (a, first) = self.specification(first, depth + 1)?;
                let (b, rest) = self.specification(rest, depth + 1)?;
                (format!("(and {a} {b})"), format!("(pair {first} {rest})"))
            }
        })
    }
}

/// Model arbitrary finite trees/byte sequences, without VM cost or a proof that
/// checked Rust extraction, optimizer, serialization or ABI code is preserved.
pub fn emit_tree(program: &Program, specification: &TreeSpecification) -> Result<TreeQueries> {
    program.validate().context("invalid structural IR")?;
    require_expression_model(program, "structural SMT")?;
    ensure!(
        program.environment == Environment::Raw
            && program.parameters.len() == 1
            && program.parameters[0].ty == Type::Tree
            && program.result == Type::Tree,
        "structural checker requires one raw tree parameter and a tree result"
    );
    let mut tr = Translation::default();
    let execution = tr.expression(&program.body)?;
    let value = execution
        .value
        .unwrap_or(Value::Tree(format!("(atom {})", sequence("")?)));
    let output = value.tree()?;
    let (expected_success, expected_output) = tr.specification(&specification.output, 0)?;
    let base = format!(
        "; Structural IR model only: all finite trees/byte sequences, no VM cost/compiler-preservation proof.\n(set-logic ALL)\n(set-option :pp.min-alias-size 1000000)\n(declare-datatypes ((Tree 0)) (((atom (atom_bytes {SEQUENCE})) (pair (pair_first Tree) (pair_rest Tree)))))\n(declare-const env Tree)\n{}",
        tr.text
    );
    let case = |shape: TreeShape| {
        let name = if shape == TreeShape::Atom {
            "atom"
        } else {
            "pair"
        };
        let accepts = shape == specification.acceptance;
        let domain = format!("{base}(assert ((_ is {name}) env))\n");
        let mismatch = format!(
            "(or (not (= {} {accepts})) (and {} {accepts} (or (not {expected_success}) (not (= {output} {expected_output})))))",
            execution.success, execution.success
        );
        (
            format!("{domain}(check-sat)\n"),
            format!("{domain}(assert {mismatch})\n(check-sat)\n"),
        )
    };
    let (atom_nonvacuity, atom_counterexample) = case(TreeShape::Atom);
    let (pair_nonvacuity, pair_counterexample) = case(TreeShape::Pair);
    Ok(TreeQueries {
        atom_nonvacuity,
        atom_counterexample,
        pair_nonvacuity,
        pair_counterexample,
    })
}

/// Compare two actual structural expression trees using the symbolic execution
/// semantics, independently of any optimizer's syntactic side-condition test.
/// Rejection diagnostics and VM cost are outside this equivalence relation.
pub fn emit_tree_equivalence(before: &Program, after: &Program) -> Result<TreeQueries> {
    for program in [before, after] {
        program
            .validate()
            .context("invalid structural equivalence IR")?;
        require_expression_model(program, "structural SMT equivalence")?;
        ensure!(
            program.environment == Environment::Raw
                && program.parameters.len() == 1
                && program.parameters[0].ty == Type::Tree
                && program.result == Type::Tree,
            "structural equivalence requires one raw tree parameter and a tree result"
        );
    }
    let mut tr = Translation::default();
    let before = tr.expression(&before.body)?;
    let after = tr.expression(&after.body)?;
    let empty = format!("(atom {})", sequence("")?);
    let before_value = before.value.unwrap_or(Value::Tree(empty.clone()));
    let after_value = after.value.unwrap_or(Value::Tree(empty));
    let mismatch = format!(
        "(or (not (= {} {})) (and {} {} (not (= {} {}))))",
        before.success,
        after.success,
        before.success,
        after.success,
        before_value.tree()?,
        after_value.tree()?
    );
    let base = format!(
        "; Structural IR equivalence only: rejection/output equality, excluding diagnostics, cost and compiler preservation.\n(set-logic ALL)\n(set-option :pp.min-alias-size 1000000)\n(declare-datatypes ((Tree 0)) (((atom (atom_bytes {SEQUENCE})) (pair (pair_first Tree) (pair_rest Tree)))))\n(declare-const env Tree)\n{}",
        tr.text
    );
    let case = |shape: &str| {
        let domain = format!("{base}(assert ((_ is {shape}) env))\n");
        (
            format!("{domain}(check-sat)\n"),
            format!("{domain}(assert {mismatch})\n(check-sat)\n"),
        )
    };
    let (atom_nonvacuity, atom_counterexample) = case("atom");
    let (pair_nonvacuity, pair_counterexample) = case("pair");
    Ok(TreeQueries {
        atom_nonvacuity,
        atom_counterexample,
        pair_nonvacuity,
        pair_counterexample,
    })
}

fn byte_sequence_sort(value: &SExpression) -> bool {
    let SExpression::List(items) = value else {
        return false;
    };
    let [SExpression::Atom(sequence), SExpression::List(bits)] = items.as_slice() else {
        return false;
    };
    let [
        SExpression::Atom(underscore),
        SExpression::Atom(bitvec),
        SExpression::Atom(width),
    ] = bits.as_slice()
    else {
        return false;
    };
    sequence == "Seq" && underscore == "_" && bitvec == "BitVec" && width == "8"
}
fn model_sequence(value: &SExpression, depth: usize) -> Result<Vec<u8>> {
    ensure!(depth <= 128, "solver atom expression nesting limit");
    let SExpression::List(items) = value else {
        bail!("invalid solver byte sequence")
    };
    match items.as_slice() {
        [SExpression::Atom(as_kw), SExpression::Atom(empty), sort]
            if as_kw == "as" && empty == "seq.empty" && byte_sequence_sort(sort) =>
        {
            Ok(vec![])
        }
        [SExpression::Atom(unit), SExpression::Atom(byte)]
            if unit == "seq.unit" && byte.starts_with("#x") && byte.len() == 4 =>
        {
            Ok(vec![u8::from_str_radix(&byte[2..], 16)?])
        }
        [SExpression::Atom(unit), SExpression::Atom(byte)]
            if unit == "seq.unit" && byte.starts_with("#b") && byte.len() == 10 =>
        {
            Ok(vec![u8::from_str_radix(&byte[2..], 2)?])
        }
        [SExpression::Atom(concat), values @ ..] if concat == "seq.++" => {
            let mut bytes = vec![];
            for value in values {
                bytes.extend(model_sequence(value, depth + 1)?);
                ensure!(bytes.len() <= 1_048_576, "solver atom too large");
            }
            Ok(bytes)
        }
        _ => bail!("unsupported solver byte-sequence representation"),
    }
}
fn model_tree(value: &SExpression, depth: usize, budget: &mut usize) -> Result<TreeValue> {
    ensure!(
        depth <= 128 && *budget > 0,
        "solver tree nesting/node limit"
    );
    *budget -= 1;
    let SExpression::List(items) = value else {
        bail!("invalid solver tree")
    };
    match items.as_slice() {
        [SExpression::Atom(atom), bytes] if atom == "atom" => Ok(TreeValue::Atom {
            hex: model_sequence(bytes, 0)?
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        }),
        [SExpression::Atom(pair), first, rest] if pair == "pair" => Ok(TreeValue::Pair {
            first: Box::new(model_tree(first, depth + 1, budget)?),
            rest: Box::new(model_tree(rest, depth + 1, budget)?),
        }),
        _ => bail!("unsupported solver tree representation"),
    }
}
fn witness_term(value: &TreeValue) -> Result<String> {
    Ok(match value {
        TreeValue::Atom { hex } => format!("(atom {})", sequence(hex)?),
        TreeValue::Pair { first, rest } => {
            format!("(pair {} {})", witness_term(first)?, witness_term(rest)?)
        }
    })
}
fn solve_tree(config: &SolverConfig, query: &str) -> Result<Option<TreeWitness>> {
    let result = run(config, query, &[])?;
    ensure!(
        result.lines().count() == 1,
        "unexpected structural solver status output"
    );
    if !status(&result)? {
        return Ok(None);
    }
    // Stable, solver-checked candidate witnesses avoid depending on a solver's
    // arbitrary choice of unconstrained bytes. The general query above remains
    // unbounded; a candidate never replaces its SAT/UNSAT obligation.
    let atom = |hex: &str| TreeValue::Atom { hex: hex.into() };
    let candidates = [
        atom(""),
        atom("01"),
        TreeValue::Pair {
            first: Box::new(atom("")),
            rest: Box::new(atom("")),
        },
        TreeValue::Pair {
            first: Box::new(atom("00")),
            rest: Box::new(atom("01")),
        },
    ];
    let mut selected = query.to_owned();
    let prefix = query
        .strip_suffix("(check-sat)\n")
        .context("malformed generated structural query")?;
    for candidate in candidates {
        let constrained = format!(
            "{prefix}(assert (= env {}))\n(check-sat)\n",
            witness_term(&candidate)?
        );
        let result = run(config, &constrained, &[])?;
        ensure!(
            result.lines().count() == 1,
            "unexpected structural candidate status output"
        );
        if status(&result)? {
            selected = constrained;
            break;
        }
    }
    let result = run(config, &selected, &["env".into()])?;
    ensure!(
        status(&result)?,
        "solver changed structural satisfiability while obtaining witness"
    );
    let values = parse_values(
        result
            .split_once('\n')
            .context("missing structural solver model")?
            .1,
    )?;
    ensure!(values.len() == 1, "unexpected structural model symbols");
    Ok(Some(TreeWitness {
        environment: model_tree(
            values
                .get("env")
                .context("missing structural environment")?,
            0,
            &mut 20_000,
        )?,
    }))
}

/// Unknown, timeout, malformed output and unsupported operations are errors;
/// neither an empty domain nor a missing counterexample witness becomes proof.
pub fn verify_tree(
    program: &Program,
    specification: &TreeSpecification,
    config: &SolverConfig,
) -> Result<TreeVerification> {
    verify_tree_queries(emit_tree(program, specification)?, config)
}

/// Prove equivalence in the structural IR model, without asserting compiler or
/// optimizer implementation correctness for expressions beyond these inputs.
pub fn verify_tree_equivalence(
    before: &Program,
    after: &Program,
    config: &SolverConfig,
) -> Result<TreeVerification> {
    verify_tree_queries(emit_tree_equivalence(before, after)?, config)
}

fn verify_tree_queries(queries: TreeQueries, config: &SolverConfig) -> Result<TreeVerification> {
    let mut cases = vec![];
    for (input_shape, nonvacuity, counterexample) in [
        (
            TreeShape::Atom,
            queries.atom_nonvacuity,
            queries.atom_counterexample,
        ),
        (
            TreeShape::Pair,
            queries.pair_nonvacuity,
            queries.pair_counterexample,
        ),
    ] {
        cases.push(TreeCase {
            input_shape,
            domain_nonvacuity: solve_tree(config, &nonvacuity)?
                .context("structural input domain is empty; no proof produced")?,
            counterexample: solve_tree(config, &counterexample)?,
        });
    }
    Ok(if cases.iter().all(|case| case.counterexample.is_none()) {
        TreeVerification::Proven { cases }
    } else {
        TreeVerification::Refuted { cases }
    })
}

#[cfg(test)]
mod model_controls {
    use super::*;

    #[test]
    fn structural_model_parser_checks_sequence_sort_shape_and_nesting() {
        let valid = parse_values("((env (atom (as seq.empty (Seq (_ BitVec 8))))))").unwrap();
        assert_eq!(
            model_tree(&valid["env"], 0, &mut 20_000).unwrap(),
            TreeValue::Atom { hex: String::new() }
        );
        for text in [
            "((env (atom (as seq.empty Int))))",
            "((env (atom (seq.unit #x0000))))",
            "((env (pair (atom (seq.unit #x00)))))",
        ] {
            let values = parse_values(text).unwrap();
            assert!(model_tree(&values["env"], 0, &mut 20_000).is_err());
        }
        assert!(parse_values(&format!("((env {}0{}))", "(".repeat(300), ")".repeat(300))).is_err());
    }
}
