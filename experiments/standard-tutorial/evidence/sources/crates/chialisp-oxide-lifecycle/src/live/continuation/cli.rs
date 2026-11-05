use super::*;
use crate::{
    fresh::{FreshTools, continuation::certify_continuation},
    live::{network::Testnet11Client, wallet::WalletIdentity},
};
use std::{collections::BTreeMap, ffi::OsString, io::Read, path::Path};

pub fn run(arguments: &[OsString]) -> Result<()> {
    let op = arguments
        .first()
        .and_then(|v| v.to_str())
        .context("continuation operation required")?;
    ensure!(
        [
            "continuation-plan",
            "continuation-status",
            "continuation-prepare-funding",
            "continuation-submit",
            "continuation-prepare-step",
            "continuation-watch",
            "continuation-serve"
        ]
        .contains(&op),
        "unknown continuation operation"
    );
    let (pairs, tail) = arguments[1..].as_chunks::<2>();
    ensure!(tail.is_empty(), "each option requires a value");
    let mut options = BTreeMap::new();
    for pair in pairs {
        let name = pair[0].to_str().context("option UTF8")?;
        ensure!(
            [
                "--network",
                "--cargo",
                "--compiler",
                "--wallet-puzzle-hash",
                "--master-public-key",
                "--mode",
                "--output",
                "--outbox",
                "--recipients",
                "--fee-mojos",
                "--step-fee-mojos"
            ]
            .contains(&name)
                && options.insert(name, &pair[1]).is_none(),
            "unknown/duplicate continuation option"
        );
    }
    let get = |name| {
        options
            .get(name)
            .copied()
            .with_context(|| format!("missing {name}"))
    };
    ensure!(
        get("--network")? == "testnet11",
        "explicit --network testnet11 required"
    );
    let text = |name| -> Result<&str> { get(name)?.to_str().context("option UTF8") };
    ensure!(
        text("--wallet-puzzle-hash")?.len() == 64 && text("--master-public-key")?.len() == 96,
        "bounded public identity hex"
    );
    let wallet: Digest = hex::decode(text("--wallet-puzzle-hash")?)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("wallet hash requires 32 bytes"))?;
    let public: [u8; 48] = hex::decode(text("--master-public-key")?)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("public key requires 48 bytes"))?;
    let identity = WalletIdentity::from_master_public_key(public, wallet)?;
    let mode = match text("--mode")? {
        "off" => ContinuationMode::Off,
        "on" => ContinuationMode::On,
        _ => anyhow::bail!("mode off/on required"),
    };
    let recipient_path = Path::new(get("--recipients")?);
    let metadata = std::fs::symlink_metadata(recipient_path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 1024,
        "bounded public recipient file"
    );
    let mut bytes = vec![];
    std::fs::File::open(recipient_path)?
        .take(1025)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 1024, "recipient file grew beyond bound");
    let encoded: Vec<String> = serde_json::from_slice(&bytes)?;
    ensure!(
        (1..=8).contains(&encoded.len()),
        "1..8 recipient hashes required"
    );
    let recipients = encoded
        .into_iter()
        .map(|s| {
            ensure!(s.len() == 64, "recipient hash requires 32 bytes");
            hex::decode(s)?
                .try_into()
                .map_err(|_| anyhow::anyhow!("recipient hash"))
        })
        .collect::<Result<Vec<Digest>>>()?;
    let tools = FreshTools::new(Path::new(get("--cargo")?), Path::new(get("--compiler")?))?;
    let recipe = certify_continuation(&tools, Path::new(get("--output")?), mode, recipients)?;
    if op == "continuation-plan" {
        println!("{}", serde_json::to_string_pretty(recipe.plan())?);
        return Ok(());
    }
    let store = ContinuationStore::new(Path::new(get("--outbox")?))?;
    let funding_fee = options
        .get("--fee-mojos")
        .map(|v| {
            v.to_str()
                .context("fee UTF8")?
                .parse::<u64>()
                .context("fee integer")
        })
        .transpose()?
        .unwrap_or(10000);
    let step_fee = options
        .get("--step-fee-mojos")
        .map(|v| {
            v.to_str()
                .context("fee UTF8")?
                .parse::<u64>()
                .context("fee integer")
        })
        .transpose()?
        .unwrap_or(0);
    let mut session = if store.path().exists() {
        let session = ContinuationSession::load(recipe, identity, store)?;
        ensure!(
            session.outbox().binding.funding_fee == funding_fee
                && session.outbox().binding.step_fee == step_fee,
            "fees differ from immutable journal"
        );
        session
    } else {
        ensure!(
            op == "continuation-prepare-funding" || op == "continuation-serve",
            "create with prepare-funding or serve"
        );
        ContinuationSession::create(recipe, identity, store, funding_fee, step_fee)?
    };
    if op == "continuation-serve" {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        return rt.block_on(async move {
            let server = crate::live::http::LiveLoopbackServer::bind_continuation(session).await?;
            println!("{}", server.url());
            server
                .run_until(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await
        });
    }
    if op != "continuation-status" {
        let mut network = Testnet11Client::new()?;
        match op {
            "continuation-prepare-funding" => session.prepare_funding(&mut network)?,
            "continuation-prepare-step" => session.prepare_step(&mut network)?,
            "continuation-submit" => session.submit(&mut network)?,
            "continuation-watch" => {
                session.monitor(&mut network)?;
            }
            _ => unreachable!(),
        }
    }
    println!("{}", serde_json::to_string_pretty(&session.view()?)?);
    Ok(())
}
