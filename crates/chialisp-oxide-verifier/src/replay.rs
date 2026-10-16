//! Execute solver inputs through real compiled CLVM, separately from the model.
use anyhow::{Context, Result, bail, ensure};
use chialisp_oxide_ir::{Environment, Program, Type};
use clvmr::{
    Allocator, ChiaDialect,
    allocator::NodePtr,
    chia_dialect::MEMPOOL_MODE,
    error::EvalErr,
    run_program,
    serde::{node_from_bytes, node_to_bytes},
};
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};

use crate::{Witness, WitnessValue};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replay {
    pub optimize: bool,
    pub outcome: ReplayOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReplayOutcome {
    Accepted { output_hex: String, cost: u64 },
    Rejected { error: String },
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

/// Replay canonical typed witnesses with optimized and unoptimized code. Cost
/// exhaustion is an error, so it cannot masquerade as a policy rejection.
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
            Err(EvalErr::CostExceeded) => {
                bail!("CLVM witness replay exceeded cost limit {max_cost}")
            }
            Err(error) => ReplayOutcome::Rejected {
                error: format!("{error:?}"),
            },
        };
        results.push(Replay { optimize, outcome });
    }
    Ok(results)
}
