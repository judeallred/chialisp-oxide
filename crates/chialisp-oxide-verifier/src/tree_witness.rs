//! Concrete structural witnesses; these are not the symbolic semantics.
use anyhow::{Context, Result, ensure};
use chialisp_oxide_ir::{Environment, Program, Type};
use clvmr::{
    Allocator, ChiaDialect,
    allocator::NodePtr,
    chia_dialect::MEMPOOL_MODE,
    run_program,
    serde::{node_from_bytes, node_to_bytes},
};
use serde::{Deserialize, Serialize};

use crate::{Replay, ReplayOutcome, classify_replay_failure};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TreeValue {
    Atom {
        hex: String,
    },
    Pair {
        first: Box<TreeValue>,
        rest: Box<TreeValue>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeWitness {
    pub environment: TreeValue,
}

pub(crate) fn decode_hex(hex: &str) -> Result<Vec<u8>> {
    ensure!(
        hex.len() <= 2_097_152
            && hex.len().is_multiple_of(2)
            && hex.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid or oversized atom hex"
    );
    (0..hex.len())
        .step_by(2)
        .map(|index| Ok(u8::from_str_radix(&hex[index..index + 2], 16)?))
        .collect()
}

impl TreeValue {
    fn validate(&self) -> Result<()> {
        let mut pending = vec![(self, 0usize)];
        let mut nodes = 0usize;
        let mut bytes = 0usize;
        while let Some((value, depth)) = pending.pop() {
            nodes += 1;
            ensure!(
                depth <= 128 && nodes <= 20_000,
                "tree witness nesting/node limit"
            );
            match value {
                Self::Atom { hex } => {
                    bytes = bytes
                        .checked_add(hex.len() / 2)
                        .context("tree witness byte budget overflow")?;
                    ensure!(bytes <= 1_048_576, "tree witness total byte limit");
                    decode_hex(hex)?;
                }
                Self::Pair { first, rest } => {
                    pending.push((first, depth + 1));
                    pending.push((rest, depth + 1));
                }
            }
        }
        Ok(())
    }
    fn allocate(
        &self,
        allocator: &mut Allocator,
        depth: usize,
        budget: &mut usize,
    ) -> Result<NodePtr> {
        ensure!(
            depth <= 128 && *budget > 0,
            "tree witness nesting/node limit"
        );
        *budget -= 1;
        Ok(match self {
            Self::Atom { hex } => allocator.new_atom(&decode_hex(hex)?)?,
            Self::Pair { first, rest } => {
                let first = first.allocate(allocator, depth + 1, budget)?;
                let rest = rest.allocate(allocator, depth + 1, budget)?;
                allocator.new_pair(first, rest)?
            }
        })
    }

    /// A safe raw assembly expression preserving byte atoms and improper tails.
    pub fn solution(&self) -> Result<String> {
        self.validate()?;
        fn render(value: &TreeValue, depth: usize, budget: &mut usize) -> Result<String> {
            ensure!(
                depth <= 128 && *budget > 0,
                "tree witness nesting/node limit"
            );
            *budget -= 1;
            Ok(match value {
                TreeValue::Atom { hex } => {
                    let bytes = decode_hex(hex)?;
                    if bytes.is_empty() {
                        "()".into()
                    } else {
                        format!("0x{}", hex.to_ascii_lowercase())
                    }
                }
                TreeValue::Pair { first, rest } => format!(
                    "({} . {})",
                    render(first, depth + 1, budget)?,
                    render(rest, depth + 1, budget)?
                ),
            })
        }
        render(self, 0, &mut 20_000)
    }

    pub fn serialized_hex(&self) -> Result<String> {
        self.validate()?;
        let mut allocator = Allocator::new();
        let node = self.allocate(&mut allocator, 0, &mut 20_000)?;
        Ok(node_to_bytes(&allocator, node)?
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }
}

/// Execute the complete raw environment, without wrapping it as typed arguments.
/// Replay limits affect witness execution only, not the symbolic tree domain.
pub fn replay_tree(program: &Program, witness: &TreeWitness, max_cost: u64) -> Result<Vec<Replay>> {
    ensure!(max_cost > 0, "tree replay cost limit must be positive");
    program.validate()?;
    witness.environment.validate()?;
    ensure!(
        program.environment == Environment::Raw
            && program.parameters.len() == 1
            && program.parameters[0].ty == Type::Tree
            && program.result == Type::Tree,
        "tree replay requires one raw tree input and a tree result"
    );
    let mut results = Vec::new();
    for optimize in [false, true] {
        let bytes = chialisp_oxide_ir::compile(program, optimize)?;
        let mut allocator = Allocator::new();
        let root = node_from_bytes(&mut allocator, &bytes)?;
        let solution = witness
            .environment
            .allocate(&mut allocator, 0, &mut 20_000)?;
        let outcome = match run_program(
            &mut allocator,
            &ChiaDialect::new(MEMPOOL_MODE),
            root,
            solution,
            max_cost,
        ) {
            Ok(reduction) => ReplayOutcome::Accepted {
                output_hex: node_to_bytes(&allocator, reduction.1)?
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect(),
                cost: reduction.0,
            },
            Err(error) => classify_replay_failure(error).with_context(|| {
                format!("tree witness replay optimize={optimize} max_cost={max_cost}")
            })?,
        };
        results.push(Replay { optimize, outcome });
    }
    Ok(results)
}
