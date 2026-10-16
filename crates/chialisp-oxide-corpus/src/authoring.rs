//! Declared Cargo inputs and the identities of the actual annotated exports.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Component, Path},
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Export {
    pub entry: String,
    pub source: String,
}

#[derive(Deserialize)]
struct Configuration {
    schema_version: u32,
    mode: String,
    package: String,
    manifest: String,
    library: String,
    exports: BTreeMap<String, Export>,
    inputs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Fingerprint {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Authoring {
    pub mode: String,
    pub package: String,
    pub manifest: String,
    pub library: String,
    pub exports: BTreeMap<String, Export>,
    pub sources: Vec<Fingerprint>,
}

/// Snapshot declared inputs, and reject a native oracle built from older inputs.
/// This covers the listed local files, not the complete Cargo dependency graph.
pub fn authoring() -> Result<Authoring> {
    let root = super::root();
    let config: Configuration =
        serde_json::from_slice(&std::fs::read(root.join("puzzles/authoring.json"))?)?;
    ensure!(
        config.schema_version == 1 && config.mode == "cargo_library",
        "unsupported corpus authoring configuration"
    );
    ensure!(
        config.package == "chialisp-oxide-puzzles"
            && config.manifest == "puzzles/Cargo.toml"
            && config.library == "puzzles/lib.rs",
        "unexpected corpus Cargo library"
    );
    ensure!(
        config.exports.len() == super::PUZZLES.len()
            && super::PUZZLES
                .iter()
                .all(|slug| config.exports.contains_key(*slug)),
        "corpus export set changed"
    );
    let mut sources = Vec::new();
    for path in &config.inputs {
        ensure!(
            !Path::new(path).is_absolute()
                && Path::new(path)
                    .components()
                    .all(|component| matches!(component, Component::Normal(_))),
            "Cargo input must be a confined relative path: {path}"
        );
        let resolved = root
            .join(path)
            .canonicalize()
            .with_context(|| format!("resolve Cargo input {path}"))?;
        ensure!(
            resolved.starts_with(&root),
            "Cargo input escapes repository: {path}"
        );
        sources.push(Fingerprint {
            path: path.clone(),
            sha256: hex::encode(Sha256::digest(std::fs::read(resolved)?)),
        });
    }
    sources.sort_by(|left, right| left.path.cmp(&right.path));
    ensure!(
        sources.windows(2).all(|pair| pair[0].path != pair[1].path),
        "duplicate Cargo input"
    );
    let includes = |path: &str| sources.iter().any(|source| source.path == path);
    for path in [
        "puzzles/authoring.json",
        config.manifest.as_str(),
        config.library.as_str(),
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        ".cargo/config.toml",
        "puzzles/fixture_keys.rs",
        "crates/chialisp-oxide/Cargo.toml",
        "crates/chialisp-oxide/src/lib.rs",
        "crates/chialisp-oxide/src/domain.rs",
        "crates/chialisp-oxide-macros/Cargo.toml",
        "crates/chialisp-oxide-macros/src/lib.rs",
    ] {
        ensure!(includes(path), "missing Cargo input {path}");
    }
    for (slug, export) in &config.exports {
        ensure!(
            export.entry == format!("chialisp_oxide_puzzles::{}::puzzle", slug.replace('-', "_"))
                && export.source == format!("puzzles/{slug}/puzzle.rs")
                && includes(&export.source),
            "invalid corpus export {slug}"
        );
    }
    let compiled: Vec<Fingerprint> = serde_json::from_str(include_str!(concat!(
        env!("OUT_DIR"),
        "/native-inputs.json"
    )))?;
    ensure!(
        sources == compiled,
        "Cargo inputs changed since the native oracle was built; run cargo build --workspace --locked before validating"
    );
    Ok(Authoring {
        mode: config.mode,
        package: config.package,
        manifest: config.manifest,
        library: config.library,
        exports: config.exports,
        sources,
    })
}
