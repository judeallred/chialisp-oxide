use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn visit(root: &Path, path: &Path, rows: &mut Vec<(String, String)>) {
    let mut entries = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
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
            visit(root, &path, rows);
        } else if [".rs", ".clsp", ".toml", ".html", ".js", ".css"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
        {
            assert!(metadata.len() <= 4 * 1024 * 1024 && rows.len() < 1024);
            let content = fs::read(&path).unwrap();
            assert!(content.len() <= 4 * 1024 * 1024 && rows.len() < 1024);
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
    }
}
fn main() {
    let crate_dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let root = crate_dir.parent().unwrap().parent().unwrap();
    let mut rows = vec![];
    for relative in [
        "crates/chialisp-oxide-lifecycle",
        "crates/chialisp-oxide",
        "crates/chialisp-oxide-macros",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(relative).display());
        visit(root, &root.join(relative), &mut rows);
    }
    for relative in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"] {
        let path = root.join(relative);
        println!("cargo:rerun-if-changed={}", path.display());
        rows.push((
            relative.into(),
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
        ));
    }
    rows.sort();
    let host = std::env::var("HOST").unwrap();
    let source = format!(
        "const NATIVE_HOST: &str = {host:?};\nconst NATIVE_INPUTS: &[(&str, &str)] = &{rows:?};\n"
    );
    fs::write(
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("native_inputs.rs"),
        source,
    )
    .unwrap();
}
