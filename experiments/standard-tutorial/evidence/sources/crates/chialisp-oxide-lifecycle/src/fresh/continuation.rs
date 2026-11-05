//! Actual-process continuation checks; imported reports never grant authority.
use super::*;
use crate::live::continuation::{ContinuationMode, ContinuationRecipe};
use chialisp_oxide_ir::{Environment, Type};

const PACKAGE: &str = "chialisp-oxide-authenticated-continuations";
const ENTRY: &str = "chialisp_oxide_authenticated_continuations::step";

fn fingerprints(tools: &FreshTools) -> Result<BTreeMap<String, String>> {
    let mut values = tools.fingerprints()?;
    let directory = tools.compiler.parent().context("tool directory")?;
    for name in ["chialisp-oxide", "chialisp-oxide-verifier"] {
        values.insert(name.into(), fingerprint(&directory.join(name))?);
    }
    Ok(values)
}

pub(crate) struct CheckedContinuationMaterial {
    modules: [Vec<u8>; 2],
    #[cfg(test)]
    fixture: bool,
    identity: crate::Digest,
    tools: FreshTools,
    fingerprints: BTreeMap<String, String>,
}
impl CheckedContinuationMaterial {
    pub(crate) fn module(&self, mode: ContinuationMode) -> &[u8] {
        &self.modules[usize::from(mode == ContinuationMode::On)]
    }
    pub(crate) fn identity(&self) -> crate::Digest {
        self.identity
    }
    pub(crate) fn require_current(&self) -> Result<()> {
        #[cfg(test)]
        if self.fixture {
            return Ok(());
        }
        require_native_inputs()?;
        ensure!(
            fingerprints(&self.tools)? == self.fingerprints,
            "continuation tools changed; repeat fresh checks"
        );
        Ok(())
    }
}

/// Native Cargo, both complete source-aware compiler bundles, independent
/// Chialisp/native/VM/SDK execution happen in this process's frozen child group.
/// The Rust reader independently admits/recompiles every newly produced export.
/// Source/tool consistency is not binary authentication or preservation proof.
pub fn certify_continuation(
    tools: &FreshTools,
    output: &Path,
    mode: ContinuationMode,
    recipients: Vec<crate::Digest>,
) -> Result<ContinuationRecipe> {
    require_native_inputs()?;
    let frozen_tools = fingerprints(tools)?;
    let output = prepare_output(output)?;
    let mut identity = Command::new(&tools.rustc);
    identity.arg("-vV");
    process::run(
        &mut identity,
        "rustc_identity",
        std::time::Duration::from_secs(10),
        &output,
    )?;
    let compiler_text = String::from_utf8(read(&output.join("rustc_identity.stdout.log"), 65536)?)?;
    let tool_directory = tools.compiler.parent().context("tool directory")?;
    for name in ["chialisp-oxide", "chialisp-oxide-verifier"] {
        fingerprint(&tool_directory.join(name))?;
    }
    let path = std::env::join_paths(
        [
            tools
                .cargo
                .parent()
                .context("cargo directory")?
                .to_path_buf(),
            tool_directory.to_path_buf(),
        ]
        .into_iter()
        .chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )),
    )?;
    let mut child = Command::new("python3");
    child.env("PATH", path);
    child
        .arg("-B")
        .arg(source_root().join("experiments/authenticated-continuations/ci.py"))
        .arg("--tools")
        .arg(tool_directory)
        .arg("--expected-host")
        .arg(NATIVE_HOST)
        .arg("--output")
        .arg(output.join("execution"))
        .env("RUSTUP_TOOLCHAIN", TOOLCHAIN)
        .env("RUSTUP_AUTO_INSTALL", "0")
        .env("CARGO_NET_OFFLINE", "true");
    process::run(
        &mut child,
        "control",
        std::time::Duration::from_secs(3600),
        &output,
    )?;
    require_native_inputs()?;
    ensure!(
        fingerprints(tools)? == frozen_tools,
        "tools changed during continuation checks"
    );
    let root = source_root()
        .join("experiments/authenticated-continuations")
        .canonicalize()?;
    let manifest = root.join("Cargo.toml");
    let options = vec![
        "--manifest-path".to_owned(),
        manifest.to_str().context("manifest UTF8")?.into(),
        "--package".into(),
        PACKAGE.into(),
        "--locked".into(),
    ];
    let mut modules = vec![];
    let mut inventory = vec![];
    let mut selected_sources = None;
    for (lane, optimize) in [("off", false), ("on", true)] {
        let path = bundle_path(&output.join(format!("execution/fresh-{lane}")))?;
        let bundle = chialisp_oxide_package::bundle::admit_bundle(&path, &Limits::default())?;
        let metadata: Value = serde_json::from_str(bundle.manifest_json())?;
        let receipt = bundle
            .source_receipt()
            .context("complete source receipt required")?;
        ensure!(
            metadata["schema_version"] == 2
                && metadata["package"] == PACKAGE
                && metadata["workspace_root"] == root.to_str().context("workspace UTF8")?
                && metadata["manifest_path"] == manifest.to_str().context("manifest UTF8")?
                && metadata["compiler"] == compiler_text
                && metadata["analysis_target"] == NATIVE_HOST
                && metadata["toolchain"] == TOOLCHAIN
                && metadata["driver_sha256"] == frozen_tools["driver"]
                && receipt.build.optimize_lir == optimize
                && receipt.build.cargo_options == options
                && receipt.unit.crate_name == "chialisp_oxide_authenticated_continuations"
                && receipt.unit.library_source == "package/src/lib.rs"
                && receipt.unit.features.is_empty()
                && receipt.unit.analysis_target == NATIVE_HOST,
            "fresh continuation bundle identities"
        );
        ensure!(
            bundle.entries().len() == 1 && bundle.entries().contains_key(ENTRY),
            "complete exact continuation export set"
        );
        let mut sources = std::collections::BTreeSet::new();
        let mut source_digests = BTreeMap::new();
        for source in bundle.sources() {
            let relative = source
                .path
                .strip_prefix("package/")
                .context("selected package source scope")?;
            ensure!(
                ["Cargo.toml", "Cargo.lock", "src/lib.rs"].contains(&relative)
                    && sources.insert(relative.to_owned()),
                "closed unique continuation source inventory"
            );
            ensure!(
                source.content_hex.len() <= MAX_FILE * 2
                    && hex::decode(&source.content_hex)? == read(&root.join(relative), MAX_FILE)?,
                "actual selected-library source bytes"
            );
            source_digests.insert(
                source.path.clone(),
                hex::encode(crate::digest(&hex::decode(&source.content_hex)?)),
            );
        }
        ensure!(
            sources.len() == 3,
            "complete continuation source attachments"
        );
        if let Some(previous) = &selected_sources {
            ensure!(
                previous == &source_digests,
                "both modes must bind identical selected source bytes"
            );
        } else {
            selected_sources = Some(source_digests);
        }
        let export = &bundle.entries()[ENTRY];
        let b32 = Type::Bytes { len: 32 };
        let terms = Type::Tuple {
            items: vec![
                b32.clone(),
                b32.clone(),
                Type::Int {
                    bits: 8,
                    signed: false,
                },
            ],
        };
        let opening = Type::Tuple {
            items: vec![b32.clone(), b32.clone()],
        };
        ensure!(
            export.program().environment == Environment::Strict
                && export.program().parameters.len() == 2
                && export.program().parameters[0].ty == terms
                && export.program().parameters[1].ty == opening,
            "strict continuation Terms/Opening ABI"
        );
        let number = Type::Int {
            bits: 64,
            signed: false,
        };
        let create = Type::Tuple {
            items: vec![number.clone(), b32, number.clone()],
        };
        let result = Type::Pair {
            first: Box::new(Type::Tuple {
                items: vec![number.clone(), number],
            }),
            rest: Box::new(Type::Pair {
                first: Box::new(create.clone()),
                rest: Box::new(Type::Optional {
                    item: Box::new(create),
                }),
            }),
        };
        let wire: Value = serde_json::from_str(export.ir_json())?;
        ensure!(
            wire["ir_version"] == 6 && export.program().result == result,
            "exact v6 condition-list output ABI"
        );
        let bytes = chialisp_oxide_ir::compile(export.program(), optimize)?;
        ensure!(
            bytes == export.artifact().bytes() && !bytes.is_empty() && bytes.len() <= 100_000,
            "fresh complete IR/artifact consistency"
        );
        inventory.push((
            lane,
            hex::encode(crate::digest(export.ir_json().as_bytes())),
            hex::encode(crate::digest(&bytes)),
        ));
        modules.push(bytes);
    }
    require_native_inputs()?;
    ensure!(
        fingerprints(tools)? == frozen_tools,
        "tools changed before continuation authority"
    );
    let material = CheckedContinuationMaterial {
        #[cfg(test)]
        fixture: false,
        modules: modules
            .try_into()
            .map_err(|_| anyhow::anyhow!("two modes"))?,
        // Sorted root-relative native declarations and admitted package-relative
        // source bytes bind implementation/source changes even when both IRs
        // and modules happen to remain identical. No output path/run ID enters.
        identity: crate::digest(&serde_json::to_vec(&(
            NATIVE_INPUTS,
            selected_sources.context("selected source inventory")?,
            frozen_tools.clone(),
            inventory,
        ))?),
        tools: FreshTools {
            cargo: tools.cargo.clone(),
            rustc: tools.rustc.clone(),
            compiler: tools.compiler.clone(),
            driver: tools.driver.clone(),
        },
        fingerprints: frozen_tools,
    };
    ContinuationRecipe::checked(material, mode, recipients)
}

#[cfg(test)]
pub(crate) fn fixture(module: Vec<u8>) -> CheckedContinuationMaterial {
    CheckedContinuationMaterial {
        fixture: true,
        identity: crate::digest(&module),
        modules: [module.clone(), module],
        tools: FreshTools {
            cargo: "fixture".into(),
            rustc: "fixture".into(),
            compiler: "fixture".into(),
            driver: "fixture".into(),
        },
        fingerprints: BTreeMap::new(),
    }
}
