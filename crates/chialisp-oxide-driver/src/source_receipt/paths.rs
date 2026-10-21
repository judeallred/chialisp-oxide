use super::collector::{CaptureError, Disposition, ErrorKind, Result};
use std::path::{Component, Path, PathBuf};

pub(super) trait Classifier: Send + Sync {
    fn classify(&self, path: &Path) -> Result<Disposition>;
    fn working_directory(&self, path: &Path) -> Result<()>;
}
pub(super) struct SourceRoots {
    package: PathBuf,
    generated: Option<PathBuf>,
    library: PathBuf,
    cwd: PathBuf,
}
fn bad() -> CaptureError {
    CaptureError::new(
        ErrorKind::Configuration,
        "source receipt path cannot be classified portably",
    )
}
fn canonical(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().as_encoded_bytes().len() > 4096 {
        return Err(CaptureError::new(
            ErrorKind::ResourceLimit,
            "source receipt path length exceeded",
        ));
    }
    path.canonicalize().map_err(|_| bad())
}
fn label(prefix: &str, relative: &Path) -> Result<String> {
    let mut text = String::new();
    let capacity = prefix
        .len()
        .checked_add(1)
        .and_then(|n| n.checked_add(relative.as_os_str().as_encoded_bytes().len()))
        .filter(|n| *n <= 4096)
        .ok_or_else(bad)?;
    text.try_reserve_exact(capacity).map_err(|_| {
        CaptureError::new(ErrorKind::ResourceLimit, "source label allocation failed")
    })?;
    text.push_str(prefix);
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(bad());
        };
        let part = part.to_str().ok_or_else(bad)?;
        if part.is_empty() || part.contains('\\') || part.chars().any(char::is_control) {
            return Err(bad());
        }
        if text
            .len()
            .checked_add(part.len() + 1)
            .is_none_or(|n| n > 4096)
        {
            return Err(bad());
        }
        text.push('/');
        text.push_str(part);
    }
    if text == prefix {
        return Err(bad());
    }
    Ok(text)
}
impl SourceRoots {
    pub fn new(package: &Path, generated: Option<&Path>, library: &Path) -> Result<Self> {
        let package = canonical(package)?;
        let generated = generated.map(canonical).transpose()?;
        let library = canonical(library)?;
        let cwd = canonical(&std::env::current_dir().map_err(|_| bad())?)?;
        if !package.is_dir()
            || generated.as_ref().is_some_and(|path| !path.is_dir())
            || !library.is_file()
        {
            return Err(bad());
        }
        Ok(Self {
            package,
            generated,
            library,
            cwd,
        })
    }
    pub fn library(&self) -> &Path {
        &self.library
    }
    pub fn package(&self) -> &Path {
        &self.package
    }
    pub fn library_label(&self) -> Result<String> {
        match self.classify(&self.library)? {
            Disposition::Attach(value) => Ok(value),
            Disposition::OutsideRoots => Err(bad()),
        }
    }
}
impl Classifier for SourceRoots {
    fn classify(&self, path: &Path) -> Result<Disposition> {
        let path = if path.is_absolute() {
            canonical(path)?
        } else {
            if path.as_os_str().as_encoded_bytes().len() > 4096 {
                return Err(bad());
            }
            // RealFileLoader resolves relative reads against the process cwd.
            // Checking only FileLoader::current_directory is insufficient: rustc
            // need not call it again after an in-process loader changes cwd.
            // These checks reject persistent drift without altering the already
            // returned compiler bytes. They are not a hostile cwd/FS race guard.
            self.working_directory(&std::env::current_dir().map_err(|_| bad())?)?;
            let resolved = canonical(&self.cwd.join(path))?;
            self.working_directory(&std::env::current_dir().map_err(|_| bad())?)?;
            resolved
        };
        if let Some(generated) = &self.generated
            && let Ok(relative) = path.strip_prefix(generated)
        {
            return Ok(Disposition::Attach(label("generated", relative)?));
        }
        if let Ok(relative) = path.strip_prefix(&self.package) {
            return Ok(Disposition::Attach(label("package", relative)?));
        }
        if path == self.library {
            return Ok(Disposition::Attach("entry/library.rs".into()));
        }
        Ok(Disposition::OutsideRoots)
    }
    fn working_directory(&self, path: &Path) -> Result<()> {
        if canonical(path)? != self.cwd {
            return Err(bad());
        }
        Ok(())
    }
}
