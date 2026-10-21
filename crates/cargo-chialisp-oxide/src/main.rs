//! Cargo orchestrates Rust compilation; chialisp-oxide-driver extracts annotated exports.
mod options;
mod registry;

use anyhow::{Context, Result, ensure};
use chialisp_oxide_backend_experiment::{RUE_REV, artifact};
use chialisp_oxide_ir::Program;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_root: PathBuf,
    target_directory: PathBuf,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    manifest_path: PathBuf,
    targets: Vec<Target>,
}

#[derive(Deserialize)]
struct Target {
    name: String,
    kind: Vec<String>,
}

fn toolchain() -> &'static str {
    include_str!("../../../rust-toolchain.toml")
        .lines()
        .find_map(|line| line.trim().strip_prefix("channel = "))
        .expect("pinned Rust channel")
        .trim_matches('"')
}

fn cargo() -> Command {
    let mut command = Command::new("rustup");
    command.args(["run", toolchain(), "cargo"]);
    command
}

fn successful(output: Output, context: &str) -> Result<Vec<u8>> {
    ensure!(
        output.status.success(),
        "{context}:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

fn pinned_rustc() -> Result<String> {
    let output = Command::new("rustup")
        .args(["run", toolchain(), "rustc", "-vV"])
        .output()
        .context("run rustup; install the pinned toolchain and rustc-dev first")?;
    Ok(String::from_utf8(successful(
        output,
        "query pinned rustc",
    )?)?)
}

fn compiler_path() -> Result<PathBuf> {
    let output = Command::new("rustup")
        .args(["which", "--toolchain", toolchain(), "rustc"])
        .output()?;
    Ok(PathBuf::from(
        String::from_utf8(successful(output, "locate pinned rustc")?)?.trim(),
    ))
}

fn reject_existing_wrappers() -> Result<()> {
    for name in [
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
    ] {
        ensure!(
            std::env::var_os(name).is_none_or(|value| value.is_empty()),
            "cargo chialisp-oxide cannot compose an existing {name} in this prototype; \
             run this command without that wrapper"
        );
    }
    // Read only the build table, never credentials or registry configuration.
    // The unstable query is coupled to the same pinned Cargo as compilation.
    let output = cargo()
        .args([
            "-Zunstable-options",
            "config",
            "get",
            "build",
            "--format",
            "json",
        ])
        .output()?;
    if !output.status.success() {
        ensure!(
            String::from_utf8_lossy(&output.stderr).contains("config value `build` is not set"),
            "could not inspect Cargo's build configuration"
        );
        return Ok(());
    }
    let config: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    for key in ["rustc-wrapper", "rustc-workspace-wrapper"] {
        ensure!(
            config["build"][key].as_str().is_none_or(str::is_empty),
            "cargo chialisp-oxide cannot compose configured build.{key} in this prototype; \
             remove that wrapper for this invocation"
        );
    }
    Ok(())
}

fn select<'a>(metadata: &'a Metadata, options: &options::Options) -> Result<&'a Package> {
    if let Some(name) = &options.package {
        let packages: Vec<_> = metadata
            .packages
            .iter()
            .filter(|p| &p.name == name)
            .collect();
        ensure!(
            packages.len() == 1,
            "--package must name one workspace package"
        );
        return Ok(packages[0]);
    }
    // Cargo locates the manifest when invoked in a member subdirectory. Do not
    // guess the default package of a virtual workspace or quietly compile all.
    let output = cargo()
        .args(["locate-project", "--message-format", "plain"])
        .args(
            options
                .manifest
                .iter()
                .flat_map(|path| ["--manifest-path".into(), path.as_os_str().to_owned()]),
        )
        .output()?;
    let manifest =
        PathBuf::from(String::from_utf8(successful(output, "locate Cargo manifest")?)?.trim());
    metadata
        .packages
        .iter()
        .find(|p| p.manifest_path == manifest)
        .context("virtual workspace: select one library package with -p NAME")
}

fn main() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if let Some(result) = registry::dispatch(&args) {
        return result;
    }
    let args = args
        .into_iter()
        .map(|arg| {
            arg.into_string().map_err(|_| {
                anyhow::anyhow!("build/check arguments must be UTF-8 in this prototype")
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let Some(options) = options::Options::parse(args)? else {
        usage();
        return Ok(());
    };
    reject_existing_wrappers()?;
    let driver = std::env::var_os("CHIALISP_OXIDE_DRIVER")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_exe()?.with_file_name("chialisp-oxide-driver"))
        .canonicalize()
        .context("build/install chialisp-oxide-driver beside cargo-chialisp-oxide, or set CHIALISP_OXIDE_DRIVER")?;
    let compiler = pinned_rustc()?;
    let rustc = compiler_path()?;
    let driver_hash = hex::encode(Sha256::digest(std::fs::read(&driver)?));
    let host = compiler
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .context("rustc host triple")?;
    let metadata: Metadata = serde_json::from_slice(&successful(
        cargo()
            .args(["metadata", "--format-version", "1", "--no-deps"])
            .args(&options.metadata)
            .env("RUSTC", &rustc)
            .output()?,
        "read Cargo metadata",
    )?)?;
    let package = select(&metadata, &options)?;
    let libraries: Vec<_> = package
        .targets
        .iter()
        .filter(|target| {
            target
                .kind
                .iter()
                .any(|kind| matches!(kind.as_str(), "lib" | "rlib"))
        })
        .collect();
    ensure!(
        libraries.len() == 1,
        "select a package containing one ordinary Rust library"
    );
    let crate_name = &libraries[0].name;
    let oxide_dir = metadata.target_directory.join("chialisp-oxide");
    std::fs::create_dir_all(&oxide_dir)?;
    // Serialize extraction, including the explicit root-package invalidation.
    // Cargo's individual command locks do not cover clean + check together.
    let build_lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(oxide_dir.join("build.lock"))?;
    build_lock.lock()?;
    // Callback policy changes are invisible to Cargo's dependency fingerprints.
    // A new driver binary must never reuse old dependency MIR metadata.
    let cargo_dir = oxide_dir.join("cargo").join(&driver_hash);
    let build_dir_config = format!("build.build-dir={}", serde_json::to_string(&cargo_dir)?);
    let nonce = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    );
    let work = oxide_dir.join("runs").join(&nonce);
    std::fs::create_dir_all(&work)?;
    // Cargo may consider an export crate fresh even after its .ir.json files
    // disappeared. Force just the selected package to re-run; dependencies keep
    // their separate extraction cache. Never consume an old IR directory.
    ensure!(
        cargo()
            .args([
                "clean",
                "--package",
                &package.name,
                "--target",
                host,
                "--target-dir"
            ])
            .arg(&cargo_dir)
            .args(["--config", &build_dir_config])
            .arg("--manifest-path")
            .arg(&package.manifest_path)
            .args(&options.clean)
            .env("RUSTC", &rustc)
            .status()?
            .success(),
        "clean selected extraction package"
    );
    let mut command = cargo();
    command
        .args(["check", "--lib", "--target", host, "--target-dir"])
        .arg(&cargo_dir)
        .args(["--config", &build_dir_config])
        .args(&options.cargo);
    if options.package.is_none() {
        command.args(["--package", &package.name]);
    }
    let status = command
        .env("RUSTC_WRAPPER", &driver)
        .env("RUSTC", &rustc)
        .env("CHIALISP_OXIDE_CARGO_MODE", "1")
        .env("CHIALISP_OXIDE_PRIMARY_PACKAGES", crate_name)
        .env("CHIALISP_OXIDE_OUTPUT_DIR", &work)
        .status()?;
    ensure!(
        status.success(),
        "Rust/CLVM checking failed; no new artifacts published"
    );
    let mut paths: Vec<_> = std::fs::read_dir(&work)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<_>>()?;
    paths.retain(|path| path.to_str().is_some_and(|path| path.ends_with(".ir.json")));
    paths.sort();
    ensure!(
        !paths.is_empty(),
        "no #[chialisp_oxide::puzzle] functions found in selected library"
    );
    let mut entries = vec![];
    for path in paths {
        let program: Program = serde_json::from_slice(&std::fs::read(&path)?)?;
        program.validate()?;
        let bytes = chialisp_oxide_ir::compile(&program, options.optimize)
            .with_context(|| format!("compile {} to CLVM", program.name))?;
        let compiled = artifact("rust_mir", options.optimize, &bytes)?;
        let filename = path.file_name().context("IR filename")?.to_string_lossy();
        let stem = filename.strip_suffix(".ir.json").context("IR suffix")?;
        std::fs::write(
            work.join(format!("{stem}.clvm.hex")),
            format!("{}\n", hex::encode(bytes)),
        )?;
        std::fs::write(
            work.join(format!("{stem}.clvm.txt")),
            format!("{}\n", compiled.disassembly),
        )?;
        std::fs::write(
            work.join(format!("{stem}.artifact.json")),
            serde_json::to_vec_pretty(&compiled)?,
        )?;
        entries.push(json!({"entry":program.name,"ir":filename,"clvm":format!("{stem}.clvm.hex"),"artifact":compiled}));
    }
    let manifest = json!({
        "schema_version":1,"status":"wip_prototype_not_ready_for_use",
        "package":package.name,"package_id":package.id,"manifest_path":package.manifest_path,"workspace_root":metadata.workspace_root,
        "compiler":compiler,"toolchain":toolchain(),"analysis_target":host,"driver_sha256":driver_hash,
        "rue_revision":RUE_REV,"cargo_options":options.cargo,"entries":entries,
    });
    std::fs::write(
        work.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    if options.check {
        println!(
            "Checked {} CLVM entry functions in {} (WIP prototype).",
            entries.len(),
            package.name
        );
        std::fs::remove_dir_all(work)?;
    } else {
        // Immutable bundle plus an atomically replaced manifest pointer keeps a
        // failed build from exposing a mixture of old and new artifacts.
        let output = options
            .output
            .unwrap_or_else(|| oxide_dir.join("artifacts"));
        std::fs::create_dir_all(&output)?;
        let bundle = output.join(&nonce);
        std::fs::create_dir(&bundle)?;
        for entry in std::fs::read_dir(&work)? {
            let entry = entry?;
            ensure!(
                entry.file_type()?.is_file(),
                "unexpected extraction subdirectory"
            );
            std::fs::copy(entry.path(), bundle.join(entry.file_name()))?;
        }
        let pointer = output.join(format!(".latest-{nonce}.json"));
        std::fs::write(
            &pointer,
            serde_json::to_vec_pretty(
                &json!({"bundle":nonce,"manifest":format!("{nonce}/manifest.json")}),
            )?,
        )?;
        std::fs::rename(pointer, output.join("latest.json"))?;
        std::fs::remove_dir_all(work)?;
        println!(
            "Built {} CLVM entry functions (WIP prototype).\nManifest: {}",
            entries.len(),
            bundle.join("manifest.json").display()
        );
    }
    Ok(())
}

fn usage() {
    println!(
        "WIP prototype — not ready for use. No crates are published.\n\
        cargo chialisp-oxide build|check [--lib] [-p NAME] [--manifest-path PATH]\n\
        \x20   [--features FEATURES] [--all-features] [--no-default-features]\n\
        \x20   [--locked|--offline|--frozen] [--release|--profile NAME] [-j N] [-v|-q]\n\
        \x20   [--no-optimize] [--output-dir PATH]\n\
        Compiles annotated functions in one Cargo library using the pinned Rust driver.\n\
        cargo check/test remain normal native Rust checks; cargo chialisp-oxide checks the CLVM subset.\n\
        cargo chialisp-oxide registry --help: offline package identities and explicit local storage."
    );
}
