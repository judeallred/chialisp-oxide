//! Structural Boolean programs over complete attacker-controlled environments.
//! The ABI decoder and expression evaluator are model semantics, not a proof
//! that the compiler or VM preserves them. No runtime functions are modeled.
use anyhow::{Context, Result, bail, ensure};
use chialisp_oxide_ir::{Environment, Program, Type};
use serde::{Deserialize, Serialize};

use crate::tree::{SEQUENCE, Translation, Value, sequence, solve_tree};
use crate::{Replay, ReplayOutcome, SolverConfig, TreeTerm, TreeWitness, require_expression_model};

/// Independently authored predicates over the complete environment. Projection
/// and byte-operation definedness survives Not/All/Any: an undefined acceptance
/// rejects; an undefined output on an accepted input is a counterexample.
/// All and Any require every child to be defined; they are not short-circuit
/// source-language evaluation constructs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "predicate", rename_all = "snake_case", deny_unknown_fields)]
pub enum TreePredicate {
    Constant { value: bool },
    IsAtom { value: TreeTerm },
    IsPair { value: TreeTerm },
    ByteEqual { left: TreeTerm, right: TreeTerm },
    ByteLength { value: TreeTerm, len: usize },
    Not { value: Box<TreePredicate> },
    All { items: Vec<TreePredicate> },
    Any { items: Vec<TreePredicate> },
}

/// The specification never derives its acceptance from Program.parameters.
/// Its Boolean output is encoded as the canonical tree atom nil or 01.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeBooleanSpecification {
    pub acceptance: TreePredicate,
    pub output: TreePredicate,
}

#[derive(Debug, Serialize)]
pub struct StructuralBooleanQueries {
    pub abi_nonvacuity: String,
    pub acceptance_nonvacuity: String,
    pub true_nonvacuity: String,
    pub false_nonvacuity: String,
    pub counterexample: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum StructuralBooleanVerification {
    Proven {
        abi_nonvacuity: TreeWitness,
        acceptance_nonvacuity: TreeWitness,
        true_witness: Option<TreeWitness>,
        false_witness: Option<TreeWitness>,
    },
    Refuted {
        abi_nonvacuity: TreeWitness,
        acceptance_nonvacuity: TreeWitness,
        true_witness: Option<TreeWitness>,
        false_witness: Option<TreeWitness>,
        counterexample: TreeWitness,
    },
}

fn validate_program(program: &Program) -> Result<()> {
    program
        .validate()
        .context("invalid structural Boolean IR")?;
    require_expression_model(program, "structural Boolean SMT")?;
    ensure!(
        program.result == Type::Bool,
        "structural Boolean checker requires a Bool result"
    );
    match program.environment {
        Environment::Strict => ensure!(
            program
                .parameters
                .iter()
                .all(|p| matches!(p.ty, Type::Atom | Type::Bytes { .. } | Type::Bool)),
            "strict structural Boolean parameters support only flat Atom, Bytes and Bool"
        ),
        Environment::Raw => ensure!(
            program.parameters.len() == 1 && program.parameters[0].ty == Type::Tree,
            "raw structural Boolean checker requires one Tree parameter"
        ),
        Environment::Ignored => {
            bail!("structural Boolean checker does not model ignored environments")
        }
    }
    Ok(())
}

/// Bound the complete specification before recursive translation, cloning or
/// formula construction. Literal work is separate from structural node work.
fn validate_specification(spec: &TreeBooleanSpecification) -> Result<()> {
    enum Node<'a> {
        Predicate(&'a TreePredicate),
        Term(&'a TreeTerm),
    }
    let mut pending = vec![
        (Node::Predicate(&spec.acceptance), 0),
        (Node::Predicate(&spec.output), 0),
    ];
    let mut work = 0usize;
    let mut literal_bytes = 0usize;
    while let Some((node, depth)) = pending.pop() {
        work += 1;
        ensure!(
            work <= 8192 && depth <= 64,
            "structural Boolean specification exceeds 8,192 nodes or depth 64"
        );
        let children = match &node {
            Node::Predicate(TreePredicate::Constant { .. })
            | Node::Term(TreeTerm::Input | TreeTerm::Atom { .. }) => 0,
            Node::Predicate(TreePredicate::All { items } | TreePredicate::Any { items }) => {
                items.len()
            }
            Node::Predicate(TreePredicate::ByteEqual { .. })
            | Node::Term(TreeTerm::Pair { .. }) => 2,
            _ => 1,
        };
        ensure!(
            work.saturating_add(pending.len()).saturating_add(children) <= 8192,
            "structural Boolean pending specification work exceeds budget"
        );
        let mut push = |node| pending.push((node, depth + 1));
        match node {
            Node::Predicate(TreePredicate::Constant { .. }) | Node::Term(TreeTerm::Input) => (),
            Node::Predicate(
                TreePredicate::IsAtom { value }
                | TreePredicate::IsPair { value }
                | TreePredicate::ByteLength { value, .. },
            ) => push(Node::Term(value)),
            Node::Predicate(TreePredicate::ByteEqual { left, right }) => {
                push(Node::Term(left));
                push(Node::Term(right));
            }
            Node::Predicate(TreePredicate::Not { value }) => push(Node::Predicate(value)),
            Node::Predicate(TreePredicate::All { items } | TreePredicate::Any { items }) => {
                ensure!(
                    items.len() <= 8192usize.saturating_sub(work),
                    "structural Boolean predicate width exceeds budget"
                );
                for item in items {
                    push(Node::Predicate(item));
                }
            }
            Node::Term(TreeTerm::Atom { hex }) => {
                ensure!(
                    hex.len() <= 8192,
                    "structural Boolean literal exceeds 4,096 bytes"
                );
                literal_bytes = literal_bytes.saturating_add(hex.len() / 2);
                ensure!(
                    literal_bytes <= 1_048_576,
                    "structural Boolean literal byte budget exceeded"
                );
                // Validate syntax now, before any SMT formatting.
                crate::tree_witness::decode_hex(hex)?;
            }
            Node::Term(TreeTerm::First { value } | TreeTerm::Rest { value }) => {
                push(Node::Term(value))
            }
            Node::Term(TreeTerm::Pair { first, rest }) => {
                push(Node::Term(first));
                push(Node::Term(rest));
            }
        }
        ensure!(
            pending.len() <= 8192,
            "structural Boolean pending specification work exceeds budget"
        );
    }
    Ok(())
}

struct PredicateValue {
    success: String,
    value: String,
}

fn all(items: &[String]) -> String {
    match items {
        [] => "true".into(),
        _ => format!("(and {})", items.join(" ")),
    }
}

fn term(tr: &mut Translation, source: &TreeTerm) -> Result<PredicateValue> {
    let (success, value) = match source {
        TreeTerm::Input => ("true".into(), "env".into()),
        TreeTerm::Atom { hex } => ("true".into(), format!("(atom {})", sequence(hex)?)),
        TreeTerm::First { value } | TreeTerm::Rest { value } => {
            let child = term(tr, value)?;
            let selector = if matches!(source, TreeTerm::First { .. }) {
                "pair_first"
            } else {
                "pair_rest"
            };
            (
                all(&[child.success, format!("((_ is pair) {})", child.value)]),
                format!("({selector} {})", child.value),
            )
        }
        TreeTerm::Pair { first, rest } => {
            let first = term(tr, first)?;
            let rest = term(tr, rest)?;
            (
                all(&[first.success, rest.success]),
                format!("(pair {} {})", first.value, rest.value),
            )
        }
    };
    Ok(PredicateValue {
        success: tr.define("Bool", &success),
        value: tr.define("Tree", &value),
    })
}

fn predicate(tr: &mut Translation, source: &TreePredicate) -> Result<PredicateValue> {
    let (success, value) = match source {
        TreePredicate::Constant { value } => ("true".into(), value.to_string()),
        TreePredicate::IsAtom { value } | TreePredicate::IsPair { value } => {
            let tree = term(tr, value)?;
            let constructor = if matches!(source, TreePredicate::IsAtom { .. }) {
                "atom"
            } else {
                "pair"
            };
            (
                tree.success,
                format!("((_ is {constructor}) {})", tree.value),
            )
        }
        TreePredicate::ByteEqual { left, right } => {
            let a = term(tr, left)?;
            let b = term(tr, right)?;
            (
                all(&[
                    a.success,
                    b.success,
                    format!("((_ is atom) {})", a.value),
                    format!("((_ is atom) {})", b.value),
                ]),
                format!("(= (atom_bytes {}) (atom_bytes {}))", a.value, b.value),
            )
        }
        TreePredicate::ByteLength { value, len } => {
            let a = term(tr, value)?;
            (
                all(&[a.success, format!("((_ is atom) {})", a.value)]),
                format!("(= (seq.len (atom_bytes {})) {len})", a.value),
            )
        }
        TreePredicate::Not { value } => {
            let value = predicate(tr, value)?;
            (value.success, format!("(not {})", value.value))
        }
        TreePredicate::All { items } | TreePredicate::Any { items } => {
            let values = items
                .iter()
                .map(|item| predicate(tr, item))
                .collect::<Result<Vec<_>>>()?;
            let success = all(&values.iter().map(|v| v.success.clone()).collect::<Vec<_>>());
            let terms = values.iter().map(|v| v.value.clone()).collect::<Vec<_>>();
            let value = if matches!(source, TreePredicate::All { .. }) {
                all(&terms)
            } else if terms.is_empty() {
                "false".into()
            } else {
                format!("(or {})", terms.join(" "))
            };
            (success, value)
        }
    };
    Ok(PredicateValue {
        success: tr.define("Bool", &success),
        value: tr.define("Bool", &value),
    })
}

fn decode(tr: &mut Translation, program: &Program) -> Result<String> {
    if program.environment == Environment::Raw {
        tr.arguments.push(Value::Tree("env".into()));
        return Ok("true".into());
    }
    let nil = format!("(atom {})", sequence("")?);
    let one = format!("(atom {})", sequence("01")?);
    let mut tail = "env".to_owned();
    let mut checks = vec![];
    for parameter in &program.parameters {
        checks.push(format!("((_ is pair) {tail})"));
        let head = tr.define("Tree", &format!("(pair_first {tail})"));
        tail = tr.define("Tree", &format!("(pair_rest {tail})"));
        checks.push(format!("((_ is atom) {head})"));
        let argument = match parameter.ty {
            Type::Atom | Type::Bytes { .. } => {
                let bytes = tr.define(SEQUENCE, &format!("(atom_bytes {head})"));
                if let Type::Bytes { len } = parameter.ty {
                    checks.push(format!("(= (seq.len {bytes}) {len})"));
                }
                Value::Atom(bytes)
            }
            Type::Bool => {
                checks.push(format!("(or (= {head} {nil}) (= {head} {one}))"));
                Value::Bool(tr.define("Bool", &format!("(= {head} {one})")))
            }
            _ => unreachable!("validated Boolean model signature"),
        };
        tr.arguments.push(argument);
    }
    checks.push(format!("(= {tail} {nil})"));
    Ok(tr.define("Bool", &all(&checks)))
}

/// Compare an actual source-derived expression IR with an independently written
/// raw-environment contract. All finite trees are in the counterexample domain.
pub fn emit_structural_boolean(
    program: &Program,
    specification: &TreeBooleanSpecification,
) -> Result<StructuralBooleanQueries> {
    validate_program(program)?;
    validate_specification(specification)?;
    let mut tr = Translation::default();
    let abi = decode(&mut tr, program)?;
    let execution = tr.expression(&program.body)?;
    let result = execution.value.unwrap_or(Value::Bool("false".into()));
    let actual_success = all(&[abi.clone(), execution.success]);
    let acceptance = predicate(&mut tr, &specification.acceptance)?;
    let expected_success = all(&[acceptance.success, acceptance.value]);
    let output = predicate(&mut tr, &specification.output)?;
    let nil = format!("(atom {})", sequence("")?);
    let one = format!("(atom {})", sequence("01")?);
    let actual_tree = format!("(ite {} {one} {nil})", result.boolean()?);
    let expected_tree = format!("(ite {} {one} {nil})", output.value);
    let mismatch = format!(
        "(or (not (= {actual_success} {expected_success})) (and {actual_success} {expected_success} (or (not {}) (not (= {actual_tree} {expected_tree})))))",
        output.success
    );
    let base = format!(
        "; Structural Boolean IR model only: arbitrary finite trees/byte sequences; no compiler, cost, termination or ABI-preservation proof.\n(set-logic ALL)\n(set-option :pp.min-alias-size 1000000)\n(declare-datatypes ((Tree 0)) (((atom (atom_bytes {SEQUENCE})) (pair (pair_first Tree) (pair_rest Tree)))))\n(declare-const env Tree)\n{}",
        tr.text
    );
    let query = |condition: &str| format!("{base}(assert {condition})\n(check-sat)\n");
    Ok(StructuralBooleanQueries {
        abi_nonvacuity: query(&abi),
        acceptance_nonvacuity: query(&expected_success),
        true_nonvacuity: query(&all(&[
            expected_success.clone(),
            output.success.clone(),
            output.value.clone(),
        ])),
        false_nonvacuity: query(&all(&[
            expected_success,
            output.success,
            format!("(not {})", output.value),
        ])),
        counterexample: query(&mismatch),
    })
}

/// Empty specified acceptance, timeout, unknown and malformed solver output are
/// errors. Program rejection is never used to constrain the witness domain.
pub fn verify_structural_boolean(
    program: &Program,
    specification: &TreeBooleanSpecification,
    config: &SolverConfig,
) -> Result<StructuralBooleanVerification> {
    let queries = emit_structural_boolean(program, specification)?;
    let abi_nonvacuity = solve_tree(config, &queries.abi_nonvacuity)?
        .context("structural Boolean ABI domain is empty; no proof produced")?;
    let acceptance_nonvacuity = solve_tree(config, &queries.acceptance_nonvacuity)?
        .context("structural Boolean specified acceptance is empty; no proof produced")?;
    let true_witness = solve_tree(config, &queries.true_nonvacuity)?;
    let false_witness = solve_tree(config, &queries.false_nonvacuity)?;
    Ok(match solve_tree(config, &queries.counterexample)? {
        None => StructuralBooleanVerification::Proven {
            abi_nonvacuity,
            acceptance_nonvacuity,
            true_witness,
            false_witness,
        },
        Some(counterexample) => StructuralBooleanVerification::Refuted {
            abi_nonvacuity,
            acceptance_nonvacuity,
            true_witness,
            false_witness,
            counterexample,
        },
    })
}

/// Execute the witness's complete environment through the actual compiler and
/// VM in both modes. Resource/infrastructure failures abort; success must be a
/// canonical Boolean atom, not an arbitrary truthy value.
pub fn replay_structural_boolean(
    program: &Program,
    witness: &TreeWitness,
    max_cost: u64,
) -> Result<Vec<Replay>> {
    validate_program(program)?;
    let replays = crate::tree_witness::replay_environment(program, witness, max_cost)?;
    for replay in &replays {
        if let ReplayOutcome::Accepted { output_hex, .. } = &replay.outcome {
            ensure!(
                output_hex == "80" || output_hex == "01",
                "Boolean replay returned noncanonical Boolean bytes"
            );
        }
    }
    Ok(replays)
}
