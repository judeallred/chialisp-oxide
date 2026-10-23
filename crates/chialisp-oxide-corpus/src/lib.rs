//! Reproducible cross-language and ledger validation of the teaching corpus.
mod authoring;
mod boomerang;
pub mod cat_genesis_tail;
mod curried;
mod factories;
mod native;
mod native_hash;
#[cfg(test)]
mod native_owned_tests;
mod native_owned_tree;
mod native_tree;
mod p2_delegated_conditions;
mod p2_singleton;
mod password;
mod spends;
use anyhow::{Context, Result, ensure};
pub use authoring::{Authoring, Export, authoring};
use chialisp_oxide_backend_experiment::{
    Artifact, Execution, FailureCode, MAX_COST, Outcome, artifact, execute,
};
use chialisp_oxide_ir::{Environment, Program};
pub use chialisp_oxide_simulator::REJECTION_PROTOCOL as SIMULATOR_REJECTION_PROTOCOL;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};
const MAX_VALIDATION_COST: u64 = 10_000_000;
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
    "p2-conditions",
    "rightmost-tree",
    "last-list-element",
    "prefix-list",
    "atom-not-equal",
    "insertion-sort-by-keys",
    "sha256tree",
    "singleton-launcher",
    "password-coin",
    "p2-singleton",
    "p2-delegated-conditions",
    "module-add",
    "module-times-ten",
    "module-factorial",
    "delegated-puzzle",
    "curry-hash",
    "currying-multiply",
    "curry-factory",
    "inner-puzzle",
    "boomerang",
    "precommit-claim",
    "precommit-registry",
    "cat-genesis-by-coin-id",
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
/// Execute one freshly funded precommit source-control ledger witness.
/// Artifact provenance and the independently compiled counterpart are supplied
/// and retained by the caller; this wrapper adds no source-authenticity claim.
pub fn validate_precommit_source_control(
    claim_code: &[u8],
    registry_code: &[u8],
    control: &str,
) -> Result<serde_json::Value> {
    Ok(serde_json::to_value(
        spends::validate_precommit_source_control(claim_code, registry_code, control)?,
    )?)
}
/// An independently specified result for a declared backend set. Only the
/// original Chialisp and the authored Rue translation may diverge from the
/// native Rust expectation; Rust CLVM must always match the actual Rust source.
#[derive(Clone, Deserialize, Serialize)]
pub struct DeclaredOutcome {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_hex: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct DeclaredDivergence {
    pub backends: Vec<String>,
    pub outcome: DeclaredOutcome,
    pub reason: String,
}
impl DeclaredDivergence {
    const INDEPENDENT_BACKENDS: [&str; 2] = ["chialisp", "rue_source"];
    fn outcome(&self, native: &Outcome) -> Result<Outcome> {
        ensure!(
            !self.backends.is_empty() && self.backends.len() <= Self::INDEPENDENT_BACKENDS.len(),
            "declared divergence needs one or two independent backends"
        );
        for (index, backend) in self.backends.iter().enumerate() {
            ensure!(
                Self::INDEPENDENT_BACKENDS.contains(&backend.as_str()),
                "declared divergence may only cover independently compiled languages, not Rust: {backend}"
            );
            ensure!(
                !self.backends[..index].contains(backend),
                "duplicate declared divergence backend {backend}"
            );
        }
        ensure!(
            !self.reason.trim().is_empty() && self.reason.len() <= 1024,
            "declared divergence needs a bounded reason"
        );
        let outcome = match (self.outcome.status.as_str(), &self.outcome.output_hex) {
            ("accepted", Some(output_hex)) => {
                ensure!(
                    !output_hex.is_empty()
                        && output_hex.len() % 2 == 0
                        && output_hex
                            .bytes()
                            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
                    "declared divergence output must be lowercase hex"
                );
                Outcome::Accepted {
                    output_hex: output_hex.clone(),
                }
            }
            ("rejected", None) => Outcome::Rejected,
            _ => anyhow::bail!(
                "declared divergence outcome must be accepted with output_hex or rejected"
            ),
        };
        ensure!(
            outcome != *native,
            "declared divergence must differ from the native Rust expectation"
        );
        Ok(outcome)
    }
}
#[derive(Deserialize, Serialize)]
pub struct Case {
    pub label: String,
    pub solution: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_divergence: Option<DeclaredDivergence>,
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
    #[serde(default)]
    pub reference_includes: BTreeMap<String, String>,
    #[serde(default)]
    pub native_sources: Vec<String>,
    #[serde(default)]
    pub validation_max_cost: Option<u64>,
    #[serde(default)]
    pub curried_examples: Vec<curried::Spec>,
    #[serde(default)]
    pub coin_instances: Vec<curried::Spec>,
    #[serde(default)]
    pub factory_relation: Option<String>,
    #[serde(default)]
    pub factory_applications: Vec<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared_divergence: Option<DeclaredDivergence>,
}
#[derive(Serialize)]
pub struct SourceDependency {
    pub path: String,
    pub sha256: String,
    pub source: String,
}

/// Resolve only explicitly declared source snapshots, never a compiler search
/// path. The reference adapter separately validates the flat include names.
fn include_sources<'a>(
    mode: ReferenceMode,
    includes: &'a BTreeMap<String, String>,
    dependencies: &'a [SourceDependency],
) -> Result<Vec<(&'a str, &'a str)>> {
    ensure!(
        includes.is_empty() || matches!(mode, ReferenceMode::Module),
        "reference includes are supported only for module references"
    );
    includes
        .iter()
        .map(|(name, path)| {
            ensure!(
                !std::path::Path::new(path).is_absolute()
                    && !path.contains('\\')
                    && path
                        .split('/')
                        .all(|part| !part.is_empty() && part != "." && part != ".."),
                "reference include must use a normalized confined source path"
            );
            let mut matching = dependencies.iter().filter(|source| source.path == *path);
            let source = matching.next().with_context(|| {
                format!("reference include {name:?} is not declared in source_dependencies")
            })?;
            ensure!(
                matching.next().is_none(),
                "ambiguous reference include source snapshot"
            );
            Ok((name.as_str(), source.source.as_str()))
        })
        .collect()
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
/// Deliberate low-budget observations, never ordinary validation rejection.
#[derive(Serialize)]
pub struct ResourceControls {
    pub max_cost: u64,
    pub cases: Vec<CaseReport>,
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
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub reference_includes: BTreeMap<String, String>,
    pub rue: RueSource,
    pub source_dependencies: Vec<SourceDependency>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub native_sources: Vec<authoring::Fingerprint>,
    pub artifacts: Vec<Artifact>,
    pub cases: Vec<CaseReport>,
    pub generated_cases: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_max_cost: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_controls: Option<ResourceControls>,
    pub simulator: Vec<spends::Scenario>,
    pub simulator_comparisons: Vec<SimulatorComparison>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub factory_checks: Vec<factories::FactoryCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_identity: Option<factories::ModuleIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curried_instances: Option<password::CurriedInstances>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curried_examples: Option<Vec<curried::Example>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coin_instances: Option<boomerang::CoinInstances>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_validation: Option<serde_json::Value>,
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
    max_cost: u64,
) -> Result<(Outcome, Vec<Run>)> {
    compare_declared(slug, solution, programs, max_cost, None)
}
/// Compare every build against the native Rust expectation, except that an
/// explicitly declared divergence binds the listed independent languages to
/// their own independently specified outcome instead.
fn compare_declared(
    slug: &str,
    solution: &str,
    programs: &[(&str, bool, Vec<u8>)],
    max_cost: u64,
    divergence: Option<&DeclaredDivergence>,
) -> Result<(Outcome, Vec<Run>)> {
    let expected = expectation(slug, solution)?;
    let declared = divergence
        .map(|divergence| divergence.outcome(&expected))
        .transpose()?;
    let mut runs = vec![];
    for (backend, optimize, bytes) in programs {
        let execution = execute(bytes, solution, max_cost)?;
        ensure!(
            execution.outcome != Outcome::Inconclusive,
            "inconclusive VM execution {slug} {backend} {solution}: {execution:?}"
        );
        let expected = match (&declared, divergence) {
            (Some(declared), Some(divergence))
                if divergence.backends.iter().any(|b| b == backend) =>
            {
                declared
            }
            _ => &expected,
        };
        ensure!(
            execution.outcome == *expected,
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
fn comparison_budget(requested: Option<u64>) -> Result<u64> {
    let max_cost = requested.unwrap_or(MAX_COST);
    ensure!(
        (1..=MAX_VALIDATION_COST).contains(&max_cost),
        "validation_max_cost must be between 1 and {MAX_VALIDATION_COST}"
    );
    Ok(max_cost)
}

fn hash_resource_controls(
    slug: &str,
    programs: &[(&str, bool, Vec<u8>)],
) -> Result<Option<ResourceControls>> {
    if slug != "sha256tree" {
        return Ok(None);
    }
    let generated: Vec<Case> = serde_json::from_slice(&std::fs::read(
        root().join("puzzles/sha256tree/generated-cases.json"),
    )?)?;
    let mut inputs = Vec::new();
    for label in [
        "right spine depth 64 with padded terminal atom",
        "left spine depth 64 with nonempty zero leaf",
        "right spine depth 128 with padded terminal atom",
        "left spine depth 128 with nonempty zero leaf",
    ] {
        let mut matching = generated.iter().filter(|case| case.label == label);
        let case = matching
            .next()
            .context("missing hash resource-control input")?;
        ensure!(
            matching.next().is_none(),
            "duplicate hash resource-control input"
        );
        inputs.push(Case {
            label: case.label.clone(),
            solution: case.solution.clone(),
            declared_divergence: None,
        });
    }
    // This is the existing shared raw-environment depth input, not a new
    // policy guard or a replacement for any ordinary generated case.
    let mut tree = "0x0001".to_string();
    for _ in 0..128 {
        tree = format!("({tree} . 0x00)");
    }
    inputs.push(Case {
        label: "corpus raw left spine depth 129".into(),
        solution: format!("({tree} . 0x0000)"),
        declared_divergence: None,
    });
    let mut cases = Vec::new();
    for input in inputs {
        let native = expectation(slug, &input.solution)?;
        ensure!(
            matches!(&native, Outcome::Accepted { .. }),
            "native hash control must succeed"
        );
        let mut runs = Vec::new();
        for (backend, optimize, bytes) in programs {
            let execution = execute(bytes, &input.solution, MAX_COST)?;
            let accepted = execution.outcome == native;
            let exhausted = execution.outcome == Outcome::Inconclusive
                && execution
                    .failure
                    .is_some_and(|failure| failure.code() == FailureCode::CostExceeded);
            ensure!(
                accepted || exhausted,
                "hash resource control changed output or produced an unexpected failure: {execution:?}"
            );
            runs.push(Run {
                backend: (*backend).into(),
                optimize_lir: *optimize,
                execution,
            });
        }
        cases.push(CaseReport {
            label: input.label,
            solution: input.solution,
            native,
            runs,
            declared_divergence: None,
        });
    }
    Ok(Some(ResourceControls {
        max_cost: MAX_COST,
        cases,
    }))
}

pub fn validate(slug: &str, program: &Program) -> Result<PuzzleReport> {
    ensure!(PUZZLES.contains(&slug), "unregistered corpus puzzle");
    let dir = root().join("puzzles").join(slug);
    let manifest: Manifest =
        serde_json::from_str(&std::fs::read_to_string(dir.join("puzzle.json"))?)?;
    ensure!(manifest.slug == slug, "corpus manifest slug mismatch");
    let max_cost = comparison_budget(manifest.validation_max_cost)?;
    ensure!(
        program.name == format!("chialisp_oxide_puzzles::{}::puzzle", slug.replace('-', "_")),
        "corpus export identity mismatch"
    );
    ensure!(
        program.environment == manifest.environment,
        "corpus environment policy mismatch"
    );
    // authoring() checks these live inputs against the build-time native oracle
    // snapshot. This is declared-input freshness, not executable authentication.
    let mut native_sources = Vec::new();
    if !manifest.native_sources.is_empty() {
        let compiled = authoring()?;
        for path in &manifest.native_sources {
            ensure!(
                !native_sources
                    .iter()
                    .any(|source: &authoring::Fingerprint| source.path == *path),
                "duplicate declared native source"
            );
            native_sources.push(
                compiled
                    .sources
                    .iter()
                    .find(|source| source.path == *path)
                    .with_context(|| format!("native source {path} is not a declared Cargo input"))?
                    .clone(),
            );
        }
    }
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
    let includes = include_sources(
        manifest.reference_mode,
        &manifest.reference_includes,
        &source_dependencies,
    )?;
    let original = match manifest.reference_mode {
        ReferenceMode::Module if !includes.is_empty() => {
            chialisp_oxide_reference::compile_with_includes(&reference, &includes)
        }
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
        let (native, runs) = compare_declared(
            slug,
            &case.solution,
            &programs,
            max_cost,
            case.declared_divergence.as_ref(),
        )?;
        cases.push(CaseReport {
            label: case.label,
            solution: case.solution,
            native,
            runs,
            declared_divergence: case.declared_divergence,
        });
    }
    let mut generated_cases = 0;
    if [
        "prefix-list",
        "atom-not-equal",
        "insertion-sort-by-keys",
        "sha256tree",
        "password-coin",
        "p2-singleton",
    ]
    .contains(&slug)
    {
        // The helper evidence gate independently recomputes this fixed suite
        // and its expected data relation. Execute every row in all five builds.
        let generated: Vec<Case> =
            serde_json::from_slice(&std::fs::read(dir.join("generated-cases.json"))?)?;
        for case in generated {
            compare(slug, &case.solution, &programs, max_cost)?;
            generated_cases += 1;
        }
    }
    if ["capped-payment", "split-payment", "bounded-counter"].contains(&slug) {
        for value in 0..=1020 {
            compare(slug, &format!("({value})"), &programs, max_cost)?;
            generated_cases += 1;
        }
    }
    if slug == "p2-delegated-conditions" {
        for case in p2_delegated_conditions::generated_cases()? {
            compare(slug, &case.solution, &programs, max_cost)?;
            generated_cases += 1;
        }
    }
    if manifest.environment == Environment::Ignored {
        for index in 0..256 {
            compare(slug, &tree_environment(index, 0), &programs, max_cost)?;
            generated_cases += 1;
        }
    }
    if manifest.environment == Environment::Raw {
        for index in 0..2048 {
            compare(slug, &tree_environment(index, 0), &programs, max_cost)?;
            generated_cases += 1;
        }
        // Atom widths and a deeper improper spine exercise data independently
        // of the finite compiler expression/type nesting budgets.
        for width in [0, 1, 31, 32, 63, 64, 255, 256, 4096, 8192] {
            let atom = if width == 0 {
                "()".into()
            } else {
                format!("0x{}", "00".repeat(width))
            };
            compare(slug, &format!("({atom} . 0x0001)"), &programs, max_cost)?;
            generated_cases += 1;
        }
        let mut tree = "0x0001".to_string();
        for _ in 0..128 {
            tree = format!("({tree} . 0x00)");
        }
        compare(slug, &format!("({tree} . 0x0000)"), &programs, max_cost)?;
        generated_cases += 1;
    }
    if slug == "rightmost-tree" {
        // Unlike factory inputs with a wide left payload, these bytes are the
        // returned atom. The compiled function has no depth or width guard.
        for width in [0, 1, 31, 32, 63, 64, 255, 256, 4096, 8192] {
            let atom = if width == 0 {
                "()".into()
            } else {
                format!("0x{}", "00".repeat(width))
            };
            compare(slug, &format!("(1 . {atom})"), &programs, max_cost)?;
            generated_cases += 1;
        }
        let mut tree = "0x0001".to_string();
        for _ in 0..128 {
            tree = format!("(0x00 . {tree})");
        }
        compare(slug, &tree, &programs, max_cost)?;
        generated_cases += 1;
    }
    if slug == "last-list-element" {
        // Returned elements may be arbitrary atoms/trees. Exercise the actual
        // final element rather than only ignored left branches or list tails.
        for width in [0, 1, 31, 32, 63, 64, 255, 256, 4096, 8192] {
            let value = if width == 0 {
                "()".into()
            } else {
                format!("0x{}", "00".repeat(width))
            };
            compare(slug, &format!("({value})"), &programs, max_cost)?;
            generated_cases += 1;
        }
        for depth in [1, 2, 8, 32, 128] {
            for tail in ["()", "0x00", "0x0000", "0x0001"] {
                let mut tree = tail.to_string();
                for _ in 0..depth {
                    tree = format!("(0x0001 . {tree})");
                }
                compare(slug, &tree, &programs, max_cost)?;
                generated_cases += 1;
            }
        }
        for value in [
            "()",
            "0x00",
            "0x0000",
            "0x0001",
            "(0x0001 . 0x0000)",
            "(8)",
            "((1 . 2) . (3 . 4))",
        ] {
            for environment in [
                format!("({value})"),
                format!("(0x00 ((8) . 0x0000) {value})"),
            ] {
                compare(slug, &environment, &programs, max_cost)?;
                generated_cases += 1;
            }
        }
    }
    let resource_controls = if slug == "password-coin" {
        Some(password::resource_controls(&cases, &programs)?)
    } else {
        hash_resource_controls(slug, &programs)?
    };
    let artifacts: Vec<Artifact> = programs
        .iter()
        .map(|(backend, opt, code)| artifact(backend, *opt, code))
        .collect::<Result<_>>()?;
    let simulator = match manifest.kind.as_str() {
        "coin_puzzle" if slug == "boomerang" => {
            ensure!(
                !manifest.coin_instances.is_empty(),
                "boomerang requires actual committed coin-instance evidence"
            );
            vec![]
        }
        "coin_puzzle" => spends::validate(slug, &programs[1].2)?,
        "pure_function" | "program_factory" | "protocol_component" => vec![],
        kind => anyhow::bail!("unknown corpus kind {kind}"),
    };
    let mut simulator_comparisons = vec![];
    if manifest.kind == "coin_puzzle" && slug != "boomerang" {
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
    let factory_checks = if manifest.kind == "program_factory" {
        match manifest.factory_relation.as_deref() {
            None => {
                ensure!(
                    slug == "p2-conditions" && manifest.factory_applications.is_empty(),
                    "factory requires an independent specification"
                );
                factories::validate(&cases)?
            }
            Some("standard_curry") => {
                ensure!(
                    !manifest.factory_applications.is_empty(),
                    "standard-curry factory needs declared applications"
                );
                factories::validate_curry(&cases, &manifest.factory_applications)?
            }
            Some(other) => anyhow::bail!("unknown factory relation {other}"),
        }
    } else {
        ensure!(
            manifest.factory_relation.is_none() && manifest.factory_applications.is_empty(),
            "factory relation declared for a non-factory"
        );
        vec![]
    };
    // A published module identity is independent of its output role. The
    // singleton launcher returns conditions; p2_conditions returns a program.
    // Extracted helpers have no whole-module identity obligation.
    let module_identity = if manifest.provenance["classification"] == "canonical_puzzle_port"
        && manifest.provenance.get("upstream_artifact").is_some()
    {
        Some(factories::identity(&manifest.provenance, &artifacts)?)
    } else {
        None
    };
    let curried_instances = if slug == "password-coin" {
        Some(password::validate(&cases, &programs)?)
    } else {
        None
    };
    let curried_examples = if manifest.curried_examples.is_empty() {
        None
    } else {
        ensure!(
            manifest.kind == "pure_function",
            "curried examples need a pure-function module"
        );
        Some(curried::validate(
            slug,
            &manifest.curried_examples,
            &programs,
            max_cost,
        )?)
    };
    let protocol_validation = if manifest.kind == "protocol_component" {
        Some(match slug {
            "p2-singleton" => p2_singleton::validate(&programs)?,
            "cat-genesis-by-coin-id" => cat_genesis_tail::validate(&programs)?,
            "p2-delegated-conditions" => p2_delegated_conditions::validate(&programs)?,
            _ => anyhow::bail!("protocol component needs a dedicated validated harness"),
        })
    } else {
        None
    };
    let coin_instances = if slug == "boomerang" {
        ensure!(
            manifest.kind == "coin_puzzle",
            "boomerang is a condition-producing coin policy"
        );
        Some(boomerang::validate(
            &manifest.coin_instances,
            &programs,
            max_cost,
        )?)
    } else {
        ensure!(
            manifest.coin_instances.is_empty(),
            "undeclared coin-instance protocol"
        );
        None
    };
    if !native_sources.is_empty() {
        // A changed declared input must not publish observations from an older
        // native binary, even when the change happened during compilation.
        authoring()?;
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
        reference_includes: manifest.reference_includes,
        rue,
        source_dependencies,
        native_sources,
        artifacts,
        cases,
        generated_cases,
        validation_max_cost: manifest.validation_max_cost,
        resource_controls,
        simulator,
        simulator_comparisons,
        factory_checks,
        module_identity,
        curried_instances,
        curried_examples,
        coin_instances,
        protocol_validation,
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

#[cfg(test)]
mod include_tests {
    use super::*;

    fn dependency(path: &str, source: &str) -> SourceDependency {
        SourceDependency {
            path: path.into(),
            source: source.into(),
            sha256: "snapshot hash tested by source reader".into(),
        }
    }

    #[test]
    fn reference_includes_use_declared_snapshots_and_preserve_their_exact_source() {
        let sources = vec![dependency(
            "fixtures/codes.clib",
            "(\n(defconstant VALUE 51)\n)\n",
        )];
        let includes = BTreeMap::from([("codes.clib".into(), "fixtures/codes.clib".into())]);
        assert_eq!(
            include_sources(ReferenceMode::Module, &includes, &sources).unwrap(),
            vec![("codes.clib", "(\n(defconstant VALUE 51)\n)\n")]
        );
        assert!(include_sources(ReferenceMode::Expression, &includes, &sources).is_err());
        assert!(include_sources(ReferenceMode::Module, &includes, &[]).is_err());
        let duplicate = vec![
            dependency("fixtures/codes.clib", "first"),
            dependency("fixtures/codes.clib", "second"),
        ];
        assert!(include_sources(ReferenceMode::Module, &includes, &duplicate).is_err());
        assert!(
            include_sources(ReferenceMode::Expression, &BTreeMap::new(), &[])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn reference_include_paths_cannot_alias_or_escape_declared_snapshots() {
        for path in [
            "",
            "/outside",
            "../outside",
            "fixtures/../codes.clib",
            "./codes.clib",
            "fixtures//codes.clib",
            "fixtures\\codes.clib",
        ] {
            let sources = vec![dependency(path, "()")];
            let includes = BTreeMap::from([("codes.clib".into(), path.into())]);
            assert!(
                include_sources(ReferenceMode::Module, &includes, &sources).is_err(),
                "{path:?}"
            );
        }
    }
}

#[cfg(test)]
mod budget_tests {
    use super::{MAX_COST, MAX_VALIDATION_COST, comparison_budget};

    #[test]
    fn comparison_budget_preserves_default_and_rejects_zero_or_excess() {
        assert_eq!(comparison_budget(None).unwrap(), MAX_COST);
        assert_eq!(comparison_budget(Some(1)).unwrap(), 1);
        assert_eq!(
            comparison_budget(Some(MAX_VALIDATION_COST)).unwrap(),
            MAX_VALIDATION_COST
        );
        for invalid in [0, MAX_VALIDATION_COST + 1, u64::MAX] {
            assert!(comparison_budget(Some(invalid)).is_err());
        }
    }
}
