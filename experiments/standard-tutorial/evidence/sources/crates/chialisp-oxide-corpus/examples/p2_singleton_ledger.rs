//! Reproduce the same canonical protocol harness used by corpus validation.
use anyhow::{Result, ensure};
#[allow(dead_code)]
#[path = "../src/p2_singleton.rs"]
mod p2_singleton;
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        args.len() == 2,
        "usage: p2_singleton_ledger ARTIFACTS.json OUTPUT.json"
    );
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let artifacts: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(&args[0])?)?;
    let report = p2_singleton::validate_artifacts(&root, &artifacts)?;
    std::fs::write(&args[1], serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
