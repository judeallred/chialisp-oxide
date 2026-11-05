use serde::Serialize;
use std::{fmt, io};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    UnknownObject,
    UnsupportedVersion,
    UnsupportedEncoding,
    InvalidManifest,
    MalformedSerialization,
    NonCanonicalSerialization,
    DigestMismatch,
    ResourceLimit,
    InvalidPath,
    CorruptObject,
    Conflict,
    Io,
}

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    message: String,
    source: Option<io::Error>,
}
impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            source: None,
        }
    }
    pub fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self {
            kind: ErrorKind::Io,
            message: context.into(),
            source: Some(source),
        }
    }
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.source {
            Some(source) => write!(f, "{}: {source}", self.message),
            None => f.write_str(&self.message),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|source| source as &dyn std::error::Error)
    }
}
pub type Result<T> = std::result::Result<T, Error>;
