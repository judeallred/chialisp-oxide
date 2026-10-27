//! Actual-process CAT source admission. Retained reports cannot enter this gate.
use super::*;
use crate::live::cat::{CatMode, CatSdkCheckReport, certify_fresh_source};
#[path = "cat_bundle.rs"]
mod bundle;
#[path = "cat_provenance.rs"]
mod provenance;
#[path = "cat_vectors.rs"]
mod vectors;

/// Opaque intermediate input to the SDK gate. Only this actual coordinator can
/// create it; no report, imported module or test fixture is an authority source.
pub(crate) struct CheckedCatSourceMaterial {
    rust_off: Vec<u8>,
    rust_on: Vec<u8>,
    canonical: Vec<u8>,
    identity: [u8; 32],
    tools: FreshTools,
    fingerprints: BTreeMap<String, String>,
}
impl CheckedCatSourceMaterial {
    pub(crate) fn selected_module(&self, mode: CatMode) -> &[u8] {
        match mode {
            CatMode::RustOff => &self.rust_off,
            CatMode::RustOn => &self.rust_on,
        }
    }
    pub(crate) fn canonical_module(&self) -> &[u8] {
        &self.canonical
    }
    pub(crate) fn source_identity(&self) -> [u8; 32] {
        self.identity
    }
    pub(crate) fn require_current(&self) -> Result<()> {
        require_native_inputs()?;
        ensure!(
            self.tools.fingerprints()? == self.fingerprints,
            "installed CAT compiler/toolchain changed"
        );
        Ok(())
    }
}

#[derive(Serialize)]
pub(crate) struct FreshCatReport {
    protocol: &'static str,
    status: &'static str,
    selected_mode: &'static str,
    source_identity: String,
    native_input_sha256: String,
    installed_tools: BTreeMap<String, String>,
    compiler_identity: String,
    processes: Vec<ProcessReceipt>,
    bundles: Vec<bundle::BundleObservation>,
    canonical: provenance::Provenance,
    native: Vec<vectors::NativeObservation>,
    primary_vm: Vec<vectors::VmObservation>,
    sdk: CatSdkCheckReport,
    scope: &'static str,
}

/// This check-only operation creates and drops the selected authority after all
/// source, VM and SDK checks; it cannot sign, persist a live outbox or submit.
pub(crate) fn check_cat(
    tools: &FreshTools,
    output: &Path,
    selected: CatMode,
) -> Result<FreshCatReport> {
    require_native_inputs()?;
    let fingerprints = tools.fingerprints()?;
    let frozen = || -> Result<()> {
        require_native_inputs()?;
        ensure!(
            tools.fingerprints()? == fingerprints,
            "installed CAT compiler/toolchain changed during checks"
        );
        Ok(())
    };
    let output = prepare_output(output)?;
    let manifest = source_root().join("puzzles/Cargo.toml").canonicalize()?;
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
                .context("compiler identity missing")?,
        "CAT actual rustc/driver declarations differ"
    );
    frozen()?;
    let mut native = Command::new(&tools.cargo);
    native
        .args(["test", "--lib"])
        .args(bundle::options(&manifest))
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
    let cases = vectors::cases()?;
    let native = vectors::native_vectors(&cases)?;
    let mut modules = vec![];
    let mut bundles = vec![];
    for optimize in [false, true] {
        let lane = if optimize { "rust_on" } else { "rust_off" };
        let mut command = Command::new(&tools.compiler);
        command
            .arg("build")
            .args(bundle::options(&manifest))
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
        let (module, report) = bundle::admit(
            &output.join(lane),
            &manifest,
            optimize,
            fingerprints.get("driver").context("frozen driver")?,
            &compiler_text,
        )?;
        modules.push(module);
        bundles.push(report);
    }
    let (canonical, canonical_report) = provenance::compile()?;
    let mut primary_vm = vec![];
    for (lane, module, reference) in [
        ("rust_off", modules[0].as_slice(), false),
        ("rust_on", modules[1].as_slice(), false),
        ("chialisp_reference", canonical.as_slice(), true),
    ] {
        primary_vm.extend(vectors::vm_vectors(module, lane, reference, &cases)?);
    }
    frozen()?;
    let native_input_sha256 = hex::encode(crate::digest(&serde_json::to_vec(NATIVE_INPUTS)?));
    let identity = crate::digest(&serde_json::to_vec(&serde_json::json!({
        "protocol":"chialisp-oxide-fresh-cat-material/v1", "native_inputs":native_input_sha256,
        "installed_tools":fingerprints,"bundles":bundles,"canonical":canonical_report,
        "native":native,"primary_vm":primary_vm
    }))?);
    let rust_on = modules.pop().context("checked Rust-on module")?;
    let rust_off = modules.pop().context("checked Rust-off module")?;
    let material = CheckedCatSourceMaterial {
        rust_off,
        rust_on,
        canonical,
        identity,
        tools: FreshTools {
            cargo: tools.cargo.clone(),
            rustc: tools.rustc.clone(),
            compiler: tools.compiler.clone(),
            driver: tools.driver.clone(),
        },
        fingerprints: fingerprints.clone(),
    };
    // The consuming SDK bridge must not return an authority before all actual
    // local issue/transfer controls complete. No fixture path is available here.
    let (authority, sdk) = certify_fresh_source(material, selected)?;
    ensure!(
        authority.mode() == Some(selected),
        "selected fresh Rust authority mode"
    );
    frozen()?;
    drop(authority);
    let report = FreshCatReport {
        protocol: "chialisp-oxide-fresh-cat-check/v1",
        status: "passed",
        selected_mode: match selected {
            CatMode::RustOff => "rust_off",
            CatMode::RustOn => "rust_on",
        },
        source_identity: hex::encode(identity),
        native_input_sha256,
        installed_tools: fingerprints,
        compiler_identity: compiler_text,
        processes,
        bundles,
        canonical: canonical_report,
        native,
        primary_vm,
        sdk,
        scope: "Actual paired Cargo native execution, complete40-export schema2 admission/recompilation in both Rust modes, independent pinned canonical compilation and raw-environment vectors, and separate actual SDK controls. Bounded broad local and declared-input consistency assumes mutable host inputs, installed compiler/native tooling and external Cargo cache dependencies remain trustworthy; no binary authentication, compiler-preservation, chain consensus, live issuance, Sage or custody claim. Primary policy VM10M, wallet bundle25M and new CAT bundle100M remain distinct.",
    };
    let bytes = serde_json::to_vec_pretty(&report)?;
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output.join("cat-check.json"))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forbidden_decimal_environment_child() {
        if std::env::var_os("OXIDE_CAT_DECIMAL_ENV_CONTROL").is_some() {
            assert!(require_native_environment().is_err());
        }
    }
    #[test]
    fn every_decimal_override_is_rejected_in_an_actual_child_environment() {
        for name in NATIVE_FORBIDDEN_ENV {
            let result = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "fresh::cat::tests::forbidden_decimal_environment_child",
                ])
                .env("OXIDE_CAT_DECIMAL_ENV_CONTROL", "1")
                .env(name, "100")
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "runtime environment control {name}"
            );
        }
    }
    #[test]
    fn source_aggregate_and_row_bounds_precede_body_reads() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source.rs");
        fs::File::create(&path)
            .unwrap()
            .set_len(MAX_FILE as u64)
            .unwrap();
        // Repeated public rows reserve65MiB before checking any invalid digest.
        let rows = vec![("source.rs", "invalid"); 17];
        let error = require_inputs_at(directory.path(), &rows)
            .unwrap_err()
            .to_string();
        assert!(error.contains("aggregate byte bound"), "{error}");
        assert!(
            require_inputs_at(directory.path(), &vec![("source.rs", ""); 1025])
                .unwrap_err()
                .to_string()
                .contains("row bound")
        );
        fs::File::create(&path)
            .unwrap()
            .set_len(MAX_FILE as u64 + 1)
            .unwrap();
        assert!(
            require_inputs_at(directory.path(), &[("source.rs", "")])
                .unwrap_err()
                .to_string()
                .contains("aggregate byte bound")
        );
    }
}
