//! Reproducible cross-language and ledger validation of the teaching corpus.
mod authoring;
mod native;
mod spends;
use anyhow::{Context, Result, ensure};
pub use authoring::{Authoring, Export, authoring};
use chialisp_oxide_backend_experiment::{
    Artifact, Execution, MAX_COST, Outcome, artifact, execute,
};
use chialisp_oxide_ir::{Environment, Program};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
    "intro-list",
    "intro-add",
    "intro-conditional",
];
#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceMode {
    #[default]
    Module,
    Expression,
}
impl ReferenceMode {
    fn is_module(&self) -> bool {
        matches!(self, Self::Module)
    }
}
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
    #[serde(default)]
    pub source_dependencies: Vec<String>,
    #[serde(default)]
    pub environment: Environment,
    #[serde(default)]
    pub reference_mode: ReferenceMode,
    #[serde(default)]
    pub rue_dependencies: Vec<String>,
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
pub struct SourceDependency {
    pub path: String,
    pub sha256: String,
    pub source: String,
}
#[derive(Serialize)]
pub struct RueSource {
    pub path: String,
    pub sha256: String,
    pub source: String,
    pub dependencies: Vec<SourceDependency>,
}
#[derive(Serialize)]
pub struct SimulatorComparison {
    pub backend: String,
    pub optimize_lir: bool,
    pub scenarios: Vec<spends::Scenario>,
}
#[derive(Serialize)]
pub struct PuzzleReport {
    pub slug: String,
    pub title: String,
    pub kind: String,
    pub security_scope: String,
    pub provenance: serde_json::Value,
    #[serde(skip_serializing_if = "Environment::is_strict")]
    pub environment: Environment,
    #[serde(skip_serializing_if = "ReferenceMode::is_module")]
    pub reference_mode: ReferenceMode,
    pub source: String,
    pub reference: String,
    pub rue: RueSource,
    pub source_dependencies: Vec<SourceDependency>,
    pub artifacts: Vec<Artifact>,
    pub cases: Vec<CaseReport>,
    pub generated_cases: usize,
    pub simulator: Vec<spends::Scenario>,
    pub simulator_comparisons: Vec<SimulatorComparison>,
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
/// Execute the real Cargo policy library with an independent canonical ABI codec.
pub fn native_execute(slug: &str, solution: &str) -> Result<Outcome> {
    ensure!(PUZZLES.contains(&slug), "unregistered corpus puzzle");
    expectation(slug, solution)
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
    ensure!(manifest.slug == slug, "corpus manifest slug mismatch");
    ensure!(
        program.name == format!("chialisp_oxide_puzzles::{}::puzzle", slug.replace('-', "_")),
        "corpus export identity mismatch"
    );
    ensure!(
        program.environment == manifest.environment,
        "corpus environment policy mismatch"
    );
    let reference = std::fs::read_to_string(dir.join("reference.clsp"))?;
    let source_dependencies = manifest
        .source_dependencies
        .iter()
        .map(|path| -> Result<SourceDependency> {
            let resolved = root().join(path).canonicalize()?;
            ensure!(
                resolved.starts_with(root()),
                "fixture dependency escapes repository"
            );
            let source = std::fs::read_to_string(resolved)?;
            Ok(SourceDependency {
                path: path.clone(),
                sha256: hex::encode(Sha256::digest(source.as_bytes())),
                source,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let source = std::fs::read_to_string(dir.join("puzzle.rs"))?;
    let rue_path = format!("puzzles/{slug}/puzzle.rue");
    let rue_source = std::fs::read_to_string(dir.join("puzzle.rue"))?;
    let rue = RueSource {
        path: rue_path,
        sha256: hex::encode(Sha256::digest(rue_source.as_bytes())),
        source: rue_source,
        dependencies: manifest
            .rue_dependencies
            .iter()
            .map(|path| -> Result<SourceDependency> {
                let resolved = root().join(path).canonicalize()?;
                ensure!(
                    resolved.starts_with(root()),
                    "Rue dependency escapes repository"
                );
                let source = std::fs::read_to_string(resolved)?;
                Ok(SourceDependency {
                    path: path.clone(),
                    sha256: hex::encode(Sha256::digest(source.as_bytes())),
                    source,
                })
            })
            .collect::<Result<_>>()?,
    };
    let original = match manifest.reference_mode {
        ReferenceMode::Module => chialisp_oxide_reference::compile(&reference),
        ReferenceMode::Expression => chialisp_oxide_reference::compile_expression(&reference),
    }
    .with_context(|| format!("compile Chialisp {slug}"))?;
    let programs = vec![
        (
            "rust_mir",
            false,
            chialisp_oxide_ir::compile(program, false)?,
        ),
        ("rust_mir", true, chialisp_oxide_ir::compile(program, true)?),
        ("chialisp", true, original),
        (
            "rue_source",
            false,
            chialisp_oxide_rue::compile_file(&dir.join("puzzle.rue"), false)?,
        ),
        (
            "rue_source",
            true,
            chialisp_oxide_rue::compile_file(&dir.join("puzzle.rue"), true)?,
        ),
    ];
    ensure!(
        std::fs::read_to_string(dir.join("puzzle.rue"))? == rue.source,
        "Rue source changed during compilation"
    );
    for dependency in &rue.dependencies {
        ensure!(
            std::fs::read_to_string(root().join(&dependency.path))? == dependency.source,
            "Rue dependency changed during compilation"
        );
    }
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
    if manifest.environment == Environment::Ignored {
        for index in 0..256 {
            compare(slug, &tree_environment(index, 0), &programs)?;
            generated_cases += 1;
        }
    }
    let artifacts = programs
        .iter()
        .map(|(backend, opt, code)| artifact(backend, *opt, code))
        .collect::<Result<_>>()?;
    let simulator = match manifest.kind.as_str() {
        "coin_puzzle" => spends::validate(slug, &programs[1].2)?,
        "pure_function" => vec![],
        kind => anyhow::bail!("unknown corpus kind {kind}"),
    };
    let mut simulator_comparisons = vec![];
    if manifest.kind == "coin_puzzle" {
        for (backend, optimize, code) in programs
            .iter()
            .filter(|(backend, opt, _)| *opt && *backend != "rust_mir")
        {
            simulator_comparisons.push(SimulatorComparison {
                backend: (*backend).into(),
                optimize_lir: *optimize,
                scenarios: spends::validate(slug, code)?,
            });
        }
    }
    Ok(PuzzleReport {
        slug: manifest.slug,
        title: manifest.title,
        kind: manifest.kind,
        security_scope: manifest.security_scope,
        provenance: manifest.provenance,
        environment: manifest.environment,
        reference_mode: manifest.reference_mode,
        source,
        reference,
        rue,
        source_dependencies,
        artifacts,
        cases,
        generated_cases,
        simulator,
        simulator_comparisons,
        validated: true,
    })
}
// A deterministic bounded set of atom, proper-list and improper-pair environments.
// Unlike a typed argument sweep, it includes numeric encodings that no u64 accepts.
fn tree_environment(index: usize, depth: usize) -> String {
    const ATOMS: &[&str] = &[
        "()",
        "1",
        "-1",
        "0x00",
        "0x0001",
        "0x80",
        "18446744073709551616",
        "0xffffffffffffffffffffffffffffffff",
    ];
    if index < ATOMS.len() || depth == 5 {
        return ATOMS[index % ATOMS.len()].into();
    }
    format!(
        "({} . {})",
        tree_environment(index / ATOMS.len(), depth + 1),
        tree_environment(index % ATOMS.len(), depth + 1)
    )
}
