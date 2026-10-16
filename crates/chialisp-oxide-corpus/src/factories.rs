//! Validate a factory's returned program separately from running the factory.
use super::{Artifact, CaseReport, Execution, MAX_COST, Outcome, Result, execute};
use anyhow::ensure;
use clvmr::{Allocator, allocator::SExp, serde::node_to_bytes};
use serde::Serialize;

#[derive(Serialize)]
pub struct ApplicationRun {
    pub solution: String,
    pub execution: Execution,
}
#[derive(Serialize)]
pub struct FactoryCheck {
    pub backend: String,
    pub optimize_lir: bool,
    pub environment: String,
    pub expected_output_hex: String,
    pub program_hex: String,
    pub application_runs: Vec<ApplicationRun>,
}
#[derive(Serialize)]
pub struct IdentityComparison {
    pub backend: String,
    pub optimize_lir: bool,
    pub serialized_bytes_equal: bool,
    pub tree_hash_equal: bool,
}
#[derive(Serialize)]
pub struct ModuleIdentity {
    pub upstream_hex: String,
    pub upstream_tree_hash: String,
    pub comparisons: Vec<IdentityComparison>,
}
pub fn identity(provenance: &serde_json::Value, artifacts: &[Artifact]) -> Result<ModuleIdentity> {
    let upstream = &provenance["upstream_artifact"];
    let bytes = upstream["clvm_hex"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing canonical hex"))?;
    let hash = upstream["tree_hash"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing canonical hash"))?;
    let comparisons: Vec<_> = artifacts
        .iter()
        .map(|a| IdentityComparison {
            backend: a.backend.clone(),
            optimize_lir: a.optimize_lir,
            serialized_bytes_equal: a.clvm_hex == bytes,
            tree_hash_equal: a.tree_hash == hash,
        })
        .collect();
    ensure!(
        comparisons
            .iter()
            .any(|c| c.backend == "chialisp" && c.serialized_bytes_equal && c.tree_hash_equal),
        "canonical compiler did not reproduce published module"
    );
    Ok(ModuleIdentity {
        upstream_hex: bytes.into(),
        upstream_tree_hash: hash.into(),
        comparisons,
    })
}
pub fn validate(cases: &[CaseReport]) -> Result<Vec<FactoryCheck>> {
    let mut checks = vec![];
    for case in cases {
        let mut a = Allocator::new();
        let env = rue_clvm::assemble(&mut a, &case.solution)?;
        let SExp::Pair(first, _) = a.sexp(env) else {
            ensure!(
                case.native == Outcome::Rejected,
                "factory accepts atom environment"
            );
            continue;
        };
        let expected_output_hex = hex::encode(node_to_bytes(&a, first)?);
        let quote = a.new_atom(&[1])?;
        let expected_program = a.new_pair(quote, first)?;
        let expected_program_hex = hex::encode(node_to_bytes(&a, expected_program)?);
        for run in &case.runs {
            let Outcome::Accepted { output_hex } = &run.execution.outcome else {
                anyhow::bail!("factory rejects pair environment");
            };
            ensure!(
                *output_hex == expected_program_hex,
                "factory output differs from independent raw-tree specification"
            );
            let code = hex::decode(output_hex)?;
            let mut application_runs = vec![];
            for solution in ["()", "1", "0x0001", "(1 . 2)"] {
                let execution = execute(&code, solution, MAX_COST)?;
                ensure!(
                    execution.outcome
                        == Outcome::Accepted {
                            output_hex: expected_output_hex.clone()
                        },
                    "produced program did not return exact payload"
                );
                application_runs.push(ApplicationRun {
                    solution: solution.into(),
                    execution,
                });
            }
            checks.push(FactoryCheck {
                backend: run.backend.clone(),
                optimize_lir: run.optimize_lir,
                environment: case.solution.clone(),
                expected_output_hex: expected_output_hex.clone(),
                program_hex: output_hex.clone(),
                application_runs,
            });
        }
    }
    Ok(checks)
}
