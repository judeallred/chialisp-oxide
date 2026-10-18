//! Replay actual admitted source-policy mutations, distinct from constructed IR.
use anyhow::{Context, Result, ensure};
use chialisp_oxide_backend_experiment::{Outcome, artifact, execute};
use chialisp_oxide_ir::Program;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

fn main() -> Result<()> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        arguments.len() == 2,
        "usage: p2_singleton_controls BUNDLE_DIRECTORY OUTPUT.json"
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let bundle = PathBuf::from(&arguments[0]);
    let pointer: Value = serde_json::from_slice(&std::fs::read(bundle.join("latest.json"))?)?;
    let manifest_path = bundle.join(pointer["manifest"].as_str().context("manifest pointer")?);
    let manifest: Value = serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
    ensure!(
        manifest["package"] == "chialisp-oxide-p2-singleton-controls",
        "wrong control package"
    );
    let entries = manifest["entries"].as_array().context("entries")?;
    ensure!(entries.len() == 3, "wrong source-control export set");
    let recipes: Value = serde_json::from_slice(&std::fs::read(
        root.join("experiments/p2-singleton-controls/mutations.json"),
    )?)?;
    let baseline = std::fs::read_to_string(
        root.join(
            recipes["baseline_source"]
                .as_str()
                .context("baseline source")?,
        ),
    )?;
    let mut expected_entries = std::collections::BTreeSet::new();
    let mut source_controls = Vec::new();
    for recipe in recipes["mutations"]
        .as_array()
        .context("mutation recipes")?
    {
        let name = recipe["name"].as_str().context("mutation name")?;
        ensure!(
            hex::encode(Sha256::digest(baseline.as_bytes())) == recipe["original_sha256"],
            "stale mutation baseline"
        );
        let old = recipe["old"].as_str().context("old source")?;
        let new = recipe["new"].as_str().context("new source")?;
        ensure!(
            baseline.matches(old).count() == 1,
            "mutation no longer has exactly one source site"
        );
        let source_path = root.join(recipe["source"].as_str().context("mutated source")?);
        let source = std::fs::read_to_string(&source_path)?;
        ensure!(
            source == baseline.replacen(old, new, 1),
            "mutated source has changes outside its reviewed recipe"
        );
        expected_entries.insert(format!(
            "chialisp_oxide_p2_singleton_controls::{name}::puzzle"
        ));
        source_controls.push(json!({"name":name,"path":source_path,"sha256":hex::encode(Sha256::digest(source.as_bytes()))}));
    }
    let actual_entries = entries
        .iter()
        .map(|entry| {
            entry["entry"]
                .as_str()
                .map(str::to_owned)
                .context("entry name")
        })
        .collect::<Result<std::collections::BTreeSet<_>>>()?;
    ensure!(
        actual_entries == expected_entries,
        "source-control identity set changed"
    );
    let vectors: Value = serde_json::from_slice(&std::fs::read(
        root.join("puzzles/p2-singleton/vectors.json"),
    )?)?;
    let witness = &vectors["named"][0];
    let solution = witness["solution"].as_str().context("witness solution")?;
    let expected = witness["expected"]["output_hex"]
        .as_str()
        .context("expected output")?;
    let mut controls = Vec::new();
    for entry in entries {
        let ir_path = manifest_path
            .parent()
            .unwrap()
            .join(entry["ir"].as_str().context("IR file")?);
        let ir_bytes = std::fs::read(&ir_path)?;
        let program: Program = serde_json::from_slice(&ir_bytes)?;
        program.validate()?;
        let mut runs = Vec::new();
        for optimized in [false, true] {
            let bytes = chialisp_oxide_ir::compile(&program, optimized)?;
            let compiled = artifact("rust_source_policy_mutation", optimized, &bytes)?;
            let execution = execute(&bytes, solution, 100_000_000)?;
            let Outcome::Accepted { output_hex } = &execution.outcome else {
                anyhow::bail!("source-policy mutant must reach its changed output: {execution:?}")
            };
            ensure!(
                output_hex != expected,
                "policy mutation was not distinguished"
            );
            runs.push(json!({"optimized":optimized,"artifact":compiled,"execution":execution,"independent_expected_output_hex":expected,"distinguished":true}));
        }
        controls.push(json!({"entry":program.name,"ir_path":ir_path,"ir_sha256":hex::encode(Sha256::digest(ir_bytes)),"runs":runs}));
    }
    let report = json!({"scope":"Three actual ordinary Rust policy mutations admitted through Cargo. Native tests and six CLVM runs distinguish the independent protocol relation. This is not an error Rust makes impossible or constructed-IR mutation.","witness":witness,"manifest_path":manifest_path,"manifest_sha256":hex::encode(Sha256::digest(std::fs::read(&manifest_path)?)),"source_controls":source_controls,"controls":controls});
    std::fs::write(&arguments[1], serde_json::to_vec_pretty(&report)?)?;
    println!("Three actual Cargo source controls distinguished in both CLVM modes (six VM runs)");
    Ok(())
}
