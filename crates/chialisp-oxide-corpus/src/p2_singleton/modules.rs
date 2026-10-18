//! Exact pinned canonical counterparts, not claimed as Rust frontend ports.
use anyhow::{Result, ensure};
use chialisp_oxide_simulator::{Bytes32, puzzle_hash};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

pub struct Modules {
    pub launcher: Vec<u8>,
    pub singleton: Vec<u8>,
    pub standard: Vec<u8>,
    pub launcher_hash: Bytes32,
    pub singleton_hash: Bytes32,
    pub provenance: Value,
}

impl Modules {
    pub fn load(root: &Path) -> Result<Self> {
        let revision = "cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8";
        let directory = root.join("corpus/upstream/chia-puzzles").join(revision);
        let code = std::fs::read_to_string(directory.join("puzzles/condition_codes.clib"))?;
        let curry = std::fs::read_to_string(directory.join("puzzles/curry-and-treehash.clib"))?;
        let truths = std::fs::read_to_string(directory.join("puzzles/singleton_truths.clib"))?;
        let macros = std::fs::read_to_string(directory.join("puzzles/utility_macros.clib"))?;
        let mut compiled = Vec::new();
        let mut provenance = Vec::new();
        for name in [
            "singleton_launcher",
            "singleton_top_layer_v1_1",
            "p2_delegated_puzzle_or_hidden_puzzle",
        ] {
            let relative = format!("puzzles/{name}.clsp");
            let source = std::fs::read_to_string(directory.join(&relative))?;
            let includes = if name == "singleton_top_layer_v1_1" {
                vec![
                    ("condition_codes.clib", code.as_str()),
                    ("curry-and-treehash.clib", curry.as_str()),
                    ("singleton_truths.clib", truths.as_str()),
                    ("utility_macros.clib", macros.as_str()),
                ]
            } else {
                vec![("condition_codes.clib", code.as_str())]
            };
            let program = chialisp_oxide_reference::compile_with_includes(&source, &includes)?;
            let published = hex::decode(
                std::fs::read_to_string(directory.join(format!("{relative}.hex")))?.trim(),
            )?;
            ensure!(
                program == published,
                "independent canonical {name} reproduction differs"
            );
            provenance.push(json!({"path":relative,"source_sha256":hex::encode(Sha256::digest(source.as_bytes())),"published_hex_sha256":hex::encode(Sha256::digest(std::fs::read(directory.join(format!("{relative}.hex")))?)),"bytes":program.len(),"tree_hash":hex::encode(puzzle_hash(&program)?),"independent_compilation_matches_published":true}));
            compiled.push(program);
        }
        let standard = compiled.pop().unwrap();
        let singleton = compiled.pop().unwrap();
        let launcher = compiled.pop().unwrap();
        Ok(Self {
            launcher_hash: puzzle_hash(&launcher)?,
            singleton_hash: puzzle_hash(&singleton)?,
            launcher,
            singleton,
            standard,
            provenance: json!({"repository":"https://github.com/Chia-Network/chia_puzzles","revision":revision,"root_tree_sha1":"e131409ed769ef4def743d4ebbcf859b40fcec2d","modules":provenance,"scope":"Canonical host counterparts independently reproduced; no Rust port claim for the top layer or StandardLayer."}),
        })
    }
}
