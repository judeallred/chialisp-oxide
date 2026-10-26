use anyhow::{Context, Result, bail, ensure};
use chialisp_oxide_backend_experiment::{
    Outcome, RUE_LOCAL_PATCHES, RUE_REV, VM_FAILURE_PROTOCOL, artifact, execute,
};
use chialisp_oxide_ir::Program;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
mod batch;
fn driver_path() -> Result<PathBuf> {
    Ok(std::env::var_os("CHIALISP_OXIDE_DRIVER")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_exe()?.with_file_name("chialisp-oxide-driver")))
}
fn extract(source: &Path) -> Result<Program> {
    let driver = driver_path()?;
    let output = Command::new(&driver)
        .arg(source)
        .output()
        .with_context(|| {
            format!(
                "run {}; build chialisp-oxide-driver first",
                driver.display()
            )
        })?;
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
fn extract_corpus(
    authoring: &chialisp_oxide_corpus::Authoring,
) -> Result<cargo_chialisp_oxide::ArtifactBundle> {
    let compiler = std::env::current_exe()?.with_file_name("cargo-chialisp-oxide");
    let output = tempfile::tempdir()?;
    let status = Command::new(&compiler)
        .args(["build", "--locked", "--manifest-path"])
        .arg(chialisp_oxide_corpus::root().join(&authoring.manifest))
        .arg("--output-dir")
        .arg(output.path())
        .status()
        .with_context(|| {
            format!(
                "run {}; build cargo-chialisp-oxide first",
                compiler.display()
            )
        })?;
    ensure!(status.success(), "Cargo corpus extraction failed");
    let bundle = cargo_chialisp_oxide::load_bundle(output.path())?;
    ensure!(
        bundle.manifest["package"] == authoring.package,
        "unexpected Cargo corpus package"
    );
    let expected = authoring
        .exports
        .values()
        .map(|export| export.entry.clone())
        .collect::<std::collections::BTreeSet<_>>();
    ensure!(
        bundle
            .programs
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            == expected,
        "Cargo corpus exports do not match declared entries"
    );
    Ok(bundle)
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
                    .context("usage: chialisp-oxide build SOURCE.rs --output PREFIX")?,
            );
            let output = PathBuf::from(
                option(&args, "--output")?.unwrap_or_else(|| "target/chialisp-oxide/puzzle".into()),
            );
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let program = extract(source)?;
            let optimized = !args.iter().any(|a| a == "--no-optimize");
            let bytes = chialisp_oxide_ir::compile(&program, optimized)?;
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
                .context("usage: chialisp-oxide run PROGRAM.clvm.hex --solution '(...)'")?;
            let solution = option(&args, "--solution")?.context("--solution required")?;
            let max_cost = option(&args, "--max-cost")?
                .map(|s| s.parse())
                .transpose()?
                .unwrap_or(1_000_000);
            let bytes = hex::decode(std::fs::read_to_string(path)?.trim())?;
            let execution = execute(&bytes, &solution, max_cost)?;
            println!("{}", serde_json::to_string_pretty(&execution)?);
            if !matches!(execution.outcome, Outcome::Accepted { .. }) {
                std::process::exit(2);
            }
        }
        "run-batch" => {
            let program = args.get(1).context(
                "usage: chialisp-oxide run-batch PROGRAM.clvm.hex --solutions INPUTS.json",
            )?;
            let solutions = option(&args, "--solutions")?.context("--solutions required")?;
            let max_cost = option(&args, "--max-cost")?
                .map(|value| value.parse())
                .transpose()?
                .unwrap_or(chialisp_oxide_backend_experiment::MAX_COST);
            let output = batch::run(Path::new(program), Path::new(&solutions), max_cost)?;
            use std::io::Write;
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(&output)?;
            stdout.write_all(b"\n")?;
        }
        "validate" => {
            let path = PathBuf::from(
                option(&args, "--output")?.unwrap_or_else(|| "corpus/validation.json".into()),
            );
            let authoring = chialisp_oxide_corpus::authoring()?;
            let bundle = extract_corpus(&authoring)?;
            // `--only a,b` validates a subset for local iteration. The resulting
            // report is partial and is rejected by every corpus evidence gate.
            let only = option(&args, "--only")?
                .map(|list| list.split(',').map(str::to_owned).collect::<Vec<_>>());
            if let Some(only) = &only {
                for slug in only {
                    ensure!(
                        chialisp_oxide_corpus::PUZZLES.contains(&slug.as_str()),
                        "unknown corpus puzzle {slug}"
                    );
                }
            }
            let mut reports = vec![];
            for slug in chialisp_oxide_corpus::PUZZLES {
                if only
                    .as_ref()
                    .is_some_and(|only| !only.iter().any(|s| s == slug))
                {
                    continue;
                }
                eprintln!("Validating {slug}");
                let export = &authoring.exports[*slug];
                let program = &bundle.programs[&export.entry];
                reports.push(chialisp_oxide_corpus::validate(slug, program)?);
            }
            let compiler = Command::new(driver_path()?)
                .arg("--compiler-version")
                .output()?;
            ensure!(compiler.status.success(), "query active rustc version");
            let compiler = String::from_utf8(compiler.stdout)?;
            ensure!(
                bundle.manifest["compiler"]
                    .as_str()
                    .and_then(|version| version.lines().next())
                    == Some(compiler.trim()),
                "Cargo extraction and validation compiler versions differ"
            );
            ensure!(
                chialisp_oxide_corpus::authoring()? == authoring,
                "Cargo inputs changed during corpus validation"
            );
            let sidecars = chialisp_oxide_corpus::registry_sidecars(&mut reports)?;
            ensure!(
                chialisp_oxide_corpus::authoring()? == authoring,
                "Cargo inputs changed during registry protocol validation"
            );
            let report = json!({
                "schema_version": 1, "authoring": authoring,
                "toolchain": compiler.trim(), "rue_revision": RUE_REV,
                "rue_local_patches": RUE_LOCAL_PATCHES,
                "optimization_pipeline": {
                    "rust_mir": {"unoptimized": [], "optimized": ["typed_ir_constants", "typed_ir_success_facts", "typed_ir_constants", "typed_ir_intervals", "typed_ir_tree_guards", "typed_ir_helper_inlining", "runtime_frame_elision", "runtime_guarded_projection_forwarding", "lazy_integer_decoder", "rue_lir", "static_pair_codegen"]},
                    "rue_source": {"unoptimized": [], "optimized": ["rue_lir"]}
                },
                "execution_vm": "clvmr 0.21.0 MEMPOOL_MODE",
                "vm_failure_protocol": VM_FAILURE_PROTOCOL,
                "simulator": {"sdk": "chia-sdk-test 0.36.0", "consensus": "chia-consensus 0.36.1", "vm": "clvmr 0.16.2", "network": "TESTNET11", "rejection_protocol": chialisp_oxide_corpus::SIMULATOR_REJECTION_PROTOCOL},
                "puzzles": reports
            });
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if let Some(sidecars) = sidecars {
                let parent = path.parent().unwrap_or_else(|| Path::new("."));
                let directory = parent.join("registry-slot-protocol");
                if let Ok(metadata) = std::fs::symlink_metadata(&directory) {
                    ensure!(
                        metadata.file_type().is_dir(),
                        "registry sidecar destination must be a real directory"
                    );
                    let mut existing = std::collections::BTreeSet::new();
                    for (index, entry) in std::fs::read_dir(&directory)?.enumerate() {
                        ensure!(
                            index < 11,
                            "registry sidecar destination exceeds eleven entries"
                        );
                        existing.insert(entry?.file_name());
                    }
                    let expected = sidecars
                        .files
                        .keys()
                        .map(|name| Path::new(name).file_name().unwrap().to_os_string())
                        .collect::<std::collections::BTreeSet<_>>();
                    ensure!(
                        existing == expected,
                        "registry sidecar destination has an unexpected inventory"
                    );
                } else {
                    std::fs::create_dir(&directory)?;
                }
                for (name, bytes) in sidecars.files {
                    let destination = parent.join(name);
                    if let Ok(metadata) = std::fs::symlink_metadata(&destination) {
                        ensure!(
                            metadata.file_type().is_file(),
                            "registry sidecar must be a regular file"
                        );
                    }
                    use std::io::Write;
                    let mut staged = tempfile::NamedTempFile::new_in(&directory)?;
                    staged.write_all(&bytes)?;
                    staged.persist(destination)?;
                }
            }
            // Keep every evidence value while avoiding repeated indentation
            // consuming the bounded report transport as the corpus grows.
            let mut encoded = Vec::new();
            let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
            let mut serializer = serde_json::Serializer::with_formatter(&mut encoded, formatter);
            serde::Serialize::serialize(&report, &mut serializer)?;
            encoded.push(b'\n');
            std::fs::write(&path, encoded)?;
            eprintln!(
                "Validated {} fixtures; report {}",
                reports.len(),
                path.display()
            );
        }
        "native" => {
            let slug = args
                .get(1)
                .context("usage: chialisp-oxide native SLUG --solution '(...)'")?;
            let solution = option(&args, "--solution")?.context("--solution required")?;
            chialisp_oxide_corpus::authoring()?;
            let outcome = chialisp_oxide_corpus::native_execute(slug, &solution)?;
            println!("{}", serde_json::to_string_pretty(&outcome)?);
        }
        "--help" | "-h" | "help" => usage(),
        _ => bail!("unknown command {command}"),
    }
    Ok(())
}
fn usage() {
    println!(
        "chialisp-oxide build SOURCE.rs --output PREFIX [--no-optimize]\nchialisp-oxide run PROGRAM.clvm.hex --solution '(...)' [--max-cost N]\nchialisp-oxide run-batch PROGRAM.clvm.hex --solutions INPUTS.json [--max-cost N]\nchialisp-oxide validate --output corpus/validation.json\nchialisp-oxide native SLUG --solution '(...)'"
    );
}
