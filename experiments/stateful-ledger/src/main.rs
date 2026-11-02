//! Offline SDK ledger executions. No wallet, network, or compiler authority.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
mod batches;
mod common;
mod fixture_keys;
#[path = "../../spend-costs/src/measurement.rs"]
mod measurement;
mod mutations;
mod spuds;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    reference_only: bool,
    programs: Vec<Program>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Program {
    name: String,
    batch_two: PathBuf,
    spuds: PathBuf,
    batch_one: PathBuf,
}
fn read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut f = fs::File::open(path)?;
    let m = f.metadata()?;
    ensure!(
        m.is_file() && m.len() <= limit as u64,
        "bounded regular input"
    );
    let mut b = vec![];
    (&mut f).take(limit as u64 + 1).read_to_end(&mut b)?;
    ensure!(b.len() <= limit, "input grew");
    Ok(b)
}
fn hex_program(path: &Path) -> Result<Vec<u8>> {
    let b = hex::decode(std::str::from_utf8(&read(path, 4_000_000)?)?.trim())?;
    chialisp_oxide_simulator::program(&b)?;
    Ok(b)
}
fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let first = args.next().context("input JSON path")?;
    if first == "--mutations" {
        let input = PathBuf::from(args.next().context("mutation input path")?);
        let output = PathBuf::from(args.next().context("mutation output path")?);
        ensure!(args.next().is_none(), "unexpected args");
        return mutations::execute(&input, &output);
    }
    let input = PathBuf::from(first);
    let output = PathBuf::from(args.next().context("output JSON path")?);
    ensure!(args.next().is_none(), "unexpected args");
    let input: Input = serde_json::from_slice(&read(&input, 1_000_000)?)?;
    let expected = if input.reference_only {
        vec!["chialisp"]
    } else {
        vec!["chialisp", "rust_on", "rust_off"]
    };
    ensure!(
        input
            .programs
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            == expected,
        "exact ordered mode set required"
    );
    let mut reports = vec![];
    for p in input.programs {
        let batch = hex_program(&p.batch_two)?;
        let spuds = hex_program(&p.spuds)?;
        let old = hex_program(&p.batch_one)?;
        if p.name == "chialisp" {
            ensure!(
                batch
                    == chialisp_oxide_reference::compile(include_str!(
                        "../../continuation-batches/reference.clsp"
                    ))?,
                "batch reference differs from fresh independent compilation"
            );
            ensure!(
                spuds
                    == chialisp_oxide_reference::compile(include_str!(
                        "../../spuds/reference.clsp"
                    ))?,
                "spuds reference differs from fresh independent compilation"
            );
        }
        let mut comparisons = vec![];
        for n in [4, 8] {
            let b1 = batches::run(&old, n, 1)?;
            let b2 = batches::run(&batch, n, 2)?;
            ensure!(
                b1["recipient_queue"] == b2["recipient_queue"]
                    && b1["payout_total"] == b2["payout_total"],
                "same queue and payout"
            );
            let mut deltas = serde_json::Map::new();
            for encoding in ["uncompressed", "backrefs"] {
                let mut fields = serde_json::Map::new();
                for field in [
                    "charged_bytes",
                    "execution_cost",
                    "condition_cost",
                    "byte_cost",
                    "total_cost",
                ] {
                    fields.insert(
                        field.into(),
                        json!(
                            i128::from(b2["whole_chain"][encoding][field].as_u64().unwrap())
                                - i128::from(b1["whole_chain"][encoding][field].as_u64().unwrap())
                        ),
                    );
                }
                deltas.insert(encoding.into(), Value::Object(fields));
            }
            comparisons.push(
                json!({"count":n,"batch_one":b1,"batch_two":b2,"batch_two_minus_batch_one":deltas}),
            );
        }
        reports.push(json!({"mode":p.name,"artifact_blob_sha256":{"batch_two":hex::encode(common::sha(&[&batch])),"spuds":hex::encode(common::sha(&[&spuds])),"batch_one":hex::encode(common::sha(&[&old]))},"module_hashes":{"batch_two":hex::encode(chialisp_oxide_simulator::puzzle_hash(&batch)?),"spuds":hex::encode(chialisp_oxide_simulator::puzzle_hash(&spuds)?),"batch_one":hex::encode(chialisp_oxide_simulator::puzzle_hash(&old)?)},"batch_comparisons":comparisons,"spuds":spuds::run(&spuds)?}));
    }
    let report: Value = json!({"schema":1,"scope":if input.reference_only{"exploratory_reference_only"}else{"three_mode_sdk_execution"},"network":"TESTNET11","simulator":chialisp_oxide_simulator::SIMULATOR_VERSION,"consensus":chialisp_oxide_simulator::CONSENSUS_VERSION,"vm":chialisp_oxide_simulator::VM_VERSION,"source_identity":{"mutations":hex::encode(common::sha(&[include_bytes!("mutations.rs")])),"manifest":hex::encode(common::sha(&[include_bytes!("../Cargo.toml")])),"lockfile":hex::encode(common::sha(&[include_bytes!("../Cargo.lock")])),"main":hex::encode(common::sha(&[include_bytes!("main.rs")])),"common":hex::encode(common::sha(&[include_bytes!("common.rs")])),"batches":hex::encode(common::sha(&[include_bytes!("batches.rs")])),"spuds":hex::encode(common::sha(&[include_bytes!("spuds.rs")])),"fixture_keys":hex::encode(common::sha(&[include_bytes!("fixture_keys.rs")])),"measurement":hex::encode(common::sha(&[include_bytes!("../../spend-costs/src/measurement.rs")]))},"compiler_preservation_proof":false,"standard_wallet_integration":false,"reports":reports});
    fs::write(output, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_batch_two_complete_sdk_chains() -> Result<()> {
        let module = chialisp_oxide_reference::compile(include_str!(
            "../../continuation-batches/reference.clsp"
        ))?;
        for count in [4, 8] {
            let report = batches::run(&module, count, 2)?;
            ensure!(
                report["steps"].as_array().unwrap().len() == count / 2,
                "complete chain"
            );
            ensure!(report["terminal_successor"] == false, "terminal");
        }
        Ok(())
    }
    #[test]
    fn independent_spuds_signed_sdk_boundaries() -> Result<()> {
        let module = chialisp_oxide_reference::compile(include_str!("../../spuds/reference.clsp"))?;
        let report = spuds::run(&module)?;
        ensure!(
            report["steps"].as_array().unwrap().len() == 3,
            "takes and claim"
        );
        ensure!(
            report["controls"].as_array().unwrap().len() == 11,
            "all actual rejected submissions"
        );
        Ok(())
    }
}
