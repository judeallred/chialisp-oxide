//! Compile trusted canonical source independently, then replay bounded witnesses.
use anyhow::{Context, Result, ensure};
use chialisp_oxide_verifier::TreeWitness;
use clvmr::{
    Allocator, ChiaDialect,
    chia_dialect::MEMPOOL_MODE,
    error::EvalErr,
    run_program,
    serde::{node_from_bytes, node_to_bytes, tree_hash_from_stream},
};
use serde_json::json;
use std::{io::Cursor, path::Path};

const MAX_COST: u64 = 10_000_000;

fn decode(hex: &str) -> Result<Vec<u8>> {
    ensure!(
        hex.len().is_multiple_of(2)
            && hex.len() <= 4_194_304
            && hex.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid/oversized witness serialization"
    );
    (0..hex.len())
        .step_by(2)
        .map(|index| Ok(u8::from_str_radix(&hex[index..index + 2], 16)?))
        .collect()
}

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        (1..=33).contains(&args.len()),
        "usage: chialisp-oxide-tree-reference SOURCE.clsp [WITNESS.json ...] (at most32 witnesses)"
    );
    let source =
        std::fs::read_to_string(Path::new(&args[0])).context("read trusted reference source")?;
    ensure!(
        source.len() <= 65_536,
        "trusted reference source too large for this proof bridge"
    );
    let program = chialisp_oxide_reference::compile(&source)?;
    let tree_hash = tree_hash_from_stream(&mut Cursor::new(program.as_slice()))?;
    let mut runs = vec![];
    for path in &args[1..] {
        let input = std::fs::read(path).context("read structural witness")?;
        ensure!(input.len() <= 2_097_152, "witness JSON too large");
        let witness: TreeWitness = serde_json::from_slice(&input)?;
        let serialized = witness.environment.serialized_hex()?;
        let mut allocator = Allocator::new();
        let root = node_from_bytes(&mut allocator, &program)?;
        let environment = node_from_bytes(&mut allocator, &decode(&serialized)?)?;
        let outcome = match run_program(
            &mut allocator,
            &ChiaDialect::new(MEMPOOL_MODE),
            root,
            environment,
            MAX_COST,
        ) {
            Ok(reduction) => json!({"status":"accepted", "cost":reduction.0,
                "output_hex":node_to_bytes(&allocator, reduction.1)?.iter().map(|byte| format!("{byte:02x}")).collect::<String>()}),
            Err(EvalErr::CostExceeded) => {
                anyhow::bail!("reference witness replay exceeded cost limit {MAX_COST}")
            }
            Err(error) => json!({"status":"rejected", "error":format!("{error:?}")}),
        };
        runs.push(json!({"witness":witness, "outcome":outcome}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"compiler":chialisp_oxide_reference::COMPILER_VERSION,
        "vm":"clvmr 0.21.0 MEMPOOL_MODE", "max_cost":MAX_COST, "bytes":program.len(),
        "clvm_hex":program.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
        "tree_hash":tree_hash.iter().map(|byte| format!("{byte:02x}")).collect::<String>(), "runs":runs})
        )?
    );
    Ok(())
}
