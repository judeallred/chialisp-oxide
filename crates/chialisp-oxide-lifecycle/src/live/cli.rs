//! Explicit live CLI adapter. Every invocation checks the source recipe afresh;
//! none imports validation credit or receives a key through argv/environment.
use super::*;
use crate::fresh::{FreshTools, PaymentMode, check_fresh_recipe};
use std::{collections::BTreeMap, ffi::OsString, path::Path};

pub fn run(arguments: &[OsString]) -> Result<()> {
    let operation = arguments.first().and_then(|value| value.to_str()).context(
        "usage: live cat-check|plan|status|prepare-funding|submit-funding|monitor-funding|prepare-payment|submit-payment|monitor-payment --network testnet11 --cargo FILE --compiler FILE --wallet-puzzle-hash HEX --master-public-key HEX --mode off|on --output NEWDIR [--outbox FILE.live.json] [--fee-mojos N] [--payment-fee-mojos N]")?;
    ensure!(
        matches!(
            operation,
            "cat-check"
                | "plan"
                | "status"
                | "prepare-funding"
                | "submit-funding"
                | "monitor-funding"
                | "prepare-payment"
                | "submit-payment"
                | "monitor-payment"
        ),
        "unknown live operation"
    );
    let (pairs, tail) = arguments[1..].as_chunks::<2>();
    ensure!(tail.is_empty(), "each live option requires one value");
    let mut options = BTreeMap::new();
    for pair in pairs {
        let key = pair[0].to_str().context("live option UTF-8")?;
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
                    | "--payment-fee-mojos"
            ) && options.insert(key, &pair[1]).is_none(),
            "unknown or duplicate live option"
        );
    }
    let required = |name| {
        options
            .get(name)
            .copied()
            .with_context(|| format!("missing {name}"))
    };
    ensure!(
        required("--network")? == "testnet11",
        "live CLI requires explicit --network testnet11"
    );
    let wallet = parse_digest(
        required("--wallet-puzzle-hash")?
            .to_str()
            .context("wallet hash UTF-8")?,
    )?;
    let master = decode_hex(
        required("--master-public-key")?
            .to_str()
            .context("public key UTF-8")?,
        48,
    )?;
    ensure!(master.len() == 48, "master public key requires 48 bytes");
    // Reject identity mismatches before compiler execution, network or any TTY.
    wallet::WalletIdentity::from_master_public_key(master.as_slice().try_into()?, wallet)?;
    let mode = match required("--mode")?.to_str() {
        Some("off") => PaymentMode::Off,
        Some("on") => PaymentMode::On,
        _ => anyhow::bail!("live mode must be off or on"),
    };
    if operation == "cat-check" {
        ensure!(
            ["--outbox", "--fee-mojos", "--payment-fee-mojos"]
                .iter()
                .all(|key| !options.contains_key(key)),
            "cat-check takes no outbox or transaction fee options"
        );
        crate::fresh::require_native_environment()?;
        let tools = FreshTools::new(
            Path::new(required("--cargo")?),
            Path::new(required("--compiler")?),
        )?;
        let selected = match mode {
            PaymentMode::Off => super::cat::CatMode::RustOff,
            PaymentMode::On => super::cat::CatMode::RustOn,
        };
        let report =
            crate::fresh::cat::check_cat(&tools, Path::new(required("--output")?), selected)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    let fee = parse_fee(options.get("--fee-mojos").copied())?;
    let payment_fee = parse_fee(options.get("--payment-fee-mojos").copied())?;
    let store = LiveStore::new(Path::new(required("--outbox")?))?;
    // Validate malformed/unknown/aliased outbox bytes before expensive fresh
    // execution or network use. Fresh recipe binding is checked again below.
    if operation != "plan" {
        store.load()?;
    }
    let tools = FreshTools::new(
        Path::new(required("--cargo")?),
        Path::new(required("--compiler")?),
    )?;
    let fresh = check_fresh_recipe(&tools, wallet, Path::new(required("--output")?), mode)?;
    let recipe = LiveRecipe::from_fresh(
        &fresh,
        master.as_slice().try_into()?,
        wallet,
        fee,
        payment_fee,
    )?;
    let mut session = if operation == "plan" {
        LiveSession::create(recipe, store)?
    } else {
        LiveSession::load(recipe, store)?
    };
    if operation != "plan" {
        let mut client = network::Testnet11Client::new()?;
        match operation {
            "prepare-funding" => session.prepare_funding(&mut client)?,
            "submit-funding" => session.submit(&mut client, TransferKind::Funding)?,
            "monitor-funding" => {
                session.monitor(&mut client, TransferKind::Funding)?;
            }
            "prepare-payment" => session.prepare_payment(&mut client)?,
            "submit-payment" => session.submit(&mut client, TransferKind::Payment)?,
            "monitor-payment" => {
                session.monitor(&mut client, TransferKind::Payment)?;
            }
            "status" => {
                let identity = client.verify_network()?;
                let tip = client.synced_tip()?;
                session.verify_retained(&identity, tip.height)?;
            }
            _ => unreachable!("closed operation inventory"),
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "protocol":"chialisp-oxide-testnet11-live-view/v1", "operation":operation,
        "scope":"WIP Testnet11 only. Fresh current-process recipe checks and local SDK preflight; provider observations are not authenticated inclusion or finality. Local bundle preflight does not establish coinbase maturity or full chain eligibility. Public outbox contains exact signed bytes for explicit reconciliation/retry, no secrets.",
            "fresh_execution":"executed_this_process", "funding_puzzle_hash":hex::encode(session.funding_puzzle_hash()?),
            "funding_amount_mojos":1000, "funding_fee_mojos":fee, "payment_fee_mojos":payment_fee,
            "payment_fee_scope":if payment_fee == 0 { "explicit single-input permissionless zero-fee payment" }
                else { "separate same-wallet sponsor input, exact payment child and wallet change" },
        "retained_funding_state":session.outbox().state(TransferKind::Funding),
        "retained_payment_state":session.outbox().state(TransferKind::Payment),
        "status_scope":"status rechecks fresh source/SDK intent and provider labels/tip; use monitor-* to refresh retained spend effects", "outbox":session.outbox()
        }))?
    );
    Ok(())
}

fn parse_digest(value: &str) -> Result<Digest> {
    let bytes = decode_hex(value, 32)?;
    ensure!(bytes.len() == 32, "puzzle hash requires 32 bytes");
    Ok(bytes.as_slice().try_into()?)
}
fn parse_fee(value: Option<&OsString>) -> Result<u64> {
    let fee = value
        .map(|value| -> Result<u64> {
            let raw = value.to_str().context("fee UTF-8")?;
            ensure!(
                !raw.is_empty() && raw.len() <= 7 && raw.bytes().all(|byte| byte.is_ascii_digit()),
                "decimal mojo fee"
            );
            Ok(raw.parse()?)
        })
        .transpose()?
        .unwrap_or(DEFAULT_FEE);
    ensure!(fee <= MAX_FEE, "fee exceeds one-million-mojo cap");
    Ok(fee)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unknown_options_network_and_secret_options_before_tools() {
        for args in [
            vec!["plan", "--network", "mainnet"],
            vec!["plan", "--secret-key", "never-read"],
            vec!["publish"],
            vec!["plan", "--network", "testnet11", "--network", "testnet11"],
        ] {
            let args = args.into_iter().map(OsString::from).collect::<Vec<_>>();
            assert!(run(&args).is_err());
        }
        assert!(parse_fee(Some(&OsString::from("1000001"))).is_err());
        assert_eq!(parse_fee(None).unwrap(), 10_000);
        assert_eq!(parse_fee(Some(&OsString::from("0"))).unwrap(), 0);
    }
}
