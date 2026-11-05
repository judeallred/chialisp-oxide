//! Tie the native oracle executable to the same declared local Cargo inputs.
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

fn main() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("../..")
        .canonicalize()
        .unwrap();
    println!(
        "cargo:rerun-if-changed={}",
        root.join("puzzles/authoring.json").display()
    );
    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("puzzles/authoring.json")).unwrap())
            .unwrap();
    let mut sources = vec![];
    for input in config["inputs"]
        .as_array()
        .expect("declared corpus Cargo inputs")
    {
        let path = input.as_str().expect("Cargo input path");
        assert!(
            !Path::new(path).is_absolute()
                && Path::new(path)
                    .components()
                    .all(|component| matches!(component, Component::Normal(_))),
            "Cargo input must be repo-relative"
        );
        let resolved = root.join(path).canonicalize().expect("resolve Cargo input");
        assert!(
            resolved.starts_with(&root),
            "Cargo input escapes repository"
        );
        println!("cargo:rerun-if-changed={}", resolved.display());
        sources.push(serde_json::json!({"path":path,"sha256":hex::encode(Sha256::digest(std::fs::read(resolved).unwrap()))}));
    }
    sources.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
    let output = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("native-inputs.json");
    std::fs::write(output, serde_json::to_vec(&sources).unwrap()).unwrap();
}
