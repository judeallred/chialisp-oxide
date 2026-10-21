//! Selected-unit raw file observations. Not authenticated build provenance.
mod collector;
mod paths;
use collector::{
    CaptureError, CaptureHandle, CaptureLimits, ErrorKind, ReadKind, Result, SealedCapture,
    UnitIdentity,
};
use paths::{Classifier, SourceRoots};
use rustc_span::source_map::{FileLoader, RealFileLoader};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

struct RecordingLoader {
    inner: Box<dyn FileLoader + Send + Sync>,
    capture: CaptureHandle,
    classifier: Arc<dyn Classifier>,
}
impl FileLoader for RecordingLoader {
    fn file_exists(&self, path: &Path) -> bool {
        self.inner.file_exists(path)
    }
    fn current_directory(&self) -> io::Result<PathBuf> {
        let result = self.inner.current_directory();
        if let Ok(path) = &result
            && self
                .capture
                .charge_path(path.as_os_str().as_encoded_bytes().len())
            && let Err(error) = self.classifier.working_directory(path)
        {
            self.capture.fail(error);
        }
        result
    }
    fn read_file(&self, path: &Path) -> io::Result<String> {
        let ticket = self.capture.begin_read();
        let result = self.inner.read_file(path);
        if let Ok(bytes) = &result
            && ticket.recording()
            && self
                .capture
                .charge_path(path.as_os_str().as_encoded_bytes().len())
        {
            ticket.succeeded(
                self.classifier.classify(path),
                ReadKind::Text,
                bytes.as_bytes(),
            );
        }
        result
    }
    fn read_binary_file(&self, path: &Path) -> io::Result<Arc<[u8]>> {
        let ticket = self.capture.begin_read();
        let result = self.inner.read_binary_file(path);
        if let Ok(bytes) = &result
            && ticket.recording()
            && self
                .capture
                .charge_path(path.as_os_str().as_encoded_bytes().len())
        {
            ticket.succeeded(self.classifier.classify(path), ReadKind::Binary, bytes);
        }
        result
    }
}

pub(super) struct CaptureSession {
    capture: CaptureHandle,
    roots: Arc<SourceRoots>,
    installed: bool,
}
fn configuration(message: &'static str) -> CaptureError {
    CaptureError::new(ErrorKind::Configuration, message)
}
impl CaptureSession {
    /// Only called after the wrapper has selected this actual primary library.
    pub fn from_selected_environment(crate_name: &str, target: &str) -> Result<Option<Self>> {
        let Some(enable) = std::env::var_os("CHIALISP_OXIDE_SOURCE_RECEIPT") else {
            return Ok(None);
        };
        if enable != "1" {
            return Err(configuration(
                "CHIALISP_OXIDE_SOURCE_RECEIPT must be exactly 1 when present",
            ));
        }
        let package = std::env::var_os("CHIALISP_OXIDE_SOURCE_PACKAGE_ROOT")
            .ok_or_else(|| configuration("missing selected source package root"))?;
        let library = std::env::var_os("CHIALISP_OXIDE_SOURCE_LIBRARY")
            .ok_or_else(|| configuration("missing selected source library path"))?;
        let actual = std::env::var_os("CARGO_MANIFEST_DIR")
            .ok_or_else(|| configuration("selected Cargo package has no manifest directory"))?;
        let generated = std::env::var_os("OUT_DIR");
        let roots = Arc::new(SourceRoots::new(
            Path::new(&package),
            generated.as_deref().map(Path::new),
            Path::new(&library),
        )?);
        if !Path::new(&package).is_absolute()
            || !Path::new(&library).is_absolute()
            || Path::new(&actual)
                .canonicalize()
                .map_err(|_| configuration("cannot locate actual selected Cargo package"))?
                != roots.package()
        {
            return Err(configuration(
                "selected source package does not match Cargo's actual package",
            ));
        }
        let capture = CaptureHandle::new(
            UnitIdentity {
                crate_name: crate_name.into(),
                analysis_target: target.into(),
                features: vec![],
                library_source: roots.library_label()?,
            },
            CaptureLimits::default(),
        )?;
        Ok(Some(Self {
            capture,
            roots,
            installed: false,
        }))
    }
    pub fn install(&mut self, config: &mut rustc_interface::interface::Config) -> Result<()> {
        if self.installed {
            return Err(configuration("source recorder installed twice"));
        }
        let rustc_session::config::Input::File(path) = &config.input else {
            return Err(configuration(
                "source receipt requires a real Cargo library file",
            ));
        };
        if path
            .canonicalize()
            .map_err(|_| configuration("cannot locate selected compiler input"))?
            != self.roots.library()
        {
            return Err(configuration(
                "compiler input differs from Cargo metadata selected library",
            ));
        }
        let inner = config
            .file_loader
            .take()
            .unwrap_or_else(|| Box::new(RealFileLoader));
        config.file_loader = Some(Box::new(RecordingLoader {
            inner,
            capture: self.capture.clone(),
            classifier: self.roots.clone(),
        }));
        self.installed = true;
        Ok(())
    }
    pub fn record_features(&self, tcx: rustc_middle::ty::TyCtxt<'_>) {
        for (name, value) in tcx.sess.config.iter() {
            if *name == rustc_span::sym::feature
                && let Some(value) = value
            {
                self.capture.record_feature(value.as_str());
            }
        }
        self.capture.finish_features();
    }
    pub fn export_limit(&self) -> Result<usize> {
        self.capture.export_limit()
    }
    pub fn stage_export(&self, program: &chialisp_oxide_ir::Program) -> Result<StagedExport> {
        stage_export(&self.capture, &program.name, program)
    }
    pub fn finish(&self, work: &Path) -> Result<()> {
        if !self.installed {
            return Err(configuration("source recorder was not installed"));
        }
        self.capture.try_seal()?.write_private_unit(work)
    }
}

/// Retained pretty JSON is bounded before storage; whole Programs are not kept.
pub(super) struct StagedExport {
    name: String,
    filename: String,
    bytes: Vec<u8>,
}

fn stage_export<T: serde::Serialize>(
    capture: &CaptureHandle,
    name: &str,
    value: &T,
) -> Result<StagedExport> {
    if name.is_empty() || name.len() > 256 {
        let error = CaptureError::new(
            ErrorKind::ResourceLimit,
            "declared export name limit exceeded",
        );
        capture.fail(error.clone());
        return Err(error);
    }
    let filename = format!("{}.ir.json", collector::sha(name.as_bytes()));
    let maximum = capture.export_budget(name, &filename)?;
    let bytes = serialize_ir(value, maximum).inspect_err(|error| {
        capture.fail(error.clone());
    })?;
    capture.record_export_checked(name, &filename, &bytes)?;
    Ok(StagedExport {
        name: name.into(),
        filename,
        bytes,
    })
}

pub(super) fn write_staged_exports(work: &Path, mut exports: Vec<StagedExport>) -> Result<()> {
    exports.sort_by(|left, right| left.name.cmp(&right.name));
    std::fs::create_dir_all(work).map_err(file_error)?;
    for export in exports {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(work.join(export.filename))
            .map_err(file_error)?;
        file.write_all(&export.bytes).map_err(file_error)?;
    }
    Ok(())
}

fn file_error(_: io::Error) -> CaptureError {
    CaptureError::new(
        ErrorKind::Io,
        "cannot write complete private source receipt",
    )
}
fn compare_blob(path: &Path, expected: &[u8]) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path).map_err(file_error)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != expected.len() as u64
    {
        return Err(configuration(
            "existing private source blob is not the expected regular file",
        ));
    }
    let mut file = File::open(path).map_err(file_error)?;
    let mut offset: usize = 0;
    let mut chunk = [0; 8192];
    loop {
        let count = file.read(&mut chunk).map_err(file_error)?;
        if count == 0 {
            break;
        }
        let end = offset
            .checked_add(count)
            .ok_or_else(|| configuration("private source blob length overflow"))?;
        if expected.get(offset..end) != Some(&chunk[..count]) {
            return Err(configuration("private source blob collision"));
        }
        offset = end;
    }
    if offset != expected.len() {
        return Err(configuration("private source blob changed"));
    }
    Ok(())
}
struct JsonWriter {
    bytes: Vec<u8>,
    max: usize,
    exceeded: bool,
}
impl Write for JsonWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.max)
        {
            self.exceeded = true;
            return Err(io::Error::other("private source receipt JSON budget"));
        }
        let needed = self.bytes.len() + bytes.len();
        if needed > self.bytes.capacity() {
            let capacity = self
                .bytes
                .capacity()
                .saturating_mul(2)
                .max(needed)
                .min(self.max);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(|_| {
                    self.exceeded = true;
                    io::Error::other("private source receipt allocation")
                })?;
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn serialize_ir<T: serde::Serialize>(value: &T, maximum: usize) -> Result<Vec<u8>> {
    let mut writer = JsonWriter {
        bytes: vec![],
        max: maximum,
        exceeded: false,
    };
    serde_json::to_writer_pretty(&mut writer, value).map_err(|_| {
        CaptureError::new(
            if writer.exceeded {
                ErrorKind::ResourceLimit
            } else {
                ErrorKind::Io
            },
            "declared source receipt IR serialization failed",
        )
    })?;
    Ok(writer.bytes)
}
impl SealedCapture {
    fn write_private_unit(&self, work: &Path) -> Result<()> {
        let metadata = std::fs::symlink_metadata(work).map_err(file_error)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(configuration(
                "source receipt output must be a private plain directory",
            ));
        }
        let mut json = JsonWriter {
            bytes: vec![],
            max: self.json_limit,
            exceeded: false,
        };
        if serde_json::to_writer_pretty(&mut json, &self.receipt).is_err() {
            return Err(CaptureError::new(
                if json.exceeded {
                    ErrorKind::ResourceLimit
                } else {
                    ErrorKind::Io
                },
                "private source receipt serialization failed",
            ));
        }
        json.write_all(b"\n").map_err(|_| {
            CaptureError::new(
                ErrorKind::ResourceLimit,
                "private source receipt JSON budget",
            )
        })?;
        for (row, contents) in self.receipt.files.iter().zip(&self.contents) {
            let path = work.join(&row.blob);
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(mut file) => {
                    file.write_all(contents).map_err(file_error)?;
                    file.sync_all().map_err(file_error)?;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    compare_blob(&path, contents)?
                }
                Err(error) => return Err(file_error(error)),
            }
        }
        let temporary = work.join(".source-unit.json.tmp");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(file_error)?;
        file.write_all(&json.bytes).map_err(file_error)?;
        file.sync_all().map_err(file_error)?;
        drop(file);
        std::fs::hard_link(&temporary, work.join("source-unit.json")).map_err(file_error)?;
        #[cfg(unix)]
        File::open(work)
            .and_then(|directory| directory.sync_all())
            .map_err(file_error)?;
        std::fs::remove_file(temporary).map_err(file_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
