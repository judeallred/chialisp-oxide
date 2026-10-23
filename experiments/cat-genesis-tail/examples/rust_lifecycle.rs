//! Actual compiled Rust TAIL lanes. Offline native SDK/VM harness, not a compiler.
#[path = "canonical_lifecycle.rs"]
mod canonical;
use anyhow::{Context, Result, ensure};
use chialisp_oxide_simulator::{
    CONSENSUS_VERSION, NETWORK, SIMULATOR_VERSION, SecretKey, VM_VERSION, puzzle_hash,
};
use clvmr::{
    Allocator, ChiaDialect,
    chia_dialect::MEMPOOL_MODE,
    run_program,
    serde::{node_from_bytes, node_to_bytes},
};
use serde_json::{Value, json};
use std::{fs::File, io::Read, path::Path};

fn bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    ensure!(
        path.is_file() && path.metadata()?.len() <= limit as u64,
        "bounded regular harness input"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "harness input changed beyond limit");
    Ok(bytes)
}
fn raw_run(module: &[u8], environment: &[u8]) -> Result<Value> {
    let mut a = Allocator::new_limited(8 * 1024 * 1024);
    let program = node_from_bytes(&mut a, module)?;
    let environment = node_from_bytes(&mut a, environment)?;
    Ok(
        match run_program(
            &mut a,
            &ChiaDialect::new(MEMPOOL_MODE),
            program,
            environment,
            1_000_000,
        ) {
            Ok(result) => {
                json!({"outcome":{"status":"accepted","output_hex":hex::encode(node_to_bytes(&a,result.1)?)},"cost":result.0,"failure":null})
            }
            Err(error) => {
                let failure = chialisp_oxide_vm::classify_failure(&error);
                ensure!(
                    failure.is_program_rejection(),
                    "inconclusive raw TAIL execution: {error}"
                );
                json!({"outcome":{"status":"rejected"},"cost":null,"failure":failure})
            }
        },
    )
}
fn main() -> Result<()> {
    let input = std::env::args_os()
        .nth(1)
        .context("usage: rust_lifecycle ARTIFACTS.json")?;
    let artifacts: Value = serde_json::from_slice(&bounded(Path::new(&input), 16 * 1024 * 1024)?)?;
    ensure!(
        artifacts["protocol"] == "chialisp-oxide-cat-genesis-rust-artifacts/v1",
        "explicit admitted Rust artifact input"
    );
    let modules = artifacts["artifacts"]
        .as_array()
        .context("six actual compiled exports")?;
    ensure!(modules.len() == 6, "exact two-mode three-control inventory");
    let canonical_module = canonical::compile(include_str!("../upstream/genesis_by_coin_id.clsp"))?;
    ensure!(
        canonical_module
            == hex::decode(include_str!("../upstream/genesis_by_coin_id.clsp.hex").trim())?
            && canonical_module == chia_puzzles::GENESIS_BY_COIN_ID,
        "independent canonical source identity"
    );
    let cat_module = canonical::compile(canonical::CAT_SOURCE)?;
    ensure!(
        cat_module == chia_puzzles::CAT_PUZZLE
            && cat_module == hex::decode(include_str!("../upstream/cat_v2.clsp.hex").trim())?,
        "independent canonical CAT2 identity"
    );
    let vectors: Value = serde_json::from_str(include_str!("../vectors.json"))?;
    let vectors = vectors["cases"].as_array().context("independent vectors")?;
    ensure!(vectors.len() == 37, "37 original vectors");
    let mut raw_runs = Vec::new();
    for row in vectors {
        let environment = hex::decode(row["environment_hex"].as_str().context("environment")?)?;
        let execution = raw_run(&canonical_module, &environment)?;
        ensure!(
            execution["outcome"] == row["expected"],
            "canonical independent expected outcome"
        );
        if execution["failure"].is_object() {
            ensure!(
                execution["failure"]["code"] == row["reference_failure_specification"]["code"],
                "canonical exact typed failure"
            );
        }
        raw_runs.push(json!({"language":"chialisp","mode":"canonical","control":"original","name":row["name"],"execution":execution}));
    }
    let owner = SecretKey::from_seed(&[0x63; 32]);
    let recipient = SecretKey::from_seed(&[0x64; 32]);
    let mut lanes = Vec::new();
    for mode in ["off", "on"] {
        let mut lane_modules = Vec::new();
        for control in ["original", "drop_delta_guard", "drop_genesis_equality"] {
            let selected = modules
                .iter()
                .filter(|a| a["mode"] == mode && a["control"] == control)
                .collect::<Vec<_>>();
            ensure!(selected.len() == 1, "unique mode/control export");
            let selected = selected[0];
            let encoded = selected["clvm_hex"]
                .as_str()
                .context("actual compiled module")?;
            ensure!(
                encoded.len() <= 4 * 1024 * 1024,
                "compiled module transport bound"
            );
            let module = hex::decode(encoded)?;
            let module_hash = hex::encode(puzzle_hash(&module)?);
            ensure!(
                module_hash
                    == selected["module_tree_hash"]
                        .as_str()
                        .context("module hash")?
                    && module_hash != hex::encode(chia_puzzles::GENESIS_BY_COIN_ID_HASH),
                "compiled module identity distinct from canonical"
            );
            for row in vectors {
                if control != "original"
                    && row["name"]
                        != if control == "drop_delta_guard" {
                            "non-nil-delta-one"
                        } else {
                            "mismatched-parent"
                        }
                {
                    continue;
                }
                let environment =
                    hex::decode(row["environment_hex"].as_str().context("environment")?)?;
                let execution = raw_run(&module, &environment)?;
                let expected = if control == "original" {
                    row["expected"].clone()
                } else {
                    json!({"status":"accepted","output_hex":"80"})
                };
                ensure!(
                    execution["outcome"] == expected,
                    "actual Rust {mode}/{control}/{} outcome",
                    row["name"]
                );
                // Rust panic translation is recorded as its own typed outcome.
                // Canonical projection/type codes are not inferred for Rust.
                if execution["failure"].is_object() {
                    ensure!(
                        execution["failure"]["code"] == "raise",
                        "actual Rust policy panic failure"
                    );
                }
                raw_runs.push(json!({"language":"rust","mode":mode,"control":control,"name":row["name"],"execution":execution}));
            }
            lane_modules.push((control, module, selected.clone()));
        }
        let original = &lane_modules[0].1;
        ensure!(
            lane_modules[1].1 != *original && lane_modules[2].1 != *original,
            "mutants retain changed actual artifacts"
        );
        let lifecycle = canonical::canonical_lifecycle(original, &owner, &recipient, 250_000_000)?;
        let mut controls = Vec::new();
        for (name, index, wrong_genesis, delta, accepted) in [
            ("original_wrong_genesis", 0, true, false, false),
            ("drop_genesis_wrong_genesis", 2, true, false, true),
            ("original_nonzero_delta", 0, false, true, false),
            ("drop_delta_nonzero_delta", 1, false, true, true),
        ] {
            let mut result = canonical::source_control(
                &lane_modules[index].1,
                name,
                wrong_genesis,
                delta,
                accepted,
                &owner,
                250_000_000,
            )?;
            result["artifact"] = lane_modules[index].2.clone();
            controls.push(result);
        }
        lanes.push(json!({"mode":mode,"original_artifact":lane_modules[0].2,"lifecycle":lifecycle,"source_controls":controls}));
    }
    let attempts = lanes
        .iter()
        .flat_map(|lane| {
            lane["lifecycle"]["attempts"]
                .as_array()
                .unwrap()
                .iter()
                .chain(
                    lane["source_controls"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .flat_map(|control| control["attempts"].as_array().unwrap().iter()),
                )
        })
        .collect::<Vec<_>>();
    let count = |status| {
        attempts
            .iter()
            .filter(|a| a["outcome"]["status"] == status)
            .count()
    };
    let component_count = attempts
        .iter()
        .map(|a| a["primary_vm"].as_array().unwrap().len())
        .sum::<usize>();
    println!(
        "{}",
        json!({"protocol":"chialisp-oxide-rust-cat-genesis-lifecycle/v1","status":"executed","sdk":SIMULATOR_VERSION,"consensus":CONSENSUS_VERSION,"sdk_vm":VM_VERSION,"network":NETWORK,"sdk_max_block_cost_clvm":chia_sdk_types::TESTNET11_CONSTANTS.max_block_cost_clvm,"sdk_transaction_max_cost":5_500_000_000u64,"primary_vm":"clvmr 0.21.0","primary_protocol":chialisp_oxide_vm::FAILURE_PROTOCOL,"raw_max_cost":1_000_000,"component_max_cost":250_000_000,"canonical_tail_hex":hex::encode(canonical_module),"cat_module_hex":hex::encode(cat_module),"owner_public_key":hex::encode(owner.public_key().to_bytes()),"recipient_public_key":hex::encode(recipient.public_key().to_bytes()),"artifacts":artifacts,"raw_tail_runs":raw_runs,"lanes":lanes,"sdk_transaction_attempts":attempts.len(),"accepted":count("accepted"),"typed_rejected":count("consensus_rejected"),"runtime_umbrellas":count("runtime_umbrella"),"component_primary_vm_runs":component_count,"scope":"Finite actual Rust TAIL off/on and independently compiled pinned canonical Chialisp raw vectors; independently funded own Rust TAIL/mutant assets through canonical CAT2 with a one-coin ring. Separate SDK/primary protocols; runtime umbrellas gain no typed consensus-rejection credit. No Sage, live network, corpus completion, authenticated execution or compiler-preservation claim."})
    );
    Ok(())
}
