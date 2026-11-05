//! Bounded transport for repeated execution of the same retained program.
//! Each solution still uses a fresh VM allocator and the ordinary executor.
use std::{fs::File, io::Read, path::Path};

use anyhow::{Context, Result, ensure};
use chialisp_oxide_backend_experiment::execute;
use serde::de::{Error, IgnoredAny, SeqAccess, Visitor};

const MAX_PROGRAM_HEX: usize = 4 * 1024 * 1024;
const MAX_INPUT_JSON: usize = 8 * 1024 * 1024;
const MAX_OUTPUT_JSON: usize = 16 * 1024 * 1024;
const MAX_SOLUTIONS: usize = 4096;
const MAX_VM_COST: u64 = 100_000_000;

struct Solutions;
impl<'de> Visitor<'de> for Solutions {
    type Value = Vec<String>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a bounded array of CLVM assembly strings")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let mut solutions = Vec::new();
        for _ in 0..MAX_SOLUTIONS {
            match sequence.next_element::<String>()? {
                Some(value) => solutions.push(value),
                None if solutions.is_empty() => {
                    return Err(A::Error::custom("batch requires at least one solution"));
                }
                None => return Ok(solutions),
            }
        }
        // Check for one extra value without allocating another String or
        // collecting the rest of a potentially very wide input array.
        if sequence.next_element::<IgnoredAny>()?.is_some() {
            return Err(A::Error::custom(format!(
                "batch exceeds {MAX_SOLUTIONS}-solution limit"
            )));
        }
        Ok(solutions)
    }
}

fn bounded_read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .with_context(|| format!("open {}", path.display()))?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= limit,
        "batch file exceeds {limit}-byte limit"
    );
    Ok(bytes)
}

/// Exit status describes transport success; each element retains its typed VM
/// outcome, including rejection or inconclusive resource exhaustion. No JSON is
/// published if any input is malformed or the response exceeds its byte bound.
pub fn run(program: &Path, solutions: &Path, max_cost: u64) -> Result<Vec<u8>> {
    ensure!(
        (1..=MAX_VM_COST).contains(&max_cost),
        "batch VM cost must be between 1 and {MAX_VM_COST} per solution"
    );
    let program = bounded_read(program, MAX_PROGRAM_HEX)?;
    let program = hex::decode(std::str::from_utf8(&program)?.trim())?;
    let input = bounded_read(solutions, MAX_INPUT_JSON)?;
    let mut decoder = serde_json::Deserializer::from_slice(&input);
    let solutions = serde::Deserializer::deserialize_seq(&mut decoder, Solutions)
        .context("batch solutions must be a bounded JSON array of CLVM assembly strings")?;
    decoder
        .end()
        .context("trailing data after batch solutions")?;
    let mut output = vec![b'['];
    for (index, solution) in solutions.iter().enumerate() {
        let execution = execute(&program, solution, max_cost)
            .with_context(|| format!("batch solution {index}"))?;
        let encoded = serde_json::to_vec(&execution)?;
        ensure!(
            output.len().saturating_add(encoded.len()).saturating_add(2) <= MAX_OUTPUT_JSON,
            "batch response exceeds {MAX_OUTPUT_JSON}-byte limit"
        );
        if index > 0 {
            output.push(b',');
        }
        output.extend_from_slice(&encoded);
    }
    output.push(b']');
    Ok(output)
}
