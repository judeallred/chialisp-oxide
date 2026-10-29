//! Fresh trusted Cargo recipe checks. This adapter is separate from the
//! historical retained simulator runner. It does not submit network traffic.
use anyhow::{Context, Result, ensure};
use chialisp_oxide_package::{
    Limits,
    artifact::{Encoding, VerifiedArtifact, inspect_artifact},
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    process::Command,
};

pub(crate) mod cat;
mod oracle;
mod process;
mod receipt;
mod validation;
pub use process::ProcessReceipt;
pub use validation::{SimulatorObservation, VmObservation, VmOutcome};
include!(concat!(env!("OUT_DIR"), "/native_inputs.rs"));

const MAX_COST: u64 = 10_000_000;
const MAX_FILE: usize = 4 * 1024 * 1024;
const PACKAGE: &str = "chialisp-oxide-lifecycle-payment";
const EXPORT: &str = "chialisp_oxide_lifecycle_payment::pay";
const TOOLCHAIN: &str = "nightly-2026-07-14";
const REFERENCE: &str = include_str!("../../recipe/reference.clsp");

fn limits() -> Limits {
    Limits {
        max_blob_bytes: 1024 * 1024,
        max_nodes: 65_536,
        max_depth: 512,
        max_atom_bytes: 1024 * 1024,
        max_atom_heap_bytes: 2 * 1024 * 1024,
        max_work: 16 * 1024 * 1024,
        max_expanded_bytes: 1024 * 1024,
        ..Limits::default()
    }
}
fn inspect(bytes: &[u8]) -> Result<VerifiedArtifact> {
    Ok(inspect_artifact(bytes, Encoding::Plain, &limits())?)
}
fn read(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= maximum as u64,
        "bounded regular file required: {}",
        path.display()
    );
    let mut bytes = vec![];
    fs::File::open(path)?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= maximum, "file grew beyond bound");
    Ok(bytes)
}
fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}
pub(crate) fn require_native_inputs() -> Result<()> {
    require_native_environment()?;
    require_inputs_at(&source_root(), NATIVE_INPUTS)
}
pub(crate) fn require_native_environment() -> Result<()> {
    for name in NATIVE_FORBIDDEN_ENV {
        ensure!(
            std::env::var_os(name).is_none(),
            "native BigDecimal configuration must remain unset: {name}"
        );
    }
    Ok(())
}
fn require_inputs_at(root: &Path, inputs: &[(&str, &str)]) -> Result<()> {
    const MAX_TOTAL: u64 = 64 * 1024 * 1024;
    ensure!(inputs.len() <= 1024, "native source row bound");
    let mut remaining = MAX_TOTAL;
    // Reserve the complete declared closure before reading any file body.
    for (relative, _) in inputs {
        ensure!(
            !Path::new(relative).is_absolute()
                && Path::new(relative)
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))),
            "confined native input path"
        );
        let metadata = fs::symlink_metadata(root.join(relative))?;
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() <= MAX_FILE as u64
                && metadata.len() <= remaining,
            "native source file/aggregate byte bound"
        );
        remaining -= metadata.len();
    }
    let mut remaining = MAX_TOTAL;
    for (relative, expected) in inputs {
        let bytes = read(&root.join(relative), MAX_FILE.min(remaining as usize))?;
        remaining -= bytes.len() as u64;
        ensure!(
            hex::encode(crate::digest(&bytes)) == *expected,
            "native linked library/source inputs changed; rebuild lifecycle binary"
        );
    }
    Ok(())
}

fn fingerprint(path: &Path) -> Result<String> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "executable must be regular"
    );
    ensure!(
        metadata.len() <= 512 * 1024 * 1024,
        "executable/library byte bound"
    );
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = 0usize;
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count)
            .context("executable length overflow")?;
        ensure!(bytes <= 512 * 1024 * 1024, "executable byte bound");
        hash.update(&buffer[..count]);
    }
    Ok(hex::encode(hash.finalize()))
}

/// Explicit installed executables, not shell fragments or compiler overrides.
/// The production Cargo adapter still enforces its pinned rustc host contract.
pub struct FreshTools {
    cargo: PathBuf,
    rustc: PathBuf,
    compiler: PathBuf,
    driver: PathBuf,
}
impl FreshTools {
    pub fn new(cargo: &Path, compiler: &Path) -> Result<Self> {
        let cargo = cargo.canonicalize()?;
        let compiler = compiler.canonicalize()?;
        ensure!(
            cargo.file_name().is_some_and(|name| name == "cargo")
                && compiler
                    .file_name()
                    .is_some_and(|name| name == "cargo-chialisp-oxide"),
            "provide actual Cargo and cargo-chialisp-oxide executables"
        );
        ensure!(
            cargo
                .parent()
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .is_some_and(|name| name == format!("{TOOLCHAIN}-{NATIVE_HOST}").as_str()),
            "native Cargo must be the installed pinned dated-nightly host toolchain"
        );
        let driver = compiler
            .parent()
            .context("compiler parent")?
            .join("chialisp-oxide-driver");
        let rustc = cargo.with_file_name("rustc");
        for path in [&cargo, &rustc, &compiler, &driver] {
            fingerprint(path)?;
        }
        Ok(Self {
            cargo,
            rustc,
            compiler,
            driver,
        })
    }
    fn fingerprints(&self) -> Result<BTreeMap<String, String>> {
        let mut rows = BTreeMap::new();
        for (name, path) in [
            ("cargo", &self.cargo),
            ("rustc", &self.rustc),
            ("cargo_adapter", &self.compiler),
            ("driver", &self.driver),
        ] {
            rows.insert(name.into(), fingerprint(path)?);
        }
        let root = self
            .cargo
            .parent()
            .context("Cargo bin directory")?
            .parent()
            .context("Cargo toolchain directory")?
            .join("lib");
        let mut libraries = BTreeMap::new();
        let mut total = 0u64;
        freeze_libraries(&root, &root, 0, &mut total, &mut libraries)?;
        ensure!(
            !libraries.is_empty(),
            "installed toolchain libraries unavailable"
        );
        rows.insert(
            "toolchain_libraries".into(),
            hex::encode(crate::digest(&serde_json::to_vec(&libraries)?)),
        );
        Ok(rows)
    }
}

fn freeze_libraries(
    root: &Path,
    directory: &Path,
    depth: usize,
    total: &mut u64,
    rows: &mut BTreeMap<String, String>,
) -> Result<()> {
    freeze_libraries_for_host(root, directory, depth, total, rows, NATIVE_HOST)
}

const LINUX_LLVM_LIBRARY: &str = "libLLVM.so.22.1-rust-1.99.0-nightly";
fn named_library_exception(host: &str) -> Option<&'static str> {
    (TOOLCHAIN == "nightly-2026-07-14" && host == "x86_64-unknown-linux-gnu")
        .then_some(LINUX_LLVM_LIBRARY)
}

// Only tests select another host. Production always passes NATIVE_HOST above.
fn freeze_libraries_for_host(
    root: &Path,
    directory: &Path,
    depth: usize,
    total: &mut u64,
    rows: &mut BTreeMap<String, String>,
    host: &str,
) -> Result<()> {
    ensure!(depth <= 8, "toolchain library directory depth bound");
    let host_library = PathBuf::from("rustlib").join(host).join("lib");
    let host_codegen = PathBuf::from("rustlib").join(host).join("codegen-backends");
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let relative = path.strip_prefix(root)?;
        // Installed rust-src/docs/LLVM tools are not loaded host libraries.
        // Only root libraries and pinned host library/codegen subtrees count.
        let host_parent = PathBuf::from("rustlib").join(host);
        let selected_directory = relative == Path::new("rustlib")
            || relative == host_parent
            || relative.starts_with(&host_library)
            || relative.starts_with(&host_codegen);
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() && !selected_directory {
            continue;
        }
        ensure!(
            !metadata.file_type().is_symlink(),
            "toolchain library symlink unsupported"
        );
        if metadata.is_dir() {
            freeze_libraries_for_host(root, &path, depth + 1, total, rows, host)?;
        } else if path.extension().is_some_and(|extension| {
            matches!(extension.to_str(), Some("dylib" | "so" | "rlib" | "rmeta"))
        }) || named_library_exception(host)
            .is_some_and(|name| relative == Path::new(name))
        {
            // The pinned Linux .so is a linker script. Its immediate versioned
            // LLVM ELF dependency has no supported extension, so name it
            // explicitly. No arbitrary suffixes, paths or links are admitted.
            ensure!(metadata.is_file(), "toolchain library must be regular");
            *total = total
                .checked_add(metadata.len())
                .context("toolchain library size overflow")?;
            ensure!(
                *total <= 2 * 1024 * 1024 * 1024
                    && rows.len() < 4096
                    && metadata.len() <= 512 * 1024 * 1024,
                "toolchain library snapshot bound"
            );
            rows.insert(
                path.strip_prefix(root)?
                    .to_str()
                    .context("toolchain library path UTF-8")?
                    .into(),
                fingerprint(&path)?,
            );
        }
    }
    Ok(())
}

#[derive(Serialize)]
pub struct FreshArtifact {
    pub lane: String,
    pub module_hex: String,
    pub module_tree_hash: String,
    pub instance_hex: String,
    pub instance_tree_hash: String,
    pub bundle_manifest_sha256: Option<String>,
}
#[derive(Serialize)]
pub struct FreshReport {
    pub protocol: &'static str,
    pub scope: &'static str,
    pub destination_hex: String,
    pub execution_protocols: Value,
    pub native_complete_conditions: Value,
    pub native_input_fingerprint: String,
    pub executable_sha256: BTreeMap<String, String>,
    pub reference_source_sha256: String,
    pub processes: Vec<ProcessReceipt>,
    pub artifacts: Vec<FreshArtifact>,
    pub vm: Vec<VmObservation>,
    pub simulator: Vec<SimulatorObservation>,
    pub sdk_transaction_attempts: u32,
}

fn prepare_output(path: &Path) -> Result<PathBuf> {
    ensure!(
        path.file_name().is_some()
            && path
                .components()
                .all(|part| !matches!(part, Component::ParentDir)),
        "output path must name a new directory"
    );
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut walk = PathBuf::new();
    for part in parent.components() {
        walk.push(part);
        ensure!(
            !fs::symlink_metadata(&walk)?.file_type().is_symlink(),
            "output parent symlink refused"
        );
    }
    let path = parent.canonicalize()?.join(path.file_name().unwrap());
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(&path)?;
    }
    Ok(path)
}
fn bundle_path(output: &Path) -> Result<PathBuf> {
    let pointer: BTreeMap<String, String> =
        serde_json::from_slice(&read(&output.join("latest.json"), 16 * 1024)?)?;
    ensure!(pointer.len() == 2, "full schema2 pointer required");
    let bundle = pointer.get("bundle").context("missing bundle")?;
    ensure!(
        !bundle.is_empty()
            && Path::new(bundle)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            && !bundle.contains(['/', '\\']),
        "invalid bundle directory"
    );
    ensure!(
        pointer.get("manifest") == Some(&format!("{bundle}/manifest.json")),
        "bundle pointer mismatch"
    );
    Ok(output.join(bundle).join("manifest.json"))
}
fn cargo_options(manifest: &Path) -> Vec<String> {
    vec![
        "--manifest-path".into(),
        manifest.to_str().unwrap().into(),
        "--package".into(),
        PACKAGE.into(),
        "--locked".into(),
        "--offline".into(),
        "--jobs".into(),
        "2".into(),
    ]
}
fn admit(
    output: &Path,
    manifest: &Path,
    optimize: bool,
    driver_hash: &str,
    compiler_text: &str,
) -> Result<(Vec<u8>, String)> {
    let path = bundle_path(output)?;
    let admitted = chialisp_oxide_package::bundle::admit_bundle(&path, &Limits::default())?;
    let value: Value = serde_json::from_str(admitted.manifest_json())?;
    let receipt = admitted
        .source_receipt()
        .context("fresh checks require complete source-aware bundle")?;
    ensure!(
        value["schema_version"] == 2
            && value["package"] == PACKAGE
            && value["manifest_path"] == manifest.to_str().context("manifest UTF-8")?
            && value["toolchain"] == TOOLCHAIN
            && value["compiler"] == compiler_text
            && value["analysis_target"] == NATIVE_HOST
            && value["driver_sha256"] == driver_hash
            && receipt.build.optimize_lir == optimize
            && receipt.build.cargo_options == cargo_options(manifest)
            && receipt.unit.crate_name == "chialisp_oxide_lifecycle_payment"
            && receipt.unit.features.is_empty()
            && receipt.unit.analysis_target == NATIVE_HOST
            && receipt.unit.library_source == "package/src/lib.rs"
            && admitted.entries().len() == 1,
        "fresh build differs from requested package/compiler/options"
    );
    let library = admitted
        .sources()
        .iter()
        .find(|source| source.path == receipt.unit.library_source)
        .context("selected native library source attachment")?;
    ensure!(
        hex::decode(&library.content_hex)? == include_bytes!("../../recipe/src/lib.rs"),
        "fresh extraction differs from actual linked native library source"
    );
    let export = admitted
        .entries()
        .get(EXPORT)
        .context("missing exact payment export")?;
    ensure!(
        export.program().environment == chialisp_oxide_ir::Environment::Strict
            && export.program().parameters.len() == 1
            && export.program().parameters[0].ty == chialisp_oxide_ir::Type::Bytes { len: 32 },
        "unexpected typed payment ABI"
    );
    let compiled = chialisp_oxide_ir::compile(export.program(), optimize)?;
    ensure!(
        compiled == export.artifact().bytes(),
        "admitted IR recompilation differs from emitted artifact (consistency check)"
    );
    Ok((
        compiled,
        hex::encode(crate::digest(admitted.manifest_json().as_bytes())),
    ))
}

/// Run actual native Cargo tests and fresh dual-mode production extraction,
/// independent reference compilation, primary VM vectors and complete SDK
/// spends. Creates a new bounded report directory; never overwrites a run.
/// This executes trusted Cargo/build scripts and is not a sandbox.
pub fn check_fresh(
    tools: &FreshTools,
    destination: [u8; 32],
    output: &Path,
) -> Result<FreshReport> {
    require_native_inputs()?;
    let fingerprints = tools.fingerprints()?;
    let output = prepare_output(output)?;
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("recipe/Cargo.toml")
        .canonicalize()?;
    let frozen = || -> Result<()> {
        require_native_inputs()?;
        ensure!(
            tools.fingerprints()? == fingerprints,
            "installed Cargo/compiler/driver changed during fresh checks"
        );
        Ok(())
    };
    let mut processes = vec![];
    let mut compiler_identity = Command::new(&tools.rustc);
    compiler_identity.arg("-vV");
    processes.push(process::run(
        &mut compiler_identity,
        "rustc_identity",
        std::time::Duration::from_secs(10),
        &output,
    )?);
    let compiler_text =
        String::from_utf8(read(&output.join("rustc_identity.stdout.log"), 64 * 1024)?)?;
    let mut driver_identity = Command::new(&tools.driver);
    driver_identity.arg("--compiler-version");
    processes.push(process::run(
        &mut driver_identity,
        "driver_identity",
        std::time::Duration::from_secs(10),
        &output,
    )?);
    let driver_text =
        String::from_utf8(read(&output.join("driver_identity.stdout.log"), 64 * 1024)?)?;
    ensure!(
        driver_text.trim_end()
            == compiler_text
                .lines()
                .next()
                .context("missing compiler identity")?,
        "driver and actual installed compiler declarations differ"
    );
    frozen()?;
    let mut native = Command::new(&tools.cargo);
    native
        .args(["test", "--lib"])
        .args(cargo_options(&manifest))
        .env("RUSTUP_TOOLCHAIN", TOOLCHAIN)
        .env("RUSTC", &tools.rustc)
        .env("CARGO_TARGET_DIR", output.join("native-target"));
    processes.push(process::run(
        &mut native,
        "native_cargo_test",
        process::DEADLINE,
        &output,
    )?);
    frozen()?;
    let (assertion, payment) =
        chialisp_oxide_lifecycle_payment::pay(chialisp_oxide::PuzzleHash(destination));
    ensure!(
        assertion.0 == 73
            && assertion.1.mojos() == 1000
            && payment.0 == 51
            && payment.1.0 == destination
            && payment.2.mojos() == 1000,
        "actual native complete output differs"
    );
    let mut programs = vec![];
    for optimize in [false, true] {
        let lane = if optimize { "rust_on" } else { "rust_off" };
        let mut command = Command::new(&tools.compiler);
        command
            .arg("build")
            .args(cargo_options(&manifest))
            .arg("--output-dir")
            .arg(output.join(lane));
        if !optimize {
            command.arg("--no-optimize");
        }
        command
            .env("CARGO_TARGET_DIR", output.join("compiler-target"))
            .env("CHIALISP_OXIDE_DRIVER", &tools.driver);
        processes.push(process::run(
            &mut command,
            lane,
            process::DEADLINE,
            &output,
        )?);
        frozen()?;
        let (bytes, manifest_hash) = admit(
            &output.join(lane),
            &manifest,
            optimize,
            fingerprints
                .get("driver")
                .context("frozen driver identity")?,
            &compiler_text,
        )?;
        programs.push((lane, bytes, Some(manifest_hash)));
    }
    programs.push((
        "chialisp_reference",
        chialisp_oxide_reference::compile(REFERENCE)?,
        None,
    ));
    frozen()?;
    let mut artifacts = vec![];
    let mut vm = vec![];
    let mut simulator = vec![];
    let mut attempts = 0;
    let failure = |stage,
                   error,
                   artifacts: &[FreshArtifact],
                   vm: &[VmObservation],
                   simulator: &[SimulatorObservation],
                   attempts| {
        receipt::failed(
            &output,
            stage,
            error,
            receipt::Observations {
                processes: &processes,
                artifacts,
                vm,
                simulator,
                sdk_transaction_attempts: attempts,
            },
        )
    };
    for (lane, module, bundle_hash) in programs {
        let instance = match validation::curry(&module, &destination) {
            Ok(instance) => instance,
            Err(error) => {
                return failure(
                    "curry_preflight",
                    error,
                    &artifacts,
                    &vm,
                    &simulator,
                    attempts,
                );
            }
        };
        artifacts.push(FreshArtifact {
            lane: lane.into(),
            module_hex: hex::encode(&module),
            module_tree_hash: inspect(&module)?.identity().module_tree_hash.to_hex(),
            instance_hex: hex::encode(&instance),
            instance_tree_hash: inspect(&instance)?.identity().module_tree_hash.to_hex(),
            bundle_manifest_sha256: bundle_hash,
        });
        let first = vm.len();
        if let Err(error) = validation::vectors_into(&module, &destination, lane, &mut vm) {
            return failure("vm_execution", error, &artifacts, &vm, &simulator, attempts);
        }
        for observation in &vm[first..] {
            if let Err(error) = oracle::verify(observation, &destination) {
                return failure(
                    "vm_exact_oracle",
                    error,
                    &artifacts,
                    &vm,
                    &simulator,
                    attempts,
                );
            }
        }
        match validation::simulate(&module, &destination, lane, &mut attempts) {
            Ok(observation) => simulator.push(observation),
            Err(error) => {
                return failure(
                    "sdk_simulator",
                    error,
                    &artifacts,
                    &vm,
                    &simulator,
                    attempts,
                );
            }
        }
    }
    if vm.len() != 33 || simulator.len() != 3 || attempts != 6 {
        return failure(
            "observation_inventory",
            anyhow::anyhow!("incomplete actual VM/SDK observation inventory"),
            &artifacts,
            &vm,
            &simulator,
            attempts,
        );
    }
    if let Err(error) = frozen() {
        return failure("input_freeze", error, &artifacts, &vm, &simulator, attempts);
    }
    let report = FreshReport {
        protocol: "chialisp-oxide-lifecycle-fresh-check/v1",
        scope: "WIP trusted local fresh execution; source/binary consistency, not authentication or compiler preservation; report alone grants no journal/GUI execution or live network credit",
        destination_hex: hex::encode(destination),
        execution_protocols: serde_json::json!({
            "primary_vm":chialisp_oxide_vm::FAILURE_PROTOCOL,
            "primary_vm_flags":clvmr::chia_dialect::MEMPOOL_MODE.bits(),
            "primary_vm_max_cost":MAX_COST,
            "reference_compiler":chialisp_oxide_reference::COMPILER_VERSION,
            "sdk":chialisp_oxide_simulator::SIMULATOR_VERSION,
            "consensus":chialisp_oxide_simulator::CONSENSUS_VERSION,
            "sdk_vm":chialisp_oxide_simulator::VM_VERSION,
            "rejection_protocol":chialisp_oxide_simulator::REJECTION_PROTOCOL,
            "network":chialisp_oxide_simulator::NETWORK,
            "genesis_challenge":hex::encode(chia_sdk_types::TESTNET11_CONSTANTS.genesis_challenge),
            "sdk_validation_height":0,
            "sdk_validation_flags":chia_consensus::spendbundle_validation::get_flags_for_height_and_constants(0,&chia_sdk_types::TESTNET11_CONSTANTS) | chia_consensus::flags::MEMPOOL_MODE,
            "sdk_max_cost":MAX_COST,
            "installed_library_snapshot_scope":{
                "roots":["lib immediate files",format!("lib/rustlib/{NATIVE_HOST}/lib/**"),format!("lib/rustlib/{NATIVE_HOST}/codegen-backends/**")],
                "types":["dylib","so","rlib","rmeta"],"max_files":4096,
                "named_immediate_regular_file_exceptions":named_library_exception(NATIVE_HOST).into_iter().collect::<Vec<_>>(),
                "named_exception_profile":{"toolchain":TOOLCHAIN,"host":NATIVE_HOST},
                "symlinks":"rejected",
                "max_file_bytes":512u64*1024*1024,"max_aggregate_bytes":2u64*1024*1024*1024,"max_depth_from_lib":8,
                "claim":"bounded named host-library consistency only; not complete build-input authentication"
            }
        }),
        native_complete_conditions: serde_json::json!({
            "assert_my_amount":{"opcode":assertion.0,"amount":assertion.1.mojos()},
            "create_coin":{"opcode":payment.0,"destination_hex":hex::encode(payment.1.0),"amount":payment.2.mojos()}
        }),
        native_input_fingerprint: hex::encode(crate::digest(&serde_json::to_vec(NATIVE_INPUTS)?)),
        executable_sha256: fingerprints,
        reference_source_sha256: hex::encode(crate::digest(REFERENCE.as_bytes())),
        processes,
        artifacts,
        vm,
        simulator,
        sdk_transaction_attempts: attempts,
    };
    receipt::write_new(&output.join("report.json"), &report)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compiled_native_snapshot_matches_current_inputs() {
        require_native_inputs().unwrap();
    }
    #[test]
    fn installed_library_snapshot_ignores_deep_source_but_binds_host_libraries() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("rustlib").join(NATIVE_HOST).join("lib");
        fs::create_dir_all(&library).unwrap();
        fs::write(library.join("libcore.rlib"), b"core").unwrap();
        fs::write(root.path().join("librustc_driver.dylib"), b"driver").unwrap();
        let codegen = root
            .path()
            .join("rustlib")
            .join(NATIVE_HOST)
            .join("codegen-backends");
        fs::create_dir_all(&codegen).unwrap();
        fs::write(codegen.join("librustc_codegen.dylib"), b"backend").unwrap();
        fs::create_dir_all(root.path().join("rustlib/src/a/b/c/d/e/f/g/h/i/j/k")).unwrap();
        let mut rows = BTreeMap::new();
        freeze_libraries(root.path(), root.path(), 0, &mut 0, &mut rows).unwrap();
        assert_eq!(rows.len(), 3);
        let before = rows.clone();
        fs::write(library.join("libcore.rlib"), b"changed").unwrap();
        rows.clear();
        freeze_libraries(root.path(), root.path(), 0, &mut 0, &mut rows).unwrap();
        assert_ne!(rows, before);
    }
    #[test]
    fn installed_library_snapshot_refuses_relevant_depth_symlink_and_oversize() {
        for case in ["depth", "symlink", "oversize"] {
            let root = tempfile::tempdir().unwrap();
            let library = root.path().join("rustlib").join(NATIVE_HOST).join("lib");
            fs::create_dir_all(&library).unwrap();
            match case {
                "depth" => fs::create_dir_all(library.join("a/b/c/d/e/f/g/h/i")).unwrap(),
                "symlink" => {
                    let target = root.path().join("real");
                    fs::write(&target, b"library").unwrap();
                    std::os::unix::fs::symlink(target, library.join("alias.rlib")).unwrap();
                }
                _ => fs::File::create(library.join("oversize.rlib"))
                    .unwrap()
                    .set_len(512 * 1024 * 1024 + 1)
                    .unwrap(),
            }
            assert!(
                freeze_libraries(root.path(), root.path(), 0, &mut 0, &mut BTreeMap::new())
                    .is_err()
            );
        }
    }
    #[test]
    fn pinned_linux_llvm_snapshot_binds_versioned_elf_with_constant_linker_script() {
        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("libLLVM-22-rust-1.99.0-nightly.so");
        let library = root.path().join(LINUX_LLVM_LIBRARY);
        fs::write(&script, b"INPUT(libLLVM.so.22.1-rust-1.99.0-nightly)\n").unwrap();
        fs::write(&library, b"\x7fELF synthetic first contents").unwrap();
        let snapshot = || {
            let mut rows = BTreeMap::new();
            freeze_libraries_for_host(
                root.path(),
                root.path(),
                0,
                &mut 0,
                &mut rows,
                "x86_64-unknown-linux-gnu",
            )
            .unwrap();
            rows
        };
        let before = snapshot();
        fs::write(&library, b"\x7fELF synthetic changed contents").unwrap();
        let after = snapshot();
        assert_eq!(before.len(), 2);
        assert_eq!(
            before["libLLVM-22-rust-1.99.0-nightly.so"],
            after["libLLVM-22-rust-1.99.0-nightly.so"]
        );
        assert_ne!(before[LINUX_LLVM_LIBRARY], after[LINUX_LLVM_LIBRARY]);
    }
    #[test]
    fn pinned_linux_llvm_profile_keeps_other_hosts_names_and_paths_closed() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(LINUX_LLVM_LIBRARY), b"named library").unwrap();
        fs::write(
            root.path().join("libLLVM.so.22.2-rust-1.99.0-nightly"),
            b"other version",
        )
        .unwrap();
        let nested = root.path().join("rustlib/x86_64-unknown-linux-gnu/lib");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join(LINUX_LLVM_LIBRARY), b"different path").unwrap();
        let mut rows = BTreeMap::new();
        freeze_libraries_for_host(
            root.path(),
            root.path(),
            0,
            &mut 0,
            &mut rows,
            "x86_64-unknown-linux-gnu",
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows.contains_key(LINUX_LLVM_LIBRARY));
        rows.clear();
        freeze_libraries_for_host(
            root.path(),
            root.path(),
            0,
            &mut 0,
            &mut rows,
            "aarch64-apple-darwin",
        )
        .unwrap();
        assert!(rows.is_empty());
    }
    #[test]
    fn pinned_linux_llvm_rejects_oversize_before_hashing_and_refuses_symlinks() {
        for symlink in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let library = root.path().join(LINUX_LLVM_LIBRARY);
            if symlink {
                let target = root.path().join("target");
                fs::write(&target, b"library").unwrap();
                std::os::unix::fs::symlink(target, &library).unwrap();
            } else {
                fs::File::create(&library)
                    .unwrap()
                    .set_len(512 * 1024 * 1024 + 1)
                    .unwrap();
            }
            let error = freeze_libraries_for_host(
                root.path(),
                root.path(),
                0,
                &mut 0,
                &mut BTreeMap::new(),
                "x86_64-unknown-linux-gnu",
            )
            .unwrap_err();
            assert!(error.to_string().contains(if symlink {
                "symlink unsupported"
            } else {
                "snapshot bound"
            }));
        }
    }
    #[test]
    fn compiled_native_snapshot_refuses_changed_missing_and_symlinked_inputs() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("library.rs");
        fs::write(&file, b"actual native body").unwrap();
        let hash = hex::encode(crate::digest(b"actual native body"));
        let inputs = [("library.rs", hash.as_str())];
        require_inputs_at(root.path(), &inputs).unwrap();
        fs::write(&file, b"changed body").unwrap();
        assert!(require_inputs_at(root.path(), &inputs).is_err());
        fs::remove_file(&file).unwrap();
        assert!(require_inputs_at(root.path(), &inputs).is_err());
        let target = root.path().join("other.rs");
        fs::write(&target, b"actual native body").unwrap();
        std::os::unix::fs::symlink(target, &file).unwrap();
        assert!(require_inputs_at(root.path(), &inputs).is_err());
    }
    #[test]
    fn output_refuses_existing_content_and_parent_symlink() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        let path = root_path.join("run");
        prepare_output(&path).unwrap();
        fs::write(path.join("keep"), b"unchanged").unwrap();
        assert!(prepare_output(&path).is_err());
        assert_eq!(fs::read(path.join("keep")).unwrap(), b"unchanged");
        let alias = root_path.join("alias");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        assert!(prepare_output(&alias.join("new")).is_err());
        assert!(!path.join("new").exists());
    }
}

/// Optimization lane selected for the journal's actual funded reveal.
#[derive(Clone, Copy)]
pub enum PaymentMode {
    Off,
    On,
}
impl PaymentMode {
    pub fn lane(self) -> &'static str {
        match self {
            Self::Off => "rust_off",
            Self::On => "rust_on",
        }
    }
}

/// Execute the complete fresh adapter, then bind its selected instance to a
/// trusted process-local runner recipe. No saved report import is supported.
pub fn check_fresh_recipe(
    tools: &FreshTools,
    destination: [u8; 32],
    output: &Path,
    mode: PaymentMode,
) -> Result<crate::runner::ValidatedRecipe> {
    let report = check_fresh(tools, destination, output)?;
    recipe_from_execution(&report, destination, mode)
}

fn recipe_from_execution(
    report: &FreshReport,
    destination: [u8; 32],
    mode: PaymentMode,
) -> Result<crate::runner::ValidatedRecipe> {
    require_native_inputs()?;
    ensure!(
        report.destination_hex == hex::encode(destination),
        "fresh recipient mismatch"
    );
    let selected = report
        .artifacts
        .iter()
        .find(|artifact| artifact.lane == mode.lane())
        .context("fresh selected optimization lane missing")?;
    let module = hex::decode(&selected.module_hex)?;
    let reveal = validation::curry(&module, &destination)?;
    ensure!(
        hex::encode(&reveal) == selected.instance_hex
            && inspect(&reveal)?.identity().module_tree_hash.to_hex()
                == selected.instance_tree_hash,
        "fresh selected instance mismatch"
    );
    let plan = crate::Plan {
        network: crate::Network {
            testnet: "testnet11".into(),
            genesis_challenge: chia_sdk_types::TESTNET11_CONSTANTS
                .genesis_challenge
                .to_bytes(),
        },
        amount: 1000,
        outputs: vec![crate::Output {
            puzzle_hash: destination,
            amount: 1000,
        }],
    };
    // Stable checks bind complete identities, never run-directory/log paths.
    // Every process executes fresh checks again before replaying these events.
    let native = serde_json::to_vec(&serde_json::json!({
        "protocol":report.protocol,"mode":mode.lane(),"destination_hex":report.destination_hex,
        "native_complete_conditions":report.native_complete_conditions,
        "native_input_fingerprint":report.native_input_fingerprint,
        "executable_sha256":report.executable_sha256,
        "reference_source_sha256":report.reference_source_sha256,
        "module_tree_hash":selected.module_tree_hash,"instance_tree_hash":selected.instance_tree_hash,
        "artifact_set": report.artifacts.iter().map(|a| (&a.lane,&a.module_tree_hash,&a.instance_tree_hash)).collect::<Vec<_>>()
    }))?;
    crate::runner::ValidatedRecipe::fresh(
        plan,
        crate::Artifact {
            reveal,
            solution: vec![0x80],
        },
        native,
        serde_json::json!({"recipe":"fresh-curried-payment-v1","retained_commit":null,
            "artifact_origin":"executed_fresh_cargo_rust_instance","fresh_execution":"executed_this_process",
            "mode":mode.lane(),"native_input_fingerprint":report.native_input_fingerprint,
            "module_tree_hash":selected.module_tree_hash,"instance_tree_hash":selected.instance_tree_hash,
            "validation_scope":"finite current-process fresh checks; consistency, not authentication or compiler preservation"}),
    )
}

#[cfg(test)]
pub(crate) fn reference_recipe_control(
    destination: [u8; 32],
    mode: PaymentMode,
) -> Result<crate::runner::ValidatedRecipe> {
    // A native-only test control, explicitly NOT production Cargo execution.
    let module = chialisp_oxide_reference::compile(REFERENCE)?;
    let mut observations = vec![];
    validation::vectors_into(
        &module,
        &destination,
        "chialisp_reference",
        &mut observations,
    )?;
    for observation in &observations {
        oracle::verify(observation, &destination)?;
    }
    let reveal = validation::curry(&module, &destination)?;
    let plan = crate::Plan {
        network: crate::Network {
            testnet: "testnet11".into(),
            genesis_challenge: chia_sdk_types::TESTNET11_CONSTANTS
                .genesis_challenge
                .to_bytes(),
        },
        amount: 1000,
        outputs: vec![crate::Output {
            puzzle_hash: destination,
            amount: 1000,
        }],
    };
    let (assertion, payment) =
        chialisp_oxide_lifecycle_payment::pay(chialisp_oxide::PuzzleHash(destination));
    ensure!(
        assertion.1.mojos() == 1000 && payment.1.0 == destination && payment.2.mojos() == 1000,
        "native test control output"
    );
    crate::runner::ValidatedRecipe::fresh(
        plan,
        crate::Artifact {
            reveal,
            solution: vec![0x80],
        },
        serde_json::to_vec(
            &serde_json::json!({"test_control_only":true,"mode":mode.lane(),"destination":hex::encode(destination)}),
        )?,
        serde_json::json!({"recipe":"independent-reference-native-test-control","mode":mode.lane(),
            "fresh_execution":"unavailable_test_control","artifact_origin":"independent_reference_test_control",
            "module_tree_hash":inspect(&module)?.identity().module_tree_hash.to_hex()}),
    )
}
