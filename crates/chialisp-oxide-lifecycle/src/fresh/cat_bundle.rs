//! Complete immutable Cargo-bundle admission before selecting the TAIL.
use super::super::*;
use chialisp_oxide_ir::{Environment, Type};
use std::collections::BTreeSet;

pub(super) const PACKAGE: &str = "chialisp-oxide-puzzles";
pub(super) const EXPORT: &str = "chialisp_oxide_puzzles::cat_genesis_by_coin_id::puzzle";
pub(super) fn options(manifest: &Path) -> Vec<String> {
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
#[derive(Serialize)]
pub(super) struct BundleObservation {
    pub lane: String,
    pub manifest_sha256: String,
    pub complete_inventory_sha256: String,
    pub exports_admitted: usize,
    pub exports_recompiled: usize,
    pub source_attachments_checked: usize,
    pub selected_ir_sha256: String,
    pub selected_module_sha256: String,
    pub selected_module_tree_hash: String,
    pub selected_module_hex: String,
}

pub(super) fn admit(
    output: &Path,
    manifest: &Path,
    optimize: bool,
    driver_hash: &str,
    compiler_text: &str,
) -> Result<(Vec<u8>, BundleObservation)> {
    let path = super::super::bundle_path(output)?;
    // The shared bounded reader validates every program, blob and source
    // inventory before selection. It establishes consistency, not authenticity.
    let bundle = chialisp_oxide_package::bundle::admit_bundle(&path, &Limits::default())?;
    let value: Value = serde_json::from_str(bundle.manifest_json())?;
    let receipt = bundle
        .source_receipt()
        .context("CAT requires complete schema2 source-aware bundle")?;
    let root = source_root();
    ensure!(
        value["schema_version"] == 2
            && value["package"] == PACKAGE
            && value["workspace_root"] == root.to_str().context("root UTF8")?
            && value["manifest_path"] == manifest.to_str().context("manifest UTF8")?
            && value["toolchain"] == TOOLCHAIN
            && value["compiler"] == compiler_text
            && value["analysis_target"] == NATIVE_HOST
            && value["driver_sha256"] == driver_hash
            && value["cargo_options"] == serde_json::to_value(options(manifest))?
            && receipt.build.cargo_options == options(manifest)
            && receipt.build.optimize_lir == optimize
            && receipt.unit.crate_name == "chialisp_oxide_puzzles"
            && receipt.unit.library_source == "package/lib.rs"
            && receipt.unit.features.is_empty()
            && receipt.unit.analysis_target == NATIVE_HOST,
        "actual CAT bundle compiler/driver/target/package/options identity"
    );
    let authoring: Value =
        serde_json::from_slice(include_bytes!("../../../../puzzles/authoring.json"))?;
    let exports = authoring["exports"]
        .as_object()
        .context("declared Cargo exports")?;
    ensure!(
        authoring["schema_version"] == 1
            && authoring["mode"] == "cargo_library"
            && authoring["package"] == PACKAGE
            && authoring["manifest"] == "puzzles/Cargo.toml"
            && authoring["library"] == "puzzles/lib.rs"
            && exports.len() == 40,
        "exact current40 Cargo declaration"
    );
    let expected = exports
        .values()
        .map(|row| row["entry"].as_str().context("declared export entry"))
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        expected.len() == 40
            && bundle
                .entries()
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                == expected,
        "complete40 export set, never selected-only fallback"
    );
    let mut source_count = 0;
    let mut selected_source = false;
    let mut native_library = false;
    for source in bundle.sources() {
        let relative = if let Some(path) = source.path.strip_prefix("package/") {
            PathBuf::from("puzzles").join(path)
        } else if let Some(path) = source.path.strip_prefix("workspace/") {
            PathBuf::from(path)
        } else {
            anyhow::bail!("unsupported automatic source scope")
        };
        ensure!(
            relative
                .components()
                .all(|p| matches!(p, Component::Normal(_))),
            "confined native source binding"
        );
        ensure!(
            NATIVE_INPUTS
                .iter()
                .any(|(name, _)| Path::new(name) == relative),
            "source attachment outside compiled native closure"
        );
        let current = read(&root.join(&relative), MAX_FILE)?;
        ensure!(
            source.content_hex.len() <= MAX_FILE * 2
                && hex::decode(&source.content_hex)? == current,
            "actual current source attachment bytes"
        );
        if source.path == "package/lib.rs" {
            ensure!(
                current == include_bytes!("../../../../puzzles/lib.rs"),
                "actual linked native library"
            );
            native_library = true;
        }
        if source.path == "package/cat-genesis-by-coin-id/puzzle.rs" {
            ensure!(
                current == include_bytes!("../../../../puzzles/cat-genesis-by-coin-id/puzzle.rs"),
                "actual paired native TAIL source"
            );
            selected_source = true;
        }
        source_count += 1;
    }
    ensure!(
        native_library && selected_source,
        "complete selected native source attachments"
    );
    // Recompile all40 independently of the selected export. This is executed
    // IR/artifact consistency, not a compiler-preservation or nominal-type proof.
    let mut inventory = BTreeMap::new();
    for (name, export) in bundle.entries() {
        ensure!(
            export.program().name == *name,
            "exact declared IR entry identity"
        );
        let bytes = chialisp_oxide_ir::compile(export.program(), optimize)?;
        ensure!(
            bytes == export.artifact().bytes(),
            "full40 IR/artifact recompile mismatch"
        );
        inventory.insert(
            name.as_str(),
            (
                hex::encode(crate::digest(export.ir_json().as_bytes())),
                hex::encode(crate::digest(export.artifact().bytes())),
                inspect(export.artifact().bytes())?
                    .identity()
                    .module_tree_hash
                    .to_hex(),
            ),
        );
    }
    let selected = bundle
        .entries()
        .get(EXPORT)
        .context("exact CAT TAIL export")?;
    ensure!(
        selected.program().environment == Environment::Raw
            && selected.program().parameters.len() == 1
            && selected.program().parameters[0].ty == Type::Tree
            && selected.program().result == Type::Tree,
        "actual raw whole-environment TAIL ABI"
    );
    let bytes = selected.artifact().bytes().to_vec();
    ensure!(
        !bytes.is_empty() && bytes.len() <= 16 * 1024,
        "bounded selected TAIL module"
    );
    let report = BundleObservation {
        lane: if optimize { "rust_on" } else { "rust_off" }.into(),
        manifest_sha256: hex::encode(crate::digest(bundle.manifest_json().as_bytes())),
        complete_inventory_sha256: hex::encode(crate::digest(&serde_json::to_vec(&inventory)?)),
        exports_admitted: 40,
        exports_recompiled: 40,
        source_attachments_checked: source_count,
        selected_ir_sha256: hex::encode(crate::digest(selected.ir_json().as_bytes())),
        selected_module_sha256: hex::encode(crate::digest(&bytes)),
        selected_module_tree_hash: inspect(&bytes)?.identity().module_tree_hash.to_hex(),
        selected_module_hex: hex::encode(&bytes),
    };
    Ok((bytes, report))
}
