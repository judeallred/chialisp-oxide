//! Read validated typed IR exports from a `cargo chialisp-oxide build` artifact bundle.
//!
//! Invoke a successful build into a fresh output directory before consuming
//! these exports. Loading checks internal identity and path confinement; the
//! manifest alone does not authenticate its compiler or source provenance.
use anyhow::{Context, Result, ensure};
use chialisp_oxide_ir::Program;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Component, Path, PathBuf},
};

const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug)]
pub struct ArtifactBundle {
    pub manifest_path: PathBuf,
    pub manifest: Value,
    /// Fully qualified Rust export names, in deterministic order.
    pub programs: BTreeMap<String, Program>,
}

fn confined_file(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    ensure!(
        !relative.is_empty()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "artifact path must be relative and cannot traverse directories: {relative}"
    );
    let path = root
        .join(path)
        .canonicalize()
        .with_context(|| format!("locate artifact {relative}"))?;
    ensure!(
        path.starts_with(root),
        "artifact escapes its bundle: {relative}"
    );
    ensure!(path.is_file(), "artifact is not a file: {relative}");
    Ok(path)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_JSON_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_JSON_BYTES,
        "artifact JSON exceeds 16 MiB: {}",
        path.display()
    );
    serde_json::from_slice(&bytes).with_context(|| format!("decode artifact {}", path.display()))
}

/// Load the latest immutable bundle and validate every typed IR export.
///
/// The returned manifest retains compiler/package metadata for the caller to
/// compare against the build it requested. CLVM files are not used to establish
/// IR semantics; callers can independently compile and validate these programs.
pub fn load_bundle(output: &Path) -> Result<ArtifactBundle> {
    let output = output
        .canonicalize()
        .context("locate Cargo chialisp-oxide output directory")?;
    let pointer: Value = read_json(&confined_file(&output, "latest.json")?)?;
    let relative = pointer["manifest"]
        .as_str()
        .context("latest.json requires a manifest path")?;
    let manifest_path = confined_file(&output, relative)?;
    let root = manifest_path
        .parent()
        .context("artifact manifest has no bundle directory")?;
    let manifest: Value = read_json(&manifest_path)?;
    ensure!(
        manifest["schema_version"] == 1,
        "unsupported Cargo chialisp-oxide artifact schema"
    );
    let entries = manifest["entries"]
        .as_array()
        .context("artifact manifest requires entries")?;
    ensure!(!entries.is_empty(), "artifact bundle contains no exports");
    let mut names = BTreeSet::new();
    let mut files = BTreeSet::new();
    let mut exports = Vec::new();
    for entry in entries {
        let name = entry["entry"]
            .as_str()
            .context("artifact entry requires a Rust export name")?;
        ensure!(
            !name.is_empty() && names.insert(name),
            "duplicate or empty artifact export name: {name}"
        );
        let relative = entry["ir"]
            .as_str()
            .context("artifact entry requires an IR path")?;
        let path = confined_file(root, relative)?;
        ensure!(
            files.insert(path.clone()),
            "artifact entries share an IR file: {relative}"
        );
        exports.push((name.to_owned(), path));
    }
    let mut programs = BTreeMap::new();
    for (name, path) in exports {
        let program: Program = read_json(&path)?;
        ensure!(
            program.name == name,
            "artifact export identity mismatch: manifest {name}, IR {}",
            program.name
        );
        program
            .validate()
            .with_context(|| format!("validate exported IR {name}"))?;
        programs.insert(name, program);
    }
    Ok(ArtifactBundle {
        manifest_path,
        manifest,
        programs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chialisp_oxide_ir::{Expr, Parameter, Type};
    use serde_json::json;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

    struct Bundle(PathBuf);
    impl Bundle {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "chialisp-oxide-artifact-loader-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
            ));
            std::fs::create_dir_all(root.join("run")).unwrap();
            let this = Self(root);
            this.write("latest.json", &json!({"manifest":"run/manifest.json"}));
            this.write("run/manifest.json",&json!({"schema_version":1,"compiler":"test compiler","package":"puzzles","entries":[{"entry":"puzzles::one::puzzle","ir":"one.ir.json"}]}));
            this.write(
                "run/one.ir.json",
                &serde_json::to_value(Program {
                    functions: vec![],
                    environment: chialisp_oxide_ir::Environment::Strict,
                    name: "puzzles::one::puzzle".into(),
                    parameters: vec![Parameter {
                        name: "value".into(),
                        ty: Type::Int {
                            bits: 8,
                            signed: false,
                        },
                    }],
                    result: Type::Int {
                        bits: 8,
                        signed: false,
                    },
                    body: Expr::Arg { index: 0 },
                })
                .unwrap(),
            );
            this
        }
        fn write(&self, path: &str, value: &Value) {
            std::fs::write(self.0.join(path), serde_json::to_vec(value).unwrap()).unwrap();
        }
        fn manifest(&self) -> Value {
            read_json(&self.0.join("run/manifest.json")).unwrap()
        }
    }
    impl Drop for Bundle {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn loads_valid_typed_exports_and_retains_manifest_metadata() {
        let fixture = Bundle::new();
        let bundle = load_bundle(&fixture.0).unwrap();
        assert_eq!(bundle.manifest["compiler"], "test compiler");
        assert_eq!(
            bundle
                .programs
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["puzzles::one::puzzle"]
        );
    }

    #[test]
    fn rejects_ambiguous_identity_invalid_ir_and_unsupported_schema() {
        for mutation in [
            "duplicate-name",
            "duplicate-file",
            "identity",
            "invalid-ir",
            "schema",
        ] {
            let fixture = Bundle::new();
            let mut manifest = fixture.manifest();
            match mutation {
                "duplicate-name" => {
                    let entry = manifest["entries"][0].clone();
                    manifest["entries"].as_array_mut().unwrap().push(entry);
                }
                "duplicate-file" => manifest["entries"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"entry":"puzzles::two::puzzle","ir":"one.ir.json"})),
                "identity" => manifest["entries"][0]["entry"] = "puzzles::other::puzzle".into(),
                "invalid-ir" => {
                    let mut ir: Value = read_json(&fixture.0.join("run/one.ir.json")).unwrap();
                    ir["body"]["index"] = 99.into();
                    fixture.write("run/one.ir.json", &ir);
                }
                "schema" => manifest["schema_version"] = 2.into(),
                _ => unreachable!(),
            }
            fixture.write("run/manifest.json", &manifest);
            assert!(load_bundle(&fixture.0).is_err(), "accepted {mutation}");
        }
    }

    #[test]
    fn rejects_manifest_and_ir_directory_traversal() {
        for (pointer, path) in [
            (true, "../outside.json"),
            (true, "/outside.json"),
            (false, "../one.ir.json"),
            (false, "/one.ir.json"),
        ] {
            let fixture = Bundle::new();
            if pointer {
                fixture.write("latest.json", &json!({"manifest":path}));
            } else {
                let mut manifest = fixture.manifest();
                manifest["entries"][0]["ir"] = path.into();
                fixture.write("run/manifest.json", &manifest);
            }
            let error = load_bundle(&fixture.0).err().unwrap();
            assert!(error.to_string().contains("cannot traverse"), "{error:#}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_to_ir_outside_the_immutable_bundle() {
        let fixture = Bundle::new();
        std::fs::rename(
            fixture.0.join("run/one.ir.json"),
            fixture.0.join("outside.ir.json"),
        )
        .unwrap();
        std::os::unix::fs::symlink("../outside.ir.json", fixture.0.join("run/one.ir.json"))
            .unwrap();
        assert!(
            load_bundle(&fixture.0)
                .err()
                .unwrap()
                .to_string()
                .contains("escapes its bundle")
        );
    }
}
