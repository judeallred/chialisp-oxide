use anyhow::{Context, Result, bail, ensure};
use oxide_backend_experiment::{Outcome, RUE_REV, artifact, execute};
use oxide_ir::Program;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
fn extract(source: &Path) -> Result<Program> {
    let driver = std::env::var_os("OXIDE_DRIVER")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_exe()?.with_file_name("oxide-driver"));
    let output = Command::new(&driver)
        .arg(source)
        .output()
        .with_context(|| format!("run {}; build oxide-driver first", driver.display()))?;
    ensure!(
        output.status.success(),
        "Rust compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let program: Program =
        serde_json::from_slice(&output.stdout).context("decode typed compiler IR")?;
    program.validate()?;
    Ok(program)
}
fn option(args: &[String], key: &str) -> Result<Option<String>> {
    if let Some(index) = args.iter().position(|a| a == key) {
        Ok(Some(
            args.get(index + 1).context("missing option value")?.clone(),
        ))
    } else {
        Ok(None)
    }
}
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let Some(command) = args.first().map(String::as_str) else {
        usage();
        return Ok(());
    };
    match command {
        "build" => {
            let source = Path::new(
                args.get(1)
                    .context("usage: oxide build SOURCE.rs --output PREFIX")?,
            );
            let output = PathBuf::from(
                option(&args, "--output")?.unwrap_or_else(|| "target/oxide/puzzle".into()),
            );
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let program = extract(source)?;
            let optimized = !args.iter().any(|a| a == "--no-optimize");
            let bytes = oxide_ir::compile(&program, optimized)?;
            let artifact = artifact("rust_mir", optimized, &bytes)?;
            let prefix = output.to_string_lossy();
            std::fs::write(
                format!("{prefix}.ir.json"),
                serde_json::to_string_pretty(&program)?,
            )?;
            std::fs::write(
                format!("{prefix}.clvm.hex"),
                format!("{}\n", hex::encode(bytes)),
            )?;
            std::fs::write(
                format!("{prefix}.clvm.txt"),
                format!("{}\n", artifact.disassembly),
            )?;
            std::fs::write(
                format!("{prefix}.artifact.json"),
                serde_json::to_string_pretty(&artifact)?,
            )?;
            println!("{}", serde_json::to_string_pretty(&artifact)?);
        }
        "run" => {
            let path = args
                .get(1)
                .context("usage: oxide run PROGRAM.clvm.hex --solution '(...)'")?;
            let solution = option(&args, "--solution")?.context("--solution required")?;
            let max_cost = option(&args, "--max-cost")?
                .map(|s| s.parse())
                .transpose()?
                .unwrap_or(1_000_000);
            let bytes = hex::decode(std::fs::read_to_string(path)?.trim())?;
            let execution = execute(&bytes, &solution, max_cost)?;
            println!("{}", serde_json::to_string_pretty(&execution)?);
            if execution.outcome == Outcome::Rejected {
                std::process::exit(2);
            }
        }
        "validate" => {
            let path = PathBuf::from(
                option(&args, "--output")?.unwrap_or_else(|| "corpus/validation.json".into()),
            );
            let mut reports = vec![];
            for slug in oxide_corpus::PUZZLES {
                eprintln!("Validating {slug}");
                let program = extract(
                    &oxide_corpus::root()
                        .join("puzzles")
                        .join(slug)
                        .join("puzzle.rs"),
                )?;
                reports.push(oxide_corpus::validate(slug, &program)?);
            }
            let report = json!({"schema_version":1,"toolchain":"rustc 1.97.1","rue_revision":RUE_REV,"rue_local_patches":["https://github.com/xch-dev/rue/issues/108","https://github.com/xch-dev/rue/issues/109"],"execution_vm":"clvmr 0.21.0 MEMPOOL_MODE","simulator":{"sdk":"chia-sdk-test 0.36.0","consensus":"chia-consensus 0.36.1","vm":"clvmr 0.16.2","network":"TESTNET11"},"puzzles":reports});
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, serde_json::to_string_pretty(&report)? + "\n")?;
            eprintln!(
                "Validated {} fixtures; report {}",
                reports.len(),
                path.display()
            );
        }
        "--help" | "-h" | "help" => usage(),
        _ => bail!("unknown command {command}"),
    }
    Ok(())
}
fn usage() {
    println!(
        "oxide build SOURCE.rs --output PREFIX [--no-optimize]\noxide run PROGRAM.clvm.hex --solution '(...)' [--max-cost N]\noxide validate --output corpus/validation.json"
    );
}
