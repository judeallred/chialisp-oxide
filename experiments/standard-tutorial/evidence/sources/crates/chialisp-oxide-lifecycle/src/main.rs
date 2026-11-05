//! Thin command adapters; shared runners own the validated state transitions.
use anyhow::{Context, Result, ensure};
use chialisp_oxide_lifecycle::{
    Stage,
    runner::{Command, SimulatorRunner},
    store::JournalStore,
};
use std::path::PathBuf;

fn main() -> Result<()> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.first().is_some_and(|argument| argument == "live") {
        return chialisp_oxide_lifecycle::live::cli::run(&arguments[1..]);
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "fresh")
    {
        return fresh_workflow(&arguments[1..]);
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "simulator")
        && arguments
            .get(1)
            .is_some_and(|argument| argument == "check-fresh")
    {
        return fresh(&arguments[2..]);
    }
    ensure!(
        arguments.len() >= 2 && arguments[0] == "simulator",
        "usage: chialisp-oxide-lifecycle simulator run|status|resume|serve [--journal FILE.lifecycle.json] [--steps 1..7]"
    );
    let operation = arguments[1].to_str().context("operation must be UTF-8")?;
    ensure!(
        matches!(operation, "run" | "status" | "resume" | "serve"),
        "unsupported simulator command"
    );
    let mut path = None;
    let mut steps = None;
    let (options, remainder) = arguments[2..].as_chunks::<2>();
    for pair in options {
        match pair[0].to_str() {
            Some("--journal") => {
                ensure!(path.is_none(), "duplicate journal option");
                path = Some(PathBuf::from(&pair[1]));
            }
            Some("--steps") => {
                ensure!(
                    steps.is_none() && !matches!(operation, "status" | "serve"),
                    "duplicate or unsupported steps option"
                );
                let count = pair[1]
                    .to_str()
                    .context("steps must be UTF-8")?
                    .parse::<usize>()?;
                ensure!(
                    (1..=7).contains(&count),
                    "steps must be between one and seven"
                );
                steps = Some(count);
            }
            _ => anyhow::bail!("unsupported simulator option"),
        }
    }
    ensure!(remainder.is_empty(), "every option requires a value");
    let path = if let Some(path) = path {
        path
    } else {
        match std::fs::create_dir("target") {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error.into()),
        }
        PathBuf::from("target/lifecycle-simulator.lifecycle.json")
    };
    let store = JournalStore::new(path)?;
    if operation == "serve" {
        return tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?
            .block_on(async move {
                let server = chialisp_oxide_lifecycle::http::LoopbackServer::bind(store).await?;
                // Only the plain loopback URL reaches stdout; never the capability.
                println!("{}", server.url());
                std::io::Write::flush(&mut std::io::stdout())?;
                server
                    .run_until(async {
                        let _ = tokio::signal::ctrl_c().await;
                    })
                    .await
            });
    }
    let (mut runner, mut expected) = if operation == "run" {
        let runner = SimulatorRunner::new()?;
        let expected = store.create(runner.journal())?;
        (runner, expected)
    } else {
        let stored = store.load()?;
        (SimulatorRunner::resume(stored.journal)?, stored.byte_digest)
    };
    if operation != "status" {
        for _ in 0..steps.unwrap_or(7) {
            if runner.journal().stage() == Stage::Confirmed {
                break;
            }
            runner.execute(Command::Advance)?;
            expected = store.commit(Some(expected), runner.journal())?;
        }
    }
    println!("{}", serde_json::to_string_pretty(&runner.view()?)?);
    Ok(())
}

fn fresh(arguments: &[std::ffi::OsString]) -> Result<()> {
    let mut options = std::collections::BTreeMap::new();
    let (pairs, rest) = arguments.as_chunks::<2>();
    ensure!(rest.is_empty(), "each fresh option requires a value");
    for pair in pairs {
        let name = pair[0].to_str().context("fresh option must be UTF-8")?;
        ensure!(
            matches!(
                name,
                "--cargo" | "--compiler" | "--destination-hex" | "--output"
            ) && options.insert(name, &pair[1]).is_none(),
            "unknown or duplicate fresh option"
        );
    }
    let required = |name| {
        options
            .get(name)
            .copied()
            .with_context(|| format!("missing {name}"))
    };
    let destination = required("--destination-hex")?
        .to_str()
        .context("destination hex must be UTF-8")?;
    ensure!(
        destination.len() == 64
            && destination
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "destination requires32bytes lowercase hex"
    );
    let mut bytes = [0; 32];
    hex::decode_to_slice(destination, &mut bytes)?;
    let tools = chialisp_oxide_lifecycle::fresh::FreshTools::new(
        std::path::Path::new(required("--cargo")?),
        std::path::Path::new(required("--compiler")?),
    )?;
    let report = chialisp_oxide_lifecycle::fresh::check_fresh(
        &tools,
        bytes,
        std::path::Path::new(required("--output")?),
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// Fresh execution happens before opening the GUI or loading journal events.
/// Every invocation needs a new capture directory; no report reader grants
/// compiler execution credit on resume.
fn fresh_workflow(arguments: &[std::ffi::OsString]) -> Result<()> {
    use chialisp_oxide_lifecycle::fresh::{FreshTools, PaymentMode, check_fresh_recipe};
    let operation = arguments.first().and_then(|argument| argument.to_str())
        .context("usage: fresh run|status|resume|serve --cargo FILE --compiler FILE --destination-hex HEX --mode off|on --output NEWDIR --journal FILE.lifecycle.json [--steps 1..7]")?;
    ensure!(
        matches!(operation, "run" | "status" | "resume" | "serve"),
        "unsupported fresh operation"
    );
    let mut options = std::collections::BTreeMap::new();
    let (pairs, rest) = arguments[1..].as_chunks::<2>();
    ensure!(
        rest.is_empty(),
        "each fresh workflow option requires a value"
    );
    for pair in pairs {
        let name = pair[0].to_str().context("option UTF-8")?;
        ensure!(
            matches!(
                name,
                "--cargo"
                    | "--compiler"
                    | "--destination-hex"
                    | "--mode"
                    | "--output"
                    | "--journal"
                    | "--steps"
            ) && options.insert(name, &pair[1]).is_none(),
            "unknown or duplicate fresh workflow option"
        );
    }
    let required = |name| {
        options
            .get(name)
            .copied()
            .with_context(|| format!("missing {name}"))
    };
    let mode = match required("--mode")?.to_str() {
        Some("off") => PaymentMode::Off,
        Some("on") => PaymentMode::On,
        _ => anyhow::bail!("mode must be off or on"),
    };
    let destination = required("--destination-hex")?
        .to_str()
        .context("destination UTF-8")?;
    ensure!(
        destination.len() == 64
            && destination
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "destination requires32bytes lowercase hex"
    );
    let mut bytes = [0; 32];
    hex::decode_to_slice(destination, &mut bytes)?;
    let steps = options
        .get("--steps")
        .map(|value| -> Result<usize> {
            ensure!(
                matches!(operation, "run" | "resume"),
                "steps only apply to run/resume"
            );
            let steps = value.to_str().context("steps UTF-8")?.parse()?;
            ensure!(
                (1..=7).contains(&steps),
                "steps must be between one and seven"
            );
            Ok(steps)
        })
        .transpose()?
        .unwrap_or(7);
    let store = JournalStore::new(PathBuf::from(required("--journal")?))?;
    let tools = FreshTools::new(
        std::path::Path::new(required("--cargo")?),
        std::path::Path::new(required("--compiler")?),
    )?;
    let recipe = check_fresh_recipe(
        &tools,
        bytes,
        std::path::Path::new(required("--output")?),
        mode,
    )?;
    if operation == "serve" {
        return tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?
            .block_on(async move {
                let server =
                    chialisp_oxide_lifecycle::http::LoopbackServer::bind_with_recipe(store, recipe)
                        .await?;
                println!("{}", server.url());
                std::io::Write::flush(&mut std::io::stdout())?;
                server
                    .run_until(async {
                        let _ = tokio::signal::ctrl_c().await;
                    })
                    .await
            });
    }
    let (mut runner, mut expected) = if operation == "run" {
        let runner = recipe.start()?;
        let expected = store.create(runner.journal())?;
        (runner, expected)
    } else {
        let stored = store.load()?;
        (recipe.resume(stored.journal)?, stored.byte_digest)
    };
    if operation != "status" {
        for _ in 0..steps {
            if runner.journal().stage() == Stage::Confirmed {
                break;
            }
            runner.execute(Command::Advance)?;
            expected = store.commit(Some(expected), runner.journal())?;
        }
    }
    println!("{}", serde_json::to_string_pretty(&runner.view()?)?);
    Ok(())
}
