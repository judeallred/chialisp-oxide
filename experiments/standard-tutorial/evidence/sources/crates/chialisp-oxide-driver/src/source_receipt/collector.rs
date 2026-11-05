//! Rustc-independent bounded observations of exact FileLoader return bytes.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ErrorKind {
    Configuration,
    ResourceLimit,
    ConflictingRead,
    PendingReads,
    Sealed,
    LockPoisoned,
    Io,
}
#[derive(Clone, Debug)]
pub(crate) struct CaptureError {
    pub kind: ErrorKind,
    pub message: &'static str,
}
impl CaptureError {
    pub fn new(kind: ErrorKind, message: &'static str) -> Self {
        Self { kind, message }
    }
}
impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}
impl std::error::Error for CaptureError {}
pub(crate) type Result<T> = std::result::Result<T, CaptureError>;
fn resource() -> CaptureError {
    CaptureError::new(
        ErrorKind::ResourceLimit,
        "declared source receipt budget exceeded",
    )
}
pub(super) fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Debug)]
pub(super) struct CaptureLimits {
    pub reads: usize,
    pub files: usize,
    pub path: usize,
    pub labels: usize,
    pub file_bytes: usize,
    pub total_bytes: usize,
    pub work: usize,
    pub exports: usize,
    pub ir_bytes: usize,
    pub total_ir_bytes: usize,
    pub features: usize,
    pub feature_bytes: usize,
    pub json_bytes: usize,
}
impl Default for CaptureLimits {
    fn default() -> Self {
        Self {
            reads: 16384,
            files: 4096,
            path: 4096,
            labels: 2 * 1024 * 1024,
            file_bytes: 2 * 1024 * 1024,
            total_bytes: 8 * 1024 * 1024,
            work: 64 * 1024 * 1024,
            exports: 1024,
            ir_bytes: 8 * 1024 * 1024,
            total_ir_bytes: 64 * 1024 * 1024,
            features: 128,
            feature_bytes: 64 * 1024,
            json_bytes: 2 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ReadKind {
    Binary,
    Text,
}
pub(super) enum Disposition {
    Attach(String),
    OutsideRoots,
}
#[derive(Debug, Serialize)]
pub(super) struct UnitIdentity {
    pub crate_name: String,
    pub analysis_target: String,
    pub features: Vec<String>,
    pub library_source: String,
}
#[derive(Debug, Serialize)]
pub(super) struct ExportRecord {
    pub entry: String,
    pub ir: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Debug, Serialize)]
pub(super) struct FileRow {
    pub path: String,
    pub read_kinds: Vec<ReadKind>,
    pub read_count: u64,
    pub bytes: u64,
    pub sha256: String,
    pub blob: String,
}
struct FileRecord {
    row: FileRow,
    contents: Vec<u8>,
}
#[derive(Debug, Default, Serialize)]
pub(super) struct Omissions {
    pub outside_roots_reads: u64,
    pub outside_roots_bytes: u64,
}
#[derive(Debug, Serialize)]
pub(super) struct UnitReceipt {
    pub schema_version: u32,
    pub protocol: &'static str,
    pub scope: &'static str,
    pub unit: UnitIdentity,
    pub exports: Vec<ExportRecord>,
    pub files: Vec<FileRow>,
    pub omitted: Omissions,
}
pub(super) struct SealedCapture {
    pub receipt: UnitReceipt,
    pub contents: Vec<Vec<u8>>,
    pub json_limit: usize,
}
struct State {
    limits: CaptureLimits,
    unit: Option<UnitIdentity>,
    failed: Option<CaptureError>,
    sealed: bool,
    active: usize,
    calls: usize,
    labels: usize,
    retained: usize,
    work: usize,
    ir_bytes: usize,
    feature_bytes: usize,
    features_recorded: bool,
    files: Vec<FileRecord>,
    exports: Vec<ExportRecord>,
    omitted: Omissions,
}
impl State {
    fn export_budget(&self, entry: &str, ir: &str) -> Result<usize> {
        if entry.is_empty()
            || entry.len() > 256
            || ir.len() > 128
            || self.exports.len() >= self.limits.exports
        {
            return Err(resource());
        }
        if self
            .exports
            .iter()
            .any(|row| row.entry == entry || row.ir == ir)
        {
            return Err(CaptureError::new(
                ErrorKind::Configuration,
                "duplicate source-unit export",
            ));
        }
        Ok(self.limits.ir_bytes.min(
            self.limits
                .total_ir_bytes
                .checked_sub(self.ir_bytes)
                .ok_or_else(resource)?,
        ))
    }
    fn admit_export(&mut self, entry: &str, ir: &str, bytes: &[u8]) -> Result<()> {
        if bytes.len() > self.export_budget(entry, ir)? {
            return Err(resource());
        }
        self.exports.try_reserve(1).map_err(|_| resource())?;
        self.ir_bytes = self
            .ir_bytes
            .checked_add(bytes.len())
            .ok_or_else(resource)?;
        self.exports.push(ExportRecord {
            entry: entry.into(),
            ir: ir.into(),
            bytes: bytes.len() as u64,
            sha256: sha(bytes),
        });
        Ok(())
    }
    fn charge(&mut self, amount: usize) -> Result<()> {
        self.work = self
            .work
            .checked_add(amount)
            .filter(|n| *n <= self.limits.work)
            .ok_or_else(resource)?;
        Ok(())
    }
    fn admit_read(&mut self, disposition: Disposition, kind: ReadKind, bytes: &[u8]) -> Result<()> {
        let Disposition::Attach(label) = disposition else {
            self.omitted.outside_roots_reads = self
                .omitted
                .outside_roots_reads
                .checked_add(1)
                .ok_or_else(resource)?;
            self.omitted.outside_roots_bytes = self
                .omitted
                .outside_roots_bytes
                .checked_add(u64::try_from(bytes.len()).map_err(|_| resource())?)
                .ok_or_else(resource)?;
            return Ok(());
        };
        if label.is_empty()
            || label.len() > self.limits.path
            || bytes.len() > self.limits.file_bytes
        {
            return Err(resource());
        }
        self.charge(label.len())?;
        if label.contains('\\')
            || label.chars().any(char::is_control)
            || label
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            || !(label.starts_with("package/")
                || label.starts_with("generated/")
                || label == "entry/library.rs")
        {
            return Err(CaptureError::new(
                ErrorKind::Configuration,
                "invalid portable source label",
            ));
        }
        let lookup = self
            .files
            .len()
            .checked_mul(label.len())
            .ok_or_else(resource)?;
        self.charge(lookup.checked_add(bytes.len()).ok_or_else(resource)?)?;
        if let Some(file) = self.files.iter_mut().find(|file| file.row.path == label) {
            if file.contents != bytes {
                return Err(CaptureError::new(
                    ErrorKind::ConflictingRead,
                    "one source label returned different raw bytes",
                ));
            }
            file.row.read_count = file.row.read_count.checked_add(1).ok_or_else(resource)?;
            if !file.row.read_kinds.contains(&kind) {
                file.row.read_kinds.push(kind);
                file.row.read_kinds.sort_unstable();
            }
            return Ok(());
        }
        if self.files.len() >= self.limits.files {
            return Err(resource());
        }
        let labels = self
            .labels
            .checked_add(label.len())
            .filter(|n| *n <= self.limits.labels)
            .ok_or_else(resource)?;
        let retained = self
            .retained
            .checked_add(bytes.len())
            .filter(|n| *n <= self.limits.total_bytes)
            .ok_or_else(resource)?;
        self.charge(
            bytes
                .len()
                .checked_mul(2)
                .and_then(|n| n.checked_add(label.len()))
                .ok_or_else(resource)?,
        )?;
        let mut contents = Vec::new();
        contents
            .try_reserve_exact(bytes.len())
            .map_err(|_| resource())?;
        self.files.try_reserve(1).map_err(|_| resource())?;
        let hash = sha(bytes);
        contents.extend_from_slice(bytes);
        self.files.push(FileRecord {
            row: FileRow {
                path: label,
                read_kinds: vec![kind],
                read_count: 1,
                bytes: bytes.len() as u64,
                blob: format!("source-{hash}.bin"),
                sha256: hash,
            },
            contents,
        });
        self.labels = labels;
        self.retained = retained;
        Ok(())
    }
}
#[derive(Clone)]
pub(super) struct CaptureHandle {
    state: Arc<Mutex<State>>,
}
impl CaptureHandle {
    pub fn new(unit: UnitIdentity, limits: CaptureLimits) -> Result<Self> {
        if unit.crate_name.is_empty()
            || unit.crate_name.len() > 256
            || unit.analysis_target.is_empty()
            || unit.analysis_target.len() > 256
            || unit.library_source.is_empty()
            || unit.library_source.len() > limits.path
            || !unit.features.is_empty()
        {
            return Err(CaptureError::new(
                ErrorKind::Configuration,
                "invalid selected source unit configuration",
            ));
        }
        Ok(Self {
            state: Arc::new(Mutex::new(State {
                limits,
                unit: Some(unit),
                failed: None,
                sealed: false,
                active: 0,
                calls: 0,
                labels: 0,
                retained: 0,
                work: 0,
                ir_bytes: 0,
                feature_bytes: 0,
                features_recorded: false,
                files: vec![],
                exports: vec![],
                omitted: Omissions::default(),
            })),
        })
    }
    pub fn fail(&self, error: CaptureError) {
        if let Ok(mut state) = self.state.lock()
            && state.failed.is_none()
        {
            state.failed = Some(error);
        }
    }
    fn checked_update<T>(&self, f: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let mut state = self.state.lock().map_err(|_| {
            CaptureError::new(ErrorKind::LockPoisoned, "source collector lock poisoned")
        })?;
        if let Some(error) = &state.failed {
            return Err(error.clone());
        }
        if state.sealed {
            let error = CaptureError::new(ErrorKind::Sealed, "source recorder already sealed");
            state.failed = Some(error.clone());
            return Err(error);
        }
        let result = f(&mut state);
        if let Err(error) = &result {
            state.failed = Some(error.clone());
        }
        result
    }
    fn update(&self, f: impl FnOnce(&mut State) -> Result<()>) {
        let _ = self.checked_update(f);
    }
    pub fn begin_read(&self) -> ReadTicket {
        let mut active = false;
        self.update(|state| {
            if state.calls >= state.limits.reads {
                return Err(resource());
            }
            state.calls += 1;
            state.active = state.active.checked_add(1).ok_or_else(resource)?;
            active = true;
            Ok(())
        });
        ReadTicket {
            capture: self.clone(),
            active,
        }
    }
    pub fn charge_path(&self, length: usize) -> bool {
        let mut allowed = false;
        self.update(|state| {
            if length > state.limits.path {
                return Err(resource());
            }
            state.charge(length)?;
            allowed = true;
            Ok(())
        });
        allowed
    }
    pub fn record_feature(&self, feature: &str) {
        self.update(|state| {
            let compared = state.unit.as_ref().map_or(0, |unit| unit.features.len());
            state.charge(
                feature
                    .len()
                    .checked_mul(compared + 1)
                    .ok_or_else(resource)?,
            )?;
            let unit = state
                .unit
                .as_mut()
                .ok_or_else(|| CaptureError::new(ErrorKind::Sealed, "missing source unit"))?;
            if unit.features.iter().any(|value| value == feature) {
                return Ok(());
            }
            if unit.features.len() >= state.limits.features {
                return Err(resource());
            }
            let total = state
                .feature_bytes
                .checked_add(feature.len())
                .filter(|n| *n <= state.limits.feature_bytes)
                .ok_or_else(resource)?;
            let mut value = String::new();
            value
                .try_reserve_exact(feature.len())
                .map_err(|_| resource())?;
            value.push_str(feature);
            unit.features.try_reserve(1).map_err(|_| resource())?;
            unit.features.push(value);
            state.feature_bytes = total;
            Ok(())
        });
    }
    pub fn finish_features(&self) {
        self.update(|state| {
            state.features_recorded = true;
            Ok(())
        });
    }
    pub fn export_limit(&self) -> Result<usize> {
        self.checked_update(|state| Ok(state.limits.exports))
    }
    pub fn export_budget(&self, entry: &str, ir: &str) -> Result<usize> {
        self.checked_update(|state| state.export_budget(entry, ir))
    }
    pub fn record_export_checked(&self, entry: &str, ir: &str, bytes: &[u8]) -> Result<()> {
        self.checked_update(|state| state.admit_export(entry, ir, bytes))
    }
    #[cfg(test)]
    pub fn record_export(&self, entry: &str, ir: &str, bytes: &[u8]) {
        let _ = self.record_export_checked(entry, ir, bytes);
    }
    pub fn try_seal(&self) -> Result<SealedCapture> {
        let mut state = self.state.lock().map_err(|_| {
            CaptureError::new(ErrorKind::LockPoisoned, "source collector lock poisoned")
        })?;
        if let Some(error) = &state.failed {
            return Err(error.clone());
        }
        if state.sealed {
            return Err(CaptureError::new(
                ErrorKind::Sealed,
                "source recorder already sealed",
            ));
        }
        if state.active != 0 {
            let error = CaptureError::new(ErrorKind::PendingReads, "source reads still in flight");
            state.failed = Some(error.clone());
            return Err(error);
        }
        if !state.features_recorded || state.exports.is_empty() {
            return Err(CaptureError::new(
                ErrorKind::Configuration,
                "missing successful source-unit analysis or exports",
            ));
        }
        let library = &state
            .unit
            .as_ref()
            .ok_or_else(|| CaptureError::new(ErrorKind::Sealed, "missing source unit"))?
            .library_source;
        if !state
            .files
            .iter()
            .any(|file| &file.row.path == library && file.row.read_kinds.contains(&ReadKind::Text))
        {
            return Err(CaptureError::new(
                ErrorKind::Configuration,
                "selected library was not captured as raw text",
            ));
        }
        state.sealed = true;
        let mut unit = state
            .unit
            .take()
            .ok_or_else(|| CaptureError::new(ErrorKind::Sealed, "missing source unit"))?;
        unit.features.sort();
        state
            .files
            .sort_unstable_by(|a, b| a.row.path.cmp(&b.row.path));
        state.exports.sort_unstable_by(|a, b| a.entry.cmp(&b.entry));
        let count = state.files.len();
        let mut files = Vec::new();
        files.try_reserve_exact(count).map_err(|_| resource())?;
        let mut contents = Vec::new();
        contents.try_reserve_exact(count).map_err(|_| resource())?;
        for file in std::mem::take(&mut state.files) {
            files.push(file.row);
            contents.push(file.contents);
        }
        Ok(SealedCapture {
            receipt: UnitReceipt {
                schema_version: 1,
                protocol: "chialisp-oxide-driver-source-unit/v1",
                scope: "selected_unit_file_loader_reads_only",
                unit,
                exports: std::mem::take(&mut state.exports),
                files,
                omitted: std::mem::take(&mut state.omitted),
            },
            contents,
            json_limit: state.limits.json_bytes,
        })
    }
}
pub(super) struct ReadTicket {
    capture: CaptureHandle,
    active: bool,
}
impl ReadTicket {
    pub fn recording(&self) -> bool {
        self.active
            && self
                .capture
                .state
                .lock()
                .is_ok_and(|state| state.failed.is_none() && !state.sealed)
    }
    pub fn succeeded(self, disposition: Result<Disposition>, kind: ReadKind, bytes: &[u8]) {
        if self.active {
            self.capture
                .update(|state| state.admit_read(disposition?, kind, bytes));
        }
    }
}
impl Drop for ReadTicket {
    fn drop(&mut self) {
        if self.active
            && let Ok(mut state) = self.capture.state.lock()
        {
            state.active = state.active.saturating_sub(1);
        }
    }
}
