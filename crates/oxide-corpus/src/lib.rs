//! Reproducible cross-language and ledger validation of the teaching corpus.
mod native;
mod spends;
use anyhow::{Context, Result, ensure};
use oxide_backend_experiment::{Artifact, Execution, MAX_COST, Outcome, artifact, execute};
use oxide_ir::Program;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
pub const PUZZLES: &[&str] = &[
    "fixed-payment",
    "capped-payment",
    "split-payment",
    "timelocked-payment",
    "signature-payment",
    "two-signature-payment",
    "recovery-payment",
    "bounded-counter",
    "boolean-gate",
];
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}
#[derive(Deserialize, Serialize)]
pub struct Case {
    pub label: String,
    pub solution: String,
}
#[derive(Deserialize, Serialize)]
pub struct Manifest {
    pub slug: String,
    pub title: String,
    pub kind: String,
    pub security_scope: String,
    pub provenance: serde_json::Value,
    pub cases: Vec<Case>,
}
#[derive(Serialize)]
pub struct Run {
    pub backend: String,
    pub optimize_lir: bool,
    pub execution: Execution,
}
#[derive(Serialize)]
pub struct CaseReport {
    pub label: String,
    pub solution: String,
    pub native: Outcome,
    pub runs: Vec<Run>,
}
#[derive(Serialize)]
pub struct PuzzleReport {
    pub slug: String,
    pub title: String,
    pub kind: String,
    pub security_scope: String,
    pub provenance: serde_json::Value,
    pub source: String,
    pub reference: String,
    pub artifacts: Vec<Artifact>,
    pub cases: Vec<CaseReport>,
    pub generated_cases: usize,
    pub simulator: Vec<spends::Scenario>,
    pub validated: bool,
}
fn expectation(slug: &str, solution: &str) -> Result<Outcome> {
    Ok(match native::execute(slug, solution)? {
        Some(bytes) => Outcome::Accepted {
            output_hex: hex::encode(bytes),
        },
        None => Outcome::Rejected,
    })
}
fn compare(
    slug: &str,
    solution: &str,
    programs: &[(&str, bool, Vec<u8>)],
) -> Result<(Outcome, Vec<Run>)> {
    let expected = expectation(slug, solution)?;
    let mut runs = vec![];
    for (backend, optimize, bytes) in programs {
        let execution = execute(bytes, solution, MAX_COST)?;
        ensure!(
            !execution
                .error
                .as_deref()
                .unwrap_or("")
                .contains("CostExceeded"),
            "unexpected cost exhaustion {slug} {backend} {solution}"
        );
        ensure!(
            execution.outcome == expected,
            "{slug} {backend} optimize={optimize} input={solution}: expected {expected:?}, got {execution:?}"
        );
        runs.push(Run {
            backend: (*backend).into(),
            optimize_lir: *optimize,
            execution,
        });
    }
    Ok((expected, runs))
}
pub fn validate(slug: &str, program: &Program) -> Result<PuzzleReport> {
    ensure!(PUZZLES.contains(&slug), "unregistered corpus puzzle");
    let dir = root().join("puzzles").join(slug);
    let manifest: Manifest =
        serde_json::from_str(&std::fs::read_to_string(dir.join("puzzle.json"))?)?;
    let reference = std::fs::read_to_string(dir.join("reference.clsp"))?;
    let source = std::fs::read_to_string(dir.join("puzzle.rs"))?;
    let original =
        oxide_chialisp::compile(&reference).with_context(|| format!("compile Chialisp {slug}"))?;
    let programs = vec![
        ("rust_mir", false, oxide_ir::compile(program, false)?),
        ("rust_mir", true, oxide_ir::compile(program, true)?),
        ("chialisp", true, original),
    ];
    let mut cases = vec![];
    for case in manifest.cases {
        let (native, runs) = compare(slug, &case.solution, &programs)?;
        cases.push(CaseReport {
            label: case.label,
            solution: case.solution,
            native,
            runs,
        });
    }
    let mut generated_cases = 0;
    if ["capped-payment", "split-payment", "bounded-counter"].contains(&slug) {
        for value in 0..=1020 {
            compare(slug, &format!("({value})"), &programs)?;
            generated_cases += 1;
        }
    }
    let artifacts = programs
        .iter()
        .map(|(backend, opt, code)| artifact(backend, *opt, code))
        .collect::<Result<_>>()?;
    let simulator = spends::validate(slug, &programs[1].2)?;
    Ok(PuzzleReport {
        slug: manifest.slug,
        title: manifest.title,
        kind: manifest.kind,
        security_scope: manifest.security_scope,
        provenance: manifest.provenance,
        source,
        reference,
        artifacts,
        cases,
        generated_cases,
        simulator,
        validated: true,
    })
}
