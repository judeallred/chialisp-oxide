mod bundle;
mod measurement;

#[cfg(test)]
mod tests;

use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    schema_version: u32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    slug: String,
    backend: String,
    optimize_lir: bool,
    label: String,
    previous_transaction_height: u32,
    transaction: bundle::Transaction,
}

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        args.len() == 2,
        "usage: chialisp-oxide-spend-costs INPUT.json OUTPUT.json"
    );
    ensure!(
        !std::path::Path::new(&args[1]).exists(),
        "output already exists; preserve previous runs"
    );
    let input: Input = serde_json::from_slice(&std::fs::read(&args[0])?)?;
    ensure!(
        input.schema_version == 1 && !input.cases.is_empty() && input.cases.len() <= 256,
        "unsupported or excessive measurement jobs"
    );
    let mut cases = Vec::new();
    for case in input.cases {
        let costs = measurement::measure(
            &case.transaction.decode()?,
            case.previous_transaction_height,
        )?;
        cases.push(
            json!({"slug":case.slug, "backend":case.backend, "optimize_lir":case.optimize_lir,
            "label":case.label, "costs":costs}),
        );
    }
    std::fs::write(
        &args[1],
        serde_json::to_vec_pretty(&json!({"schema_version":1,"cases":cases}))?,
    )?;
    Ok(())
}
