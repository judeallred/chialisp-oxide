//! Automatically declared selected-unit files, never compiler authentication.
use anyhow::{Context, Result, bail, ensure};
use chialisp_oxide_package::{
    Digest32, Error, ErrorKind, Limits, digest,
    receipt::{
        BuildDeclaration, ExportDeclaration, FileDeclaration, ObservationKind, OmittedReads,
        RECEIPT_PROTOCOL, RECEIPT_SCOPE, SnapshotDeclaration, SourceReceipt, UnitDeclaration,
    },
};
use serde::{
    Deserialize,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
};

const UNIT_NAME: &str = "source-unit.json";
const MAX_UNIT_BYTES: usize = 2 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 4096;
const MAX_READS: u64 = 16_384;
const MAX_READ_BYTES: u64 = 64 * 1024 * 1024;
const MAX_FEATURES: usize = 128;
const MAX_FEATURE_BYTES: usize = 64 * 1024;

fn resource(message: &str) -> anyhow::Error {
    Error::new(ErrorKind::ResourceLimit, message).into()
}

fn bounded_path(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path.as_os_str().as_encoded_bytes().len() <= MAX_PATH_BYTES
            && path.components().count() <= 256,
        "source receipt path is empty or exceeds local bounds"
    );
    Ok(())
}

/// Inputs are canonical local paths; reject symlink/special components before I/O.
fn read_regular(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    bounded_path(path)?;
    let mut current = PathBuf::new();
    for component in path.components() {
        ensure!(
            !matches!(component, Component::ParentDir | Component::CurDir),
            "source receipt path is not normalized"
        );
        current.push(component.as_os_str());
        let metadata = std::fs::symlink_metadata(&current)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "source receipt path contains a symlink"
        );
        if current != path {
            ensure!(
                metadata.is_dir(),
                "source receipt parent is not a directory"
            );
        } else {
            ensure!(
                metadata.is_file(),
                "source receipt input is not a regular file"
            );
            if metadata.len() > maximum as u64 {
                return Err(resource("source receipt input exceeds byte limit"));
            }
        }
    }
    let capacity = maximum
        .checked_add(1)
        .context("source receipt byte bound overflow")?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(capacity as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(resource("source receipt input exceeds byte limit"));
    }
    Ok(bytes)
}

pub(super) fn read_ir(path: &Path, total: &mut usize) -> Result<Vec<u8>> {
    read_ir_bounded(path, total, &Limits::default())
}

fn read_ir_bounded(path: &Path, total: &mut usize, limits: &Limits) -> Result<Vec<u8>> {
    read_ir_with_cap(path, total, limits, limits.max_ir_bytes)
}

fn read_ir_with_cap(
    path: &Path,
    total: &mut usize,
    limits: &Limits,
    maximum: usize,
) -> Result<Vec<u8>> {
    let remaining = limits
        .max_lookup_bytes
        .checked_sub(*total)
        .ok_or_else(|| resource("receipt IR reads exceed aggregate byte bound"))?;
    let bytes = read_regular(path, limits.max_ir_bytes.min(remaining).min(maximum))?;
    *total = total
        .checked_add(bytes.len())
        .context("receipt IR read accounting overflow")?;
    Ok(bytes)
}

fn canonical(path: &Path) -> Result<PathBuf> {
    bounded_path(path)?;
    let path = path
        .canonicalize()
        .with_context(|| format!("locate receipt input {}", path.display()))?;
    bounded_path(&path)?;
    Ok(path)
}

fn label(path: &Path, package: &Path, fallback: &str) -> Result<String> {
    let label = if let Ok(relative) = path.strip_prefix(package) {
        format!(
            "package/{}",
            relative.to_str().context("source label must be UTF-8")?
        )
    } else {
        fallback.to_owned()
    };
    validate_label(&label)?;
    Ok(label)
}

fn validate_label(label: &str) -> Result<()> {
    ensure!(
        !label.is_empty()
            && label.len() <= MAX_PATH_BYTES
            && !label.contains('\\')
            && !label.chars().any(char::is_control)
            && !label
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            && Path::new(label)
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "invalid portable source label"
    );
    Ok(())
}

struct Snapshot {
    path: PathBuf,
    label: String,
    bytes: Vec<u8>,
    kind: ObservationKind,
}

pub(super) struct CapturePlan {
    package_root: PathBuf,
    workspace_root: PathBuf,
    library: PathBuf,
    crate_name: String,
    snapshots: Vec<Snapshot>,
    refs: SnapshotDeclaration,
    limits: Limits,
}

pub(super) struct BuildContext<'a> {
    pub package: &'a str,
    pub package_id: &'a str,
    pub compiler: &'a str,
    pub toolchain: &'a str,
    pub analysis_target: &'a str,
    pub driver_sha256: &'a str,
    pub rue_revision: &'a str,
    pub cargo_options: &'a [String],
    pub optimize_lir: bool,
}

impl CapturePlan {
    pub fn begin(
        package_manifest: &Path,
        workspace_root: &Path,
        library: &Path,
        build_script: Option<&Path>,
        crate_name: &str,
    ) -> Result<Self> {
        let limits = Limits::default();
        let package_manifest = canonical(package_manifest)?;
        let package_root = package_manifest
            .parent()
            .context("package manifest parent")?
            .to_path_buf();
        let workspace_root = canonical(workspace_root)?;
        let library = canonical(library)?;
        let mut snapshots = Vec::new();
        let mut snapshot =
            |path: PathBuf, fallback: &str, kind: ObservationKind| -> Result<String> {
                let path = canonical(&path)?;
                let label = label(&path, &package_root, fallback)?;
                let bytes = read_regular(&path, limits.max_source_bytes)?;
                snapshots.push(Snapshot {
                    path,
                    label: label.clone(),
                    bytes,
                    kind,
                });
                Ok(label)
            };
        let package_manifest = snapshot(
            package_manifest,
            "package/Cargo.toml",
            ObservationKind::LauncherManifest,
        )?;
        let workspace_manifest = snapshot(
            workspace_root.join("Cargo.toml"),
            "workspace/Cargo.toml",
            ObservationKind::LauncherManifest,
        )?;
        let build_script = build_script
            .map(|path| {
                snapshot(
                    path.to_path_buf(),
                    "entry/build-script.rs",
                    ObservationKind::LauncherBuildScriptSource,
                )
            })
            .transpose()?;
        Ok(Self {
            package_root,
            workspace_root,
            library,
            crate_name: crate_name.into(),
            snapshots,
            refs: SnapshotDeclaration {
                package_manifest,
                workspace_manifest,
                lockfile: None,
                build_script,
            },
            limits,
        })
    }

    pub fn configure(&self, command: &mut Command) {
        command
            .env("CHIALISP_OXIDE_SOURCE_RECEIPT", "1")
            .env("CHIALISP_OXIDE_SOURCE_PACKAGE_ROOT", &self.package_root)
            .env("CHIALISP_OXIDE_SOURCE_LIBRARY", &self.library);
    }

    pub fn finish(
        mut self,
        work: &Path,
        context: &BuildContext<'_>,
        exports: &[ExportDeclaration],
    ) -> Result<SourceReceipt> {
        let bytes = read_regular(&work.join(UNIT_NAME), MAX_UNIT_BYTES)
            .context("read successful selected driver source receipt (use --artifact-only explicitly to omit capture)")?;
        preflight_unit(&bytes)?;
        let unit: DriverUnit =
            serde_json::from_slice(&bytes).context("decode closed driver source unit")?;
        ensure!(
            unit.schema_version == 1
                && unit.protocol == "chialisp-oxide-driver-source-unit/v1"
                && unit.scope == "selected_unit_file_loader_reads_only",
            "unsupported driver source-unit protocol"
        );
        ensure!(
            unit.unit.crate_name == self.crate_name
                && unit.unit.analysis_target == context.analysis_target,
            "selected source-unit identity differs from Cargo metadata"
        );
        ensure!(
            unit.unit.features.len() <= MAX_FEATURES && sorted_unique(&unit.unit.features),
            "invalid selected feature declaration"
        );
        let feature_bytes = unit.unit.features.iter().try_fold(0usize, |sum, value| {
            sum.checked_add(value.len())
                .context("feature length overflow")
        })?;
        ensure!(
            feature_bytes <= MAX_FEATURE_BYTES,
            "selected feature declaration exceeds byte bound"
        );
        ensure!(
            !unit.exports.is_empty() && unit.exports.len() <= self.limits.max_objects,
            "invalid driver export count"
        );
        ensure!(
            unit.exports
                .windows(2)
                .all(|pair| pair[0].entry < pair[1].entry),
            "driver exports must be sorted unique"
        );
        let mut expected: Vec<_> = exports.iter().collect();
        expected.sort_by(|left, right| left.entry.cmp(&right.entry));
        ensure!(
            unit.exports.len() == expected.len(),
            "driver receipt does not cover every emitted export"
        );
        let mut ir_total = 0usize;
        for (declared, expected) in unit.exports.iter().zip(expected) {
            ensure!(
                declared == expected,
                "driver receipt IR identity differs from complete compiled export set"
            );
            let filename = format!("{}.ir.json", digest(declared.entry.as_bytes()));
            ensure!(
                declared.ir == filename,
                "driver source-unit IR filename is not derived from the export"
            );
            let declared_length = usize::try_from(declared.bytes)
                .context("declared IR length exceeds host bounds")?;
            let ir = read_ir_with_cap(
                &work.join(&declared.ir),
                &mut ir_total,
                &self.limits,
                declared_length,
            )?;
            ensure!(
                declared.bytes == ir.len() as u64 && declared.sha256 == digest(&ir),
                "driver receipt IR bytes changed"
            );
        }
        ensure!(
            unit.files.len() <= self.limits.max_sources
                && unit
                    .files
                    .windows(2)
                    .all(|pair| pair[0].path < pair[1].path),
            "driver source files must be bounded sorted unique"
        );
        // Omitted bytes were returned unchanged, never copied or hashed by the
        // collector. Their checked-u64 sum is not its attached-read work budget.
        ensure!(
            unit.omitted.outside_roots_reads <= MAX_READS,
            "outside-read event counter exceeds recorder bound"
        );
        ensure!(
            unit.omitted.outside_roots_reads != 0 || unit.omitted.outside_roots_bytes == 0,
            "outside-read byte count requires a successful read"
        );
        let mut files = BTreeMap::new();
        let mut blobs = BTreeMap::<String, Vec<u8>>::new();
        let mut total = 0usize;
        let mut reads = unit.omitted.outside_roots_reads;
        let mut read_bytes = 0u64;
        for file in unit.files {
            validate_label(&file.path)?;
            ensure!(
                file.path.starts_with("package/")
                    || file.path.starts_with("generated/")
                    || file.path == "entry/library.rs",
                "driver retained an unsupported source root"
            );
            ensure!(
                !file.read_kinds.is_empty()
                    && file.read_kinds.len() <= 2
                    && sorted_unique(&file.read_kinds),
                "invalid driver read-kind set"
            );
            let kinds = file
                .read_kinds
                .iter()
                .map(|kind| match kind.as_str() {
                    "binary" => Ok(ObservationKind::CompilerBinary),
                    "text" => Ok(ObservationKind::CompilerText),
                    _ => bail!("unknown driver read kind"),
                })
                .collect::<Result<Vec<_>>>()?;
            ensure!(
                file.read_count >= kinds.len() as u64,
                "driver source row has too few successful reads"
            );
            reads = reads
                .checked_add(file.read_count)
                .context("source read count overflow")?;
            read_bytes = file
                .bytes
                .checked_mul(file.read_count)
                .and_then(|bytes| read_bytes.checked_add(bytes))
                .context("source repeated-read byte accounting overflow")?;
            ensure!(
                reads <= MAX_READS && read_bytes <= MAX_READ_BYTES,
                "driver source read work exceeds recorder bounds"
            );
            let length =
                usize::try_from(file.bytes).context("source length exceeds host bounds")?;
            charge_source(&mut total, length, &self.limits)?;
            ensure!(
                file.blob == format!("source-{}.bin", file.sha256),
                "source blob filename does not match its digest"
            );
            let content = if let Some(bytes) = blobs.get(&file.blob) {
                ensure!(
                    bytes.len() == length,
                    "shared source blob has conflicting lengths"
                );
                bytes
            } else {
                let content = read_regular(&work.join(&file.blob), length)?;
                ensure!(
                    content.len() == length && digest(&content) == file.sha256,
                    "driver source blob content mismatch"
                );
                blobs.insert(file.blob.clone(), content);
                blobs.get(&file.blob).context("inserted source blob")?
            };
            ensure!(content.len() == length, "source blob length mismatch");
            files.insert(
                file.path.clone(),
                FileDeclaration {
                    path: file.path,
                    kinds,
                    compiler_read_count: file.read_count,
                    bytes: file.bytes,
                    sha256: file.sha256,
                    blob: file.blob,
                },
            );
        }
        validate_label(&unit.unit.library_source)?;
        let library = files
            .get(&unit.unit.library_source)
            .context("driver did not retain its selected library")?;
        ensure!(
            library.kinds.contains(&ObservationKind::CompilerText),
            "selected library was not observed as compiler text"
        );
        let expected_library = label(&self.library, &self.package_root, "entry/library.rs")?;
        // OUT_DIR has higher driver priority. A source within it is identified by
        // the driver, which checks exact Config.input; normal roots are checked here.
        ensure!(
            unit.unit.library_source == expected_library
                || unit.unit.library_source.starts_with("generated/"),
            "selected library source label differs from metadata"
        );
        check_reserved_blobs(work, blobs.keys().map(String::as_str), &self.limits)?;
        for snapshot in self.snapshots {
            let maximum = snapshot.bytes.len().min(snapshot_read_cap(
                &snapshot.label,
                &files,
                total,
                &self.limits,
            )?);
            let after = read_regular(&snapshot.path, maximum).with_context(|| {
                format!(
                    "launcher source declaration changed during Cargo: {}",
                    snapshot.label
                )
            })?;
            ensure!(
                after == snapshot.bytes,
                "launcher source declaration changed during Cargo: {}",
                snapshot.label
            );
            merge_snapshot(
                work,
                snapshot,
                &mut files,
                &mut blobs,
                &mut total,
                &self.limits,
            )?;
        }
        let lockfile = self.workspace_root.join("Cargo.lock");
        match std::fs::symlink_metadata(&lockfile) {
            Ok(_) => {
                let path = canonical(&lockfile)?;
                let label = label(&path, &self.package_root, "workspace/Cargo.lock")?;
                let maximum = snapshot_read_cap(&label, &files, total, &self.limits)?;
                let bytes = read_regular(&path, maximum)?;
                self.refs.lockfile = Some(label.clone());
                merge_snapshot(
                    work,
                    Snapshot {
                        path,
                        label,
                        bytes,
                        kind: ObservationKind::LauncherLockfilePostCargo,
                    },
                    &mut files,
                    &mut blobs,
                    &mut total,
                    &self.limits,
                )?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if files.len() > self.limits.max_sources {
            return Err(resource("too many automatically declared source files"));
        }
        let receipt = SourceReceipt {
            protocol: RECEIPT_PROTOCOL.into(),
            scope: RECEIPT_SCOPE.into(),
            build: BuildDeclaration {
                package: context.package.into(),
                package_id: context.package_id.into(),
                compiler_sha256: digest(context.compiler.as_bytes()),
                toolchain: context.toolchain.into(),
                analysis_target: context.analysis_target.into(),
                driver_sha256: context.driver_sha256.parse()?,
                rue_revision: context.rue_revision.into(),
                cargo_options: context.cargo_options.to_vec(),
                optimize_lir: context.optimize_lir,
            },
            unit: UnitDeclaration {
                crate_name: unit.unit.crate_name,
                analysis_target: unit.unit.analysis_target,
                features: unit.unit.features,
                library_source: unit.unit.library_source,
            },
            exports: unit.exports,
            files: files.into_values().collect(),
            snapshots: self.refs,
            omitted: unit.omitted,
        };
        std::fs::remove_file(work.join(UNIT_NAME))?;
        Ok(receipt)
    }
}

fn sorted_unique(values: &[String]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn charge_source(total: &mut usize, length: usize, limits: &Limits) -> Result<()> {
    if length > limits.max_source_bytes {
        return Err(resource("declared source exceeds per-file limit"));
    }
    *total = total
        .checked_add(length)
        .context("declared source byte accounting overflow")?;
    if *total > limits.max_total_source_bytes {
        return Err(resource(
            "declared sources exceed expanded aggregate byte limit",
        ));
    }
    Ok(())
}

fn snapshot_read_cap(
    label: &str,
    files: &BTreeMap<String, FileDeclaration>,
    total: usize,
    limits: &Limits,
) -> Result<usize> {
    if let Some(row) = files.get(label) {
        return Ok(usize::try_from(row.bytes)
            .context("declared source length exceeds host bounds")?
            .min(limits.max_source_bytes));
    }
    if files.len() >= limits.max_sources {
        return Err(resource("too many declared source labels"));
    }
    Ok(limits
        .max_total_source_bytes
        .checked_sub(total)
        .ok_or_else(|| resource("declared sources exceed expanded aggregate byte limit"))?
        .min(limits.max_source_bytes))
}

fn merge_snapshot(
    work: &Path,
    snapshot: Snapshot,
    files: &mut BTreeMap<String, FileDeclaration>,
    blobs: &mut BTreeMap<String, Vec<u8>>,
    total: &mut usize,
    limits: &Limits,
) -> Result<()> {
    if let Some(file) = files.get_mut(&snapshot.label) {
        let bytes = blobs
            .get(&file.blob)
            .context("source row has no retained content")?;
        ensure!(
            *bytes == snapshot.bytes,
            "launcher and compiler observed conflicting bytes for {}",
            snapshot.label
        );
        if !file.kinds.contains(&snapshot.kind) {
            file.kinds.push(snapshot.kind);
            file.kinds.sort();
        }
        return Ok(());
    }
    if files.len() >= limits.max_sources {
        return Err(resource("too many declared source labels"));
    }
    charge_source(total, snapshot.bytes.len(), limits)?;
    let sha256 = digest(&snapshot.bytes);
    let blob = format!("source-{sha256}.bin");
    if let Some(existing) = blobs.get(&blob) {
        ensure!(
            *existing == snapshot.bytes,
            "source blob identity has conflicting bytes"
        );
    } else {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(work.join(&blob))?;
        file.write_all(&snapshot.bytes)?;
        file.sync_all()?;
        blobs.insert(blob.clone(), snapshot.bytes);
    }
    files.insert(
        snapshot.label.clone(),
        FileDeclaration {
            path: snapshot.label,
            kinds: vec![snapshot.kind],
            compiler_read_count: 0,
            bytes: blobs.get(&blob).context("new source blob missing")?.len() as u64,
            sha256,
            blob,
        },
    );
    Ok(())
}

fn check_reserved_blobs<'a>(
    work: &Path,
    expected: impl Iterator<Item = &'a str>,
    limits: &Limits,
) -> Result<()> {
    let mut expected: BTreeSet<_> = expected.map(str::to_owned).collect();
    let maximum = limits
        .max_objects
        .checked_mul(4)
        .and_then(|n| n.checked_add(limits.max_sources + 2))
        .context("bundle inventory bound overflow")?;
    for (count, entry) in std::fs::read_dir(work)?.enumerate() {
        if count >= maximum {
            return Err(resource("source-unit directory exceeds entry bound"));
        }
        let entry = entry?;
        ensure!(
            entry.file_type()?.is_file(),
            "unexpected nonregular extraction output"
        );
        let name = entry.file_name();
        let name = name
            .to_str()
            .context("extraction output filename must be UTF-8")?;
        if name.starts_with("source-") && name.ends_with(".bin") {
            ensure!(
                expected.remove(name),
                "undeclared reserved source blob in driver output"
            );
        }
    }
    ensure!(expected.is_empty(), "missing declared driver source blob");
    Ok(())
}

/// Final publication follows this closed inventory, never arbitrary work files.
pub(super) fn publication_files(
    exports: &[ExportDeclaration],
    receipt: Option<&SourceReceipt>,
) -> Result<BTreeSet<String>> {
    let mut files = BTreeSet::from(["manifest.json".to_owned()]);
    for export in exports {
        let stem = export
            .ir
            .strip_suffix(".ir.json")
            .context("export IR suffix")?;
        for name in [
            export.ir.clone(),
            format!("{stem}.clvm.hex"),
            format!("{stem}.clvm.txt"),
            format!("{stem}.artifact.json"),
        ] {
            ensure!(files.insert(name), "duplicate extraction output filename");
        }
    }
    if let Some(receipt) = receipt {
        for file in &receipt.files {
            files.insert(file.blob.clone());
        }
    }
    Ok(files)
}

pub(super) fn check_publication_files(work: &Path, expected: &BTreeSet<String>) -> Result<()> {
    let mut remaining = expected.clone();
    for (count, entry) in std::fs::read_dir(work)?.enumerate() {
        ensure!(count < expected.len(), "unexpected extra extraction output");
        let entry = entry?;
        ensure!(
            entry.file_type()?.is_file(),
            "unexpected nonregular extraction output"
        );
        let name = entry.file_name();
        ensure!(
            remaining.remove(name.to_str().context("extraction filename must be UTF-8")?),
            "undeclared extraction output"
        );
    }
    ensure!(remaining.is_empty(), "missing extraction output");
    Ok(())
}

pub(super) fn write_manifest(path: &Path, manifest: &serde_json::Value) -> Result<()> {
    struct BoundedWriter {
        file: std::fs::File,
        remaining: usize,
    }
    impl Write for BoundedWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.remaining {
                return Err(std::io::Error::other(
                    "Cargo manifest exceeds local byte limit",
                ));
            }
            self.file.write_all(bytes)?;
            self.remaining -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.file.flush()
        }
    }
    let mut writer = BoundedWriter {
        file: std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?,
        remaining: Limits::default().max_envelope_bytes,
    };
    serde_json::to_writer_pretty(&mut writer, manifest)?;
    writer.file.sync_all()?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DriverUnit {
    schema_version: u32,
    protocol: String,
    scope: String,
    unit: DriverIdentity,
    exports: Vec<ExportDeclaration>,
    files: Vec<DriverFile>,
    omitted: OmittedReads,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DriverIdentity {
    crate_name: String,
    analysis_target: String,
    features: Vec<String>,
    library_source: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DriverFile {
    path: String,
    read_kinds: Vec<String>,
    read_count: u64,
    bytes: u64,
    sha256: Digest32,
    blob: String,
}

// Bound the entire private wire before allocating typed strings/collections.
// Its fixed schema is shallow; a generic bounded scan also rejects duplicate keys.
fn preflight_unit(bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_UNIT_BYTES {
        return Err(resource("source-unit JSON exceeds byte limit"));
    }
    struct Scan<'a> {
        nodes: &'a mut usize,
        depth: usize,
    }
    impl<'de> DeserializeSeed<'de> for Scan<'_> {
        type Value = ();
        fn deserialize<D: de::Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> std::result::Result<(), D::Error> {
            *self.nodes += 1;
            if *self.nodes > 65_536 || self.depth > 8 {
                return Err(de::Error::custom("source-unit JSON structural limit"));
            }
            deserializer.deserialize_any(self)
        }
    }
    impl<'de> Visitor<'de> for Scan<'_> {
        type Value = ();
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("bounded source-unit JSON")
        }
        fn visit_bool<E: de::Error>(self, _: bool) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_unit<E: de::Error>(self) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<(), E> {
            if value.len() > MAX_PATH_BYTES {
                return Err(E::custom("source-unit JSON string limit"));
            }
            Ok(())
        }
        fn visit_seq<A: SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> std::result::Result<(), A::Error> {
            let mut count = 0;
            while sequence
                .next_element_seed(Scan {
                    nodes: self.nodes,
                    depth: self.depth + 1,
                })?
                .is_some()
            {
                count += 1;
                if count > 4096 {
                    return Err(de::Error::custom("source-unit JSON array limit"));
                }
            }
            Ok(())
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<(), A::Error> {
            struct Key;
            impl<'de> DeserializeSeed<'de> for Key {
                type Value = String;
                fn deserialize<D: de::Deserializer<'de>>(
                    self,
                    d: D,
                ) -> std::result::Result<String, D::Error> {
                    d.deserialize_str(self)
                }
            }
            impl Visitor<'_> for Key {
                type Value = String;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("bounded object key")
                }
                fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<String, E> {
                    if value.len() > 64 {
                        return Err(E::custom("source-unit key limit"));
                    }
                    Ok(value.to_owned())
                }
            }
            let mut keys = BTreeSet::new();
            while let Some(key) = map.next_key_seed(Key)? {
                if keys.len() >= 16 || !keys.insert(key) {
                    return Err(de::Error::custom(
                        "source-unit object size or duplicate key",
                    ));
                }
                map.next_value_seed(Scan {
                    nodes: self.nodes,
                    depth: self.depth + 1,
                })?;
            }
            Ok(())
        }
    }
    let mut nodes = 0;
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    Scan {
        nodes: &mut nodes,
        depth: 0,
    }
    .deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(())
}

#[cfg(test)]
mod tests;
