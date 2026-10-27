use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

const FORBIDDEN_ENV: &[&str] = &[
    "RUST_BIGDECIMAL_DEFAULT_PRECISION",
    "RUST_BIGDECIMAL_DEFAULT_ROUNDING_MODE",
    "RUST_BIGDECIMAL_FMT_EXPONENTIAL_LOWER_THRESHOLD",
    "RUST_BIGDECIMAL_FMT_EXPONENTIAL_UPPER_THRESHOLD",
    "RUST_BIGDECIMAL_FMT_MAX_INTEGER_PADDING",
    "RUST_BIGDECIMAL_SERDE_SCALE_LIMIT",
];
const MAX_FILE: u64 = 4 * 1024 * 1024;
const MAX_TOTAL: u64 = 64 * 1024 * 1024;
const MAX_FILES: usize = 1024;
const MAX_ENTRIES: usize = 16_384;

fn retain(root: &Path, path: &Path, rows: &mut Vec<(String, String)>, remaining: &mut u64) {
    let before = fs::symlink_metadata(path).unwrap();
    assert!(before.is_file() && !before.file_type().is_symlink());
    assert!(before.len() <= MAX_FILE && before.len() <= *remaining && rows.len() < MAX_FILES);
    let maximum = MAX_FILE.min(*remaining);
    let mut content = vec![];
    fs::File::open(path)
        .unwrap()
        .take(maximum + 1)
        .read_to_end(&mut content)
        .unwrap();
    assert!(content.len() as u64 == before.len() && content.len() as u64 <= maximum);
    *remaining -= content.len() as u64;
    println!("cargo:rerun-if-changed={}", path.display());
    rows.push((
        path.strip_prefix(root)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned(),
        format!("{:x}", Sha256::digest(content)),
    ));
}
fn visit(
    root: &Path,
    path: &Path,
    rows: &mut Vec<(String, String)>,
    remaining: &mut u64,
    work: &mut usize,
    depth: usize,
) {
    assert!(depth <= 32 && !fs::symlink_metadata(path).unwrap().file_type().is_symlink());
    let mut entries = vec![];
    for entry in fs::read_dir(path).unwrap() {
        *work += 1;
        assert!(*work <= MAX_ENTRIES);
        entries.push(entry.unwrap().path());
    }
    entries.sort();
    for path in entries {
        let name = path.file_name().unwrap().to_str().unwrap();
        if matches!(name, "target" | ".git") {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).unwrap();
        assert!(
            !metadata.file_type().is_symlink(),
            "native source snapshot refuses symlinks"
        );
        if metadata.is_dir() {
            visit(root, &path, rows, remaining, work, depth + 1);
        } else if [
            ".rs", ".clsp", ".clib", ".rue", ".toml", ".html", ".js", ".css",
        ]
        .iter()
        .any(|suffix| name.ends_with(suffix))
        {
            retain(root, &path, rows, remaining);
        }
    }
}
fn main() {
    for name in FORBIDDEN_ENV {
        println!("cargo:rerun-if-env-changed={name}");
        assert!(
            std::env::var_os(name).is_none(),
            "BigDecimal configuration must be unset during native compilation: {name}"
        );
    }
    let crate_dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let root = crate_dir.parent().unwrap().parent().unwrap();
    let mut rows = vec![];
    let mut remaining = MAX_TOTAL;
    let mut work = 0;
    // Broad local native/adapter/compiler source closure. Installed compiler and
    // standard-library bytes are separately frozen by FreshTools at execution.
    for relative in ["crates", "puzzles", "vendor/rue-lir", "experiments/backend"] {
        println!("cargo:rerun-if-changed={}", root.join(relative).display());
        visit(
            root,
            &root.join(relative),
            &mut rows,
            &mut remaining,
            &mut work,
            0,
        );
    }
    for relative in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        ".cargo/config.toml",
        "puzzles/authoring.json",
        "experiments/cat-genesis-tail/provenance.json",
        "experiments/cat-genesis-tail/vectors.json",
        "experiments/cat-genesis-tail/upstream/genesis_by_coin_id.clsp",
        "experiments/cat-genesis-tail/upstream/genesis_by_coin_id.clsp.hex",
        "experiments/cat-genesis-tail/upstream/cat_truths.clib",
        "experiments/cat-genesis-tail/upstream/LICENSE",
    ] {
        retain(root, &root.join(relative), &mut rows, &mut remaining);
    }
    // The linked corpus build script consumes this entire declaration, not
    // only the selected TAIL policy. Bind its external retained source inputs.
    let authoring_path = root.join("puzzles/authoring.json");
    assert!(fs::symlink_metadata(&authoring_path).unwrap().len() <= 1024 * 1024);
    let mut authoring_bytes = vec![];
    fs::File::open(&authoring_path)
        .unwrap()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut authoring_bytes)
        .unwrap();
    assert!(authoring_bytes.len() <= 1024 * 1024);
    let authoring: serde_json::Value = serde_json::from_slice(&authoring_bytes).unwrap();
    let declared = authoring["inputs"].as_array().unwrap();
    assert!(declared.len() <= MAX_FILES);
    for input in declared {
        let relative = input.as_str().unwrap();
        assert!(
            !Path::new(relative).is_absolute()
                && Path::new(relative)
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_)))
        );
        if !rows.iter().any(|(path, _)| path == relative) {
            retain(root, &root.join(relative), &mut rows, &mut remaining);
        }
    }
    rows.sort();
    assert!(
        rows.windows(2).all(|pair| pair[0].0 != pair[1].0),
        "duplicate native source path"
    );
    let host = std::env::var("HOST").unwrap();
    let source = format!(
        "const NATIVE_HOST: &str = {host:?};\nconst NATIVE_INPUTS: &[(&str, &str)] = &{rows:?};\nconst NATIVE_FORBIDDEN_ENV: &[&str] = &{FORBIDDEN_ENV:?};\n"
    );
    fs::write(
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("native_inputs.rs"),
        source,
    )
    .unwrap();
}
