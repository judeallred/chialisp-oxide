//! Eight actual SDK submissions for the paired precommit source controls.
//! Native policy checks and template VM observations are separate evidence.
use anyhow::{Context, Result, ensure};
use chialisp_oxide_simulator::{
    CONSENSUS_VERSION, NETWORK, SIMULATOR_VERSION, VM_VERSION, puzzle_hash,
};
use serde::Deserialize;
use serde_json::json;
use std::{collections::BTreeSet, io::Read};

const PROTOCOL: &str = "chialisp-oxide-precommit-source-controls-ledger/v1";
const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_MODULE_HEX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: String,
    runs: Vec<Input>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    mode: String,
    name: String,
    module_hex: String,
}

fn control(name: &str) -> Result<(&'static str, bool)> {
    Ok(match name {
        "claim_original" => ("minimum_age_original", true),
        "claim_drop_age" => ("minimum_age_removed", true),
        "registry_original" => ("owner_signature_original", false),
        "registry_drop_signature" => ("owner_signature_removed", false),
        _ => anyhow::bail!("unexpected precommit source-control name"),
    })
}

fn main() -> Result<()> {
    let args = std::env::args_os().collect::<Vec<_>>();
    ensure!(args.len() == 2, "usage: precommit_controls REQUEST.json");
    let file = std::fs::File::open(&args[1])?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_REQUEST_BYTES as u64,
        "request must be a bounded regular file"
    );
    let mut bytes = Vec::new();
    file.take((MAX_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_REQUEST_BYTES,
        "request grew beyond byte bound"
    );
    let request: Request = serde_json::from_slice(&bytes)?;
    ensure!(request.protocol == PROTOCOL, "unexpected ledger protocol");
    ensure!(
        request.runs.len() == 8,
        "exactly eight control runs required"
    );
    let mut identities = BTreeSet::new();
    for input in &request.runs {
        ensure!(matches!(input.mode.as_str(), "off" | "on"), "invalid mode");
        control(&input.name)?;
        ensure!(
            identities.insert((input.mode.as_str(), input.name.as_str())),
            "duplicate control mode/name"
        );
        ensure!(
            !input.module_hex.is_empty() && input.module_hex.len() <= MAX_MODULE_HEX_BYTES,
            "module hex exceeds predecode byte bound or is empty"
        );
    }
    // Compile each untouched reference source once, independently of every
    // candidate Cargo artifact. Each candidate composes with the other side.
    let reference_claim = chialisp_oxide_reference::compile(include_str!(
        "../../../puzzles/precommit-claim/reference.clsp"
    ))?;
    let reference_registry = chialisp_oxide_reference::compile(include_str!(
        "../../../puzzles/precommit-registry/reference.clsp"
    ))?;
    let mut runs = Vec::new();
    let mut transaction_attempts = 0usize;
    for input in request.runs {
        let (control, claim_candidate) = control(&input.name)?;
        let module = hex::decode(&input.module_hex).context("invalid candidate module hex")?;
        let reference = if claim_candidate {
            &reference_registry
        } else {
            &reference_claim
        };
        let (claim, registry) = if claim_candidate {
            (module.as_slice(), reference.as_slice())
        } else {
            (reference.as_slice(), module.as_slice())
        };
        let module_tree_hash = hex::encode(puzzle_hash(&module)?);
        let reference_tree_hash = hex::encode(puzzle_hash(reference)?);
        let scenarios =
            chialisp_oxide_corpus::validate_precommit_source_control(claim, registry, control)?;
        ensure!(
            scenarios.as_array().is_some_and(|rows| rows.len() == 1),
            "one SDK submission must return one scenario"
        );
        transaction_attempts += 1;
        runs.push(json!({
            "mode":input.mode,
            "name":input.name,
            "control":control,
            "module_tree_hash":module_tree_hash,
            "reference_tree_hash":reference_tree_hash,
            "scenarios":scenarios,
        }));
    }
    ensure!(
        transaction_attempts == 8,
        "exactly eight SDK submissions required"
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "protocol":PROTOCOL,
            "sdk":SIMULATOR_VERSION,
            "consensus":CONSENSUS_VERSION,
            "vm":VM_VERSION,
            "network":NETWORK,
            "transaction_attempts":transaction_attempts,
            "runs":runs,
        }))?
    );
    Ok(())
}
