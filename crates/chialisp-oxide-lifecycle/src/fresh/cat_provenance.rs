//! Independently pinned canonical TAIL source/include/artifact/license.
use super::super::*;

const BASE: &str = "experiments/cat-genesis-tail/";
const PINS: [(&str, &str); 4] = [
    (
        "upstream/genesis_by_coin_id.clsp",
        "1914e2d935d0b8d8b514e0d9c07098ce488000a7bcf1b826266cab5e3a265d82",
    ),
    (
        "upstream/genesis_by_coin_id.clsp.hex",
        "00cb7c17f4bf44ce835930f7d6a9d02c1b811e3cb0966e0482cf46338c960235",
    ),
    (
        "upstream/cat_truths.clib",
        "6dd8c22abf35e750cefda8005f3540078bd7f63749f38af6b0ae47e962fbbb9b",
    ),
    (
        "upstream/LICENSE",
        "d2db94fa34f37910c925ac45d98084a41c1bcb08296eb5525ead62e9f26aeb75",
    ),
];
#[derive(Serialize)]
pub(super) struct Provenance {
    pub repository: &'static str,
    pub revision: &'static str,
    pub tree: &'static str,
    pub license: &'static str,
    pub files: BTreeMap<String, String>,
    pub canonical_module_hash: String,
    pub canonical_module_bytes: usize,
    pub scope: &'static str,
}

pub(super) fn compile() -> Result<(Vec<u8>, Provenance)> {
    let root = source_root();
    let declared: Value =
        serde_json::from_slice(&read(&root.join(BASE).join("provenance.json"), 64 * 1024)?)?;
    ensure!(
        declared["schema_version"] == 1
            && declared["repository"] == "https://github.com/Chia-Network/chia_puzzles"
            && declared["revision"] == "d122456feeef55708d354a065623037bfa010714"
            && declared["root_tree_sha1"] == "89e5f55c5afdde2c436b9a2c08b8e30ecf11eb62"
            && declared["license"] == "Apache-2.0",
        "canonical CAT repository/tree/license pin"
    );
    let files = declared["files"]
        .as_array()
        .context("canonical declared files")?;
    ensure!(files.len() == 4, "complete canonical four-file provenance");
    let mut current = BTreeMap::new();
    let mut hashes = BTreeMap::new();
    for (name, expected) in PINS {
        let payload = read(&root.join(BASE).join(name), 32 * 1024)?;
        let sha = hex::encode(crate::digest(&payload));
        let rows = files
            .iter()
            .filter(|row| row["retained_path"] == name)
            .collect::<Vec<_>>();
        ensure!(
            sha == expected
                && rows.len() == 1
                && rows[0]["sha256"] == expected
                && rows[0]["bytes"] == payload.len(),
            "exact independently pinned canonical bytes"
        );
        current.insert(name, payload);
        hashes.insert(format!("{BASE}{name}"), sha);
    }
    let source = std::str::from_utf8(&current["upstream/genesis_by_coin_id.clsp"])?;
    let include = std::str::from_utf8(&current["upstream/cat_truths.clib"])?;
    let canonical =
        chialisp_oxide_reference::compile_with_includes(source, &[("cat_truths.clib", include)])?;
    let retained =
        hex::decode(std::str::from_utf8(&current["upstream/genesis_by_coin_id.clsp.hex"])?.trim())?;
    ensure!(
        canonical == retained && canonical.len() == 45,
        "independent canonical compilation must preserve exact published45byte artifact"
    );
    let hash = inspect(&canonical)?.identity().module_tree_hash.to_hex();
    ensure!(
        hash == "493afb89eed93ab86741b2aa61b8f5de495d33ff9b781dfc8919e602b2afa150"
            && declared["canonical_artifact"]["module_tree_hash"] == hash,
        "canonical module identity unchanged"
    );
    Ok((
        canonical,
        Provenance {
            repository: "https://github.com/Chia-Network/chia_puzzles",
            revision: "d122456feeef55708d354a065623037bfa010714",
            tree: "89e5f55c5afdde2c436b9a2c08b8e30ecf11eb62",
            license: "Apache-2.0",
            files: hashes,
            canonical_module_hash: hash,
            canonical_module_bytes: 45,
            scope: "Exact pinned source/include/license/artifact consistency and current independent Chialisp compilation; no deployed-source authentication or Rust module-hash compatibility claim.",
        },
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn current_pinned_canonical_source_compiles_to_exact_published_bytes() {
        let (module, report) = super::compile().unwrap();
        assert_eq!(module.len(), 45);
        assert_eq!(report.canonical_module_bytes, 45);
    }
}
