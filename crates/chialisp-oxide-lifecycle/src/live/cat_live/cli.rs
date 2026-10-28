//! Explicit commands for the closed funding→Eve→normal CAT lifecycle only.
use super::*;
use crate::fresh::FreshTools;
use std::{collections::BTreeMap, ffi::OsString, path::Path};

pub(in crate::live) fn run(arguments: &[OsString]) -> Result<()> {
    let operation = arguments
        .first()
        .and_then(|s| s.to_str())
        .context("CAT operation required")?;
    ensure!(
        matches!(
            operation,
            "cat-plan"
                | "cat-prepare-funding"
                | "cat-submit-funding"
                | "cat-monitor-funding"
                | "cat-prepare-issuance"
                | "cat-submit-issuance"
                | "cat-monitor-issuance"
                | "cat-status"
        ),
        "unknown CAT operation"
    );
    let (pairs, tail) = arguments[1..].as_chunks::<2>();
    ensure!(tail.is_empty(), "every CAT option requires one value");
    let mut options = BTreeMap::new();
    for pair in pairs {
        let key = pair[0].to_str().context("CAT option UTF-8")?;
        ensure!(
            matches!(
                key,
                "--network"
                    | "--cargo"
                    | "--compiler"
                    | "--wallet-puzzle-hash"
                    | "--master-public-key"
                    | "--mode"
                    | "--output"
                    | "--outbox"
                    | "--fee-mojos"
                    | "--issuance-fee-mojos"
            ) && options.insert(key, &pair[1]).is_none(),
            "unknown or duplicate CAT option"
        );
    }
    let required = |key| {
        options
            .get(key)
            .copied()
            .with_context(|| format!("missing {key}"))
    };
    ensure!(
        required("--network")? == "testnet11",
        "CAT requires explicit Testnet11"
    );
    let hash = decode_hex(
        required("--wallet-puzzle-hash")?
            .to_str()
            .context("wallet hash UTF-8")?,
        32,
    )?;
    let wallet: Digest = hash
        .as_slice()
        .try_into()
        .context("wallet hash dimension")?;
    let master = decode_hex(
        required("--master-public-key")?
            .to_str()
            .context("master public key UTF-8")?,
        48,
    )?;
    let master: [u8; 48] = master
        .as_slice()
        .try_into()
        .context("master public key dimension")?;
    WalletIdentity::from_master_public_key(master, wallet)?;
    let mode = match required("--mode")?.to_str() {
        Some("off") => CatMode::RustOff,
        Some("on") => CatMode::RustOn,
        _ => anyhow::bail!("CAT mode must be off or on"),
    };
    let fee = parse_fee(options.get("--fee-mojos").copied())?;
    let issuance_fee = parse_fee(options.get("--issuance-fee-mojos").copied())?;
    let store = CatStore::new(Path::new(required("--outbox")?))?;
    if operation != "cat-plan" {
        store.load()?;
    }
    let tools = FreshTools::new(
        Path::new(required("--cargo")?),
        Path::new(required("--compiler")?),
    )?;
    let (authority, _report) =
        crate::fresh::cat::certify_cat(&tools, Path::new(required("--output")?), mode)?;
    let recipe = CatRecipe::new(authority, master, wallet, fee, issuance_fee)?;
    let mut session = if operation == "cat-plan" {
        CatSession::create(recipe, store)?
    } else {
        CatSession::load(recipe, store)?
    };
    if operation != "cat-plan" {
        let mut network = network::Testnet11Client::new()?;
        match operation {
            "cat-prepare-funding" => session.prepare_funding(&mut network)?,
            "cat-submit-funding" => session.submit(&mut network, CatStage::Funding)?,
            "cat-monitor-funding" => {
                session.monitor(&mut network, CatStage::Funding)?;
            }
            "cat-prepare-issuance" => session.prepare_issuance(&mut network)?,
            "cat-submit-issuance" => session.submit(&mut network, CatStage::Issuance)?,
            "cat-monitor-issuance" => {
                session.monitor(&mut network, CatStage::Issuance)?;
            }
            "cat-status" => session.status(&mut network)?,
            _ => unreachable!("closed CAT operation inventory"),
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "protocol":"chialisp-oxide-testnet11-cat-live-view/v1","operation":operation,
            "scope":"WIP Testnet11 only. Actual current-process source gate, full local BLS/consensus preflight and provider consistency observations; no chain authentication/finality or production-readiness claim. Public signed outbox has no secrets. Fixed same-wallet recipient and change; no transfer commands.",
            "fresh_execution":"executed_this_process","funding_state":session.outbox().state(CatStage::Funding),
            "issuance_state":session.outbox().state(CatStage::Issuance),"outbox":session.outbox()
        }))?
    );
    Ok(())
}
fn parse_fee(value: Option<&OsString>) -> Result<u64> {
    let fee = value
        .map(|s| -> Result<u64> {
            let s = s.to_str().context("fee UTF-8")?;
            ensure!(
                !s.is_empty() && s.len() <= 7 && s.bytes().all(|b| b.is_ascii_digit()),
                "decimal mojo fee"
            );
            Ok(s.parse()?)
        })
        .transpose()?
        .unwrap_or(DEFAULT_FEE);
    ensure!(fee <= MAX_FEE, "CAT fee exceeds one-million-mojo cap");
    Ok(fee)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_network_secret_options_and_transfers_fail_before_tools() {
        for args in [
            vec!["cat-transfer"],
            vec!["cat-plan", "--network", "mainnet"],
            vec!["cat-plan", "--secret-key", "never-read"],
            vec![
                "cat-plan",
                "--network",
                "testnet11",
                "--network",
                "testnet11",
            ],
        ] {
            assert!(run(&args.into_iter().map(OsString::from).collect::<Vec<_>>()).is_err());
        }
        assert_eq!(parse_fee(None).unwrap(), 10_000);
        assert!(parse_fee(Some(&OsString::from("1000001"))).is_err());
    }
}
