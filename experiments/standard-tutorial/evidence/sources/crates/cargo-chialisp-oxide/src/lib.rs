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

mod bundle_sniff;

const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug)]
pub struct ArtifactBundle {
    pub manifest_path: PathBuf,
    pub manifest: Value,
    /// Fully qualified Rust export names, in deterministic order.
    pub programs: BTreeMap<String, Program>,
    /// Present only after complete schema-2 bundle/source admission. This is a
    /// checked declaration, not authenticated build provenance.
    pub source_receipt: Option<chialisp_oxide_package::receipt::SourceReceipt>,
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

fn read_json_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_JSON_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_JSON_BYTES {
        return Err(chialisp_oxide_package::Error::new(
            chialisp_oxide_package::ErrorKind::ResourceLimit,
            format!("artifact JSON exceeds 16 MiB: {}", path.display()),
        )
        .into());
    }
    Ok(bytes)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_slice(&read_json_bytes(path)?)
        .with_context(|| format!("decode artifact {}", path.display()))
}

/// Load the latest immutable bundle and validate every typed IR export.
///
/// The returned manifest retains compiler/package metadata for the caller to
/// compare against the build it requested. CLVM files are not used to establish
/// IR semantics; callers can independently compile and validate these programs.
pub fn load_bundle(output: &Path) -> Result<ArtifactBundle> {
    load_bundle_with_legacy_decoder(output, |bytes| {
        serde_json::from_slice(bytes).context("decode legacy artifact manifest")
    })
}

// Keep the weak schema-1 transport contract explicit. Tests supply a failing
// decoder to prove schema-2 and bounded-preflight paths never parse a Value here.
fn load_bundle_with_legacy_decoder(
    output: &Path,
    legacy_decoder: impl FnOnce(&[u8]) -> Result<Value>,
) -> Result<ArtifactBundle> {
    let output = output
        .canonicalize()
        .context("locate Cargo chialisp-oxide output directory")?;
    let limits = chialisp_oxide_package::Limits::default();
    let pointer = bundle_sniff::pointer(
        &read_json_bytes(&confined_file(&output, "latest.json")?)?,
        &limits,
    )?;
    let relative = pointer
        .manifest
        .as_deref()
        .context("latest.json requires a manifest path")?;
    let manifest_path = confined_file(&output, relative)?;
    let root = manifest_path
        .parent()
        .context("artifact manifest has no bundle directory")?;
    let manifest_bytes = read_json_bytes(&manifest_path)?;
    if bundle_sniff::source_aware(&manifest_bytes, &limits)? {
        // Preserve the original weak schema-1 compatibility path below. A new
        // receipt can never be silently discarded by that legacy path.
        let bundle = pointer
            .bundle
            .as_deref()
            .context("schema-2 pointer requires a bundle name")?;
        ensure!(
            pointer.fields == 2
                && !bundle.is_empty()
                && !bundle.contains(['/', '\\'])
                && !matches!(bundle, "." | "..")
                && relative == format!("{bundle}/manifest.json"),
            "invalid schema-2 atomic bundle pointer"
        );
        ensure!(
            !std::fs::symlink_metadata(output.join("latest.json"))?
                .file_type()
                .is_symlink(),
            "schema-2 latest pointer is a symlink"
        );
        let admitted =
            chialisp_oxide_package::bundle::admit_bundle(&output.join(relative), &limits)
                .context("admit complete schema-2 Cargo bundle and declared sources")?;
        ensure!(
            admitted.manifest_json().as_bytes() == manifest_bytes.as_slice(),
            "Cargo manifest changed during complete admission"
        );
        let manifest = serde_json::from_str(admitted.manifest_json())?;
        let source_receipt = admitted.source_receipt().cloned();
        let programs = admitted.into_programs();
        return Ok(ArtifactBundle {
            manifest_path,
            manifest,
            programs,
            source_receipt,
        });
    }
    let manifest = legacy_decoder(&manifest_bytes)?;
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
        source_receipt: None,
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
        assert!(bundle.source_receipt.is_none());
        assert_eq!(
            bundle
                .programs
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["puzzles::one::puzzle"]
        );
    }

    fn source_bundle() -> (Bundle, Value) {
        use chialisp_oxide_package::{
            digest,
            receipt::{RECEIPT_PROTOCOL, RECEIPT_SCOPE},
        };
        let fixture = Bundle::new();
        let ir = std::fs::read(fixture.0.join("run/one.ir.json")).unwrap();
        let compiler = "rustc constructed fixture\nhost: fixture-host\n";
        let mut files = Vec::new();
        for (path, contents, kinds, count) in [
            (
                "package/Cargo.toml",
                b"".as_slice(),
                json!(["launcher_manifest"]),
                0,
            ),
            (
                "package/src/lib.rs",
                b"abc".as_slice(),
                json!(["compiler_text"]),
                1,
            ),
        ] {
            let blob = format!("source-{}.bin", digest(contents));
            std::fs::write(fixture.0.join("run").join(&blob), contents).unwrap();
            files.push(json!({"path":path,"kinds":kinds,"compiler_read_count":count,"bytes":contents.len(),"sha256":digest(contents),"blob":blob}));
        }
        let manifest = json!({
            "schema_version":2,"status":"wip_prototype_not_ready_for_use","package":"puzzles","package_id":"fixture-puzzles@0.1.0",
            "manifest_path":"/declared/Cargo.toml","workspace_root":"/declared","compiler":compiler,"toolchain":"nightly-fixture","analysis_target":"fixture-host",
            "driver_sha256":"00".repeat(32),"rue_revision":"a".repeat(40),"cargo_options":[],
            "entries":[{"entry":"puzzles::one::puzzle","ir":"one.ir.json","clvm":"one.clvm.hex","artifact":{"backend":"rust_mir","optimize_lir":true,"bytes":1,"tree_hash":"4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a","clvm_hex":"80","disassembly":"()"}}],
            "source_receipt":{"protocol":RECEIPT_PROTOCOL,"scope":RECEIPT_SCOPE,
                "build":{"package":"puzzles","package_id":"fixture-puzzles@0.1.0","compiler_sha256":digest(compiler.as_bytes()),"toolchain":"nightly-fixture","analysis_target":"fixture-host","driver_sha256":"00".repeat(32),"rue_revision":"a".repeat(40),"cargo_options":[],"optimize_lir":true},
                "unit":{"crate_name":"puzzles","analysis_target":"fixture-host","features":[],"library_source":"package/src/lib.rs"},
                "exports":[{"entry":"puzzles::one::puzzle","ir":"one.ir.json","bytes":ir.len(),"sha256":digest(&ir)}],"files":files,
                "snapshots":{"package_manifest":"package/Cargo.toml","workspace_manifest":"package/Cargo.toml","lockfile":null,"build_script":null},
                "omitted":{"outside_roots_reads":0,"outside_roots_bytes":0}}
        });
        fixture.write(
            "latest.json",
            &json!({"bundle":"run","manifest":"run/manifest.json"}),
        );
        fixture.write("run/manifest.json", &manifest);
        std::fs::write(fixture.0.join("run/one.clvm.hex"), "80\n").unwrap();
        (fixture, manifest)
    }

    fn forbid_legacy_value(_: &[u8]) -> Result<Value> {
        panic!("schema-2/bounded routing called the legacy Value decoder")
    }

    #[test]
    fn v2_uses_complete_admission_and_returns_checked_source_declarations() {
        use chialisp_oxide_package::digest;
        let (fixture, manifest) = source_bundle();
        let loaded = load_bundle_with_legacy_decoder(&fixture.0, forbid_legacy_value).unwrap();
        assert_eq!(
            loaded.source_receipt.as_ref().unwrap().unit.library_source,
            "package/src/lib.rs"
        );
        assert_eq!(
            loaded.programs["puzzles::one::puzzle"].name,
            "puzzles::one::puzzle"
        );
        std::fs::write(
            fixture
                .0
                .join("run")
                .join(format!("source-{}.bin", digest(b"abc"))),
            b"abd",
        )
        .unwrap();
        assert!(
            load_bundle_with_legacy_decoder(&fixture.0, forbid_legacy_value).is_err(),
            "v2 source bytes were silently discarded"
        );
        let mut downgrade = manifest;
        downgrade["schema_version"] = json!(1);
        fixture.write("run/manifest.json", &downgrade);
        assert!(
            load_bundle_with_legacy_decoder(&fixture.0, forbid_legacy_value).is_err(),
            "receipt-bearing input downgraded to weak v1"
        );
    }

    #[test]
    fn routing_bounds_and_malformed_claims_precede_legacy_value_decoding() {
        use chialisp_oxide_package::{Error, ErrorKind, Limits};
        let deep = format!(
            "{{\"schema_version\":2,\"padding\":{}0{}}}",
            "[".repeat(97),
            "]".repeat(97)
        );
        let wide = format!(
            "{{\"schema_version\":2,\"padding\":[{}0]}}",
            "0,".repeat(Limits::default().max_json_nodes)
        );
        for (raw, kind) in [
            (deep, ErrorKind::ResourceLimit),
            (wide, ErrorKind::ResourceLimit),
            (
                r#"{"schema_version":1,"source_receipt":null}"#.into(),
                ErrorKind::InvalidManifest,
            ),
            (
                r#"{"schema_version":1,"source_receipt":{}}"#.into(),
                ErrorKind::InvalidManifest,
            ),
            (
                r#"{"schema_version":2,"source_receipt":null}"#.into(),
                ErrorKind::InvalidManifest,
            ),
            (
                r#"{"schema_version":2,"schema_\u0076ersion":1}"#.into(),
                ErrorKind::InvalidManifest,
            ),
            (
                r#"{"schema_version":1,"source_receipt":null,"source_receipt":null}"#.into(),
                ErrorKind::InvalidManifest,
            ),
        ] {
            let fixture = Bundle::new();
            fixture.write(
                "latest.json",
                &json!({"bundle":"run","manifest":"run/manifest.json"}),
            );
            std::fs::write(fixture.0.join("run/manifest.json"), raw).unwrap();
            let error =
                load_bundle_with_legacy_decoder(&fixture.0, forbid_legacy_value).unwrap_err();
            assert_eq!(
                error.downcast_ref::<Error>().unwrap().kind(),
                kind,
                "{error:#}"
            );
        }
        for raw in [
            r#"{"manifest":"run/manifest.json","mani\u0066est":"run/manifest.json"}"#.to_owned(),
            r#"{"manifest":"run/manifest.json","extra":0,"extra":1}"#.to_owned(),
            format!(
                "{{\"manifest\":\"run/manifest.json\",\"padding\":{}0{}}}",
                "[".repeat(97),
                "]".repeat(97)
            ),
        ] {
            let fixture = Bundle::new();
            std::fs::write(fixture.0.join("latest.json"), raw).unwrap();
            assert!(load_bundle_with_legacy_decoder(&fixture.0, forbid_legacy_value).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn source_admission_keeps_the_original_lexical_manifest_path() {
        let (fixture, _) = source_bundle();
        let root = fixture.0.join("run");
        std::fs::rename(root.join("manifest.json"), root.join("retained.json")).unwrap();
        std::os::unix::fs::symlink("retained.json", root.join("manifest.json")).unwrap();
        assert!(load_bundle_with_legacy_decoder(&fixture.0, forbid_legacy_value).is_err());
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
