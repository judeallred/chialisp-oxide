//! Execute solver inputs through real compiled CLVM, separately from the model.
use anyhow::{Context, Result, bail, ensure};
use chialisp_oxide_ir::{Environment, Program, Type};
use chialisp_oxide_vm::{VmFailure, classify_failure};
use clvmr::{
    Allocator, ChiaDialect,
    allocator::NodePtr,
    chia_dialect::MEMPOOL_MODE,
    error::EvalErr,
    run_program,
    serde::{node_from_bytes, node_to_bytes},
};
use num_bigint::BigInt;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use std::{error::Error, fmt};

use crate::{Witness, WitnessValue};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replay {
    pub optimize: bool,
    pub outcome: ReplayOutcome,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReplayOutcome {
    Accepted {
        output_hex: String,
        cost: u64,
    },
    Rejected {
        error: String,
        #[serde(serialize_with = "serialize_program_failure")]
        failure: VmFailure,
    },
}

fn serialize_program_failure<S: serde::Serializer>(
    failure: &VmFailure,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    if !failure.is_program_rejection() {
        return Err(serde::ser::Error::custom(
            "rejected replay outcomes require a program failure",
        ));
    }
    failure.serialize(serializer)
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
enum ReplayOutcomeWire {
    Accepted { output_hex: String, cost: u64 },
    Rejected { error: String, failure: VmFailure },
}

impl<'de> Deserialize<'de> for ReplayOutcome {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        match ReplayOutcomeWire::deserialize(deserializer)? {
            ReplayOutcomeWire::Accepted { output_hex, cost } => {
                Ok(Self::Accepted { output_hex, cost })
            }
            ReplayOutcomeWire::Rejected { error, failure } => {
                if !failure.is_program_rejection() {
                    return Err(D::Error::custom(
                        "rejected replay outcomes require a program failure",
                    ));
                }
                Ok(Self::Rejected { error, failure })
            }
        }
    }
}

/// A VM resource or infrastructure failure is inconclusive replay evidence,
/// rather than a successful demonstration of puzzle rejection.
#[derive(Debug)]
pub struct ReplayFailure {
    pub failure: VmFailure,
    pub error: EvalErr,
}

impl fmt::Display for ReplayFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let failure = serde_json::to_string(&self.failure).map_err(|_| fmt::Error)?;
        write!(
            formatter,
            "CLVM witness replay aborted ({failure}): {:?}",
            self.error
        )
    }
}

impl Error for ReplayFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}

/// Classify every VM execution error before it can become replay evidence.
/// Shared typed classification is authoritative; diagnostic text is not parsed.
pub fn classify_replay_failure(error: EvalErr) -> Result<ReplayOutcome> {
    let failure = classify_failure(&error);
    if !failure.is_program_rejection() {
        return Err(ReplayFailure { failure, error }.into());
    }
    Ok(ReplayOutcome::Rejected {
        error: format!("{error:?}"),
        failure,
    })
}

fn allocate(allocator: &mut Allocator, value: &WitnessValue, ty: &Type) -> Result<NodePtr> {
    Ok(match (value, ty) {
        (WitnessValue::Int { value }, Type::Int { bits, signed }) => {
            ensure!(
                [8, 16, 32, 64, 128].contains(bits),
                "unsupported witness integer width"
            );
            let value: BigInt = value.parse().context("invalid witness integer")?;
            let magnitude = BigInt::from(1) << usize::from(bits - u16::from(*signed));
            let minimum = if *signed {
                -&magnitude
            } else {
                BigInt::from(0)
            };
            ensure!(
                value >= minimum && value < magnitude,
                "witness outside typed input domain"
            );
            allocator.new_atom(&if value == BigInt::from(0) {
                vec![]
            } else {
                value.to_signed_bytes_be()
            })?
        }
        (WitnessValue::Bool { value }, Type::Bool) => {
            if *value {
                allocator.new_atom(&[1])?
            } else {
                NodePtr::NIL
            }
        }
        (WitnessValue::Unit, Type::Unit) => NodePtr::NIL,
        (WitnessValue::Tuple { items }, Type::Tuple { items: types }) => {
            ensure!(items.len() == types.len(), "witness tuple arity mismatch");
            let mut tail = NodePtr::NIL;
            for (item, ty) in items.iter().zip(types).rev() {
                let head = allocate(allocator, item, ty)?;
                tail = allocator.new_pair(head, tail)?;
            }
            tail
        }
        _ => bail!("witness type mismatch or unsupported byte value"),
    })
}

/// Replay canonical typed witnesses with optimized and unoptimized code.
/// Resource/infrastructure failures cannot masquerade as program rejection.
pub fn replay(program: &Program, witness: &Witness, max_cost: u64) -> Result<Vec<Replay>> {
    ensure!(max_cost > 0, "CLVM replay cost limit must be positive");
    program.validate()?;
    ensure!(
        program.environment != Environment::Raw,
        "scalar witness replay does not encode raw environments; use replay_tree"
    );
    ensure!(
        witness.arguments.len() == program.parameters.len(),
        "witness argument count mismatch"
    );
    let mut results = Vec::new();
    for optimize in [false, true] {
        let bytes = chialisp_oxide_ir::compile(program, optimize)?;
        let mut allocator = Allocator::new();
        let root = node_from_bytes(&mut allocator, &bytes)?;
        let mut solution = NodePtr::NIL;
        for (value, parameter) in witness.arguments.iter().zip(&program.parameters).rev() {
            let head = allocate(&mut allocator, value, &parameter.ty)?;
            solution = allocator.new_pair(head, solution)?;
        }
        let outcome = match run_program(
            &mut allocator,
            &ChiaDialect::new(MEMPOOL_MODE),
            root,
            solution,
            max_cost,
        ) {
            Ok(reduction) => {
                let bytes = node_to_bytes(&allocator, reduction.1)?;
                let output_hex = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
                ReplayOutcome::Accepted {
                    output_hex,
                    cost: reduction.0,
                }
            }
            Err(error) => classify_replay_failure(error).with_context(|| {
                format!("scalar witness replay optimize={optimize} max_cost={max_cost}")
            })?,
        };
        results.push(Replay { optimize, outcome });
    }
    Ok(results)
}
