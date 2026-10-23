//! Transport framing limits, charged before copies. No network implementation.
use super::{Error, ErrorKind, Result};
use std::io::{self, Write};

pub const MAX_BODY: usize = 64 * 1024;
pub const MAX_REQUEST: usize = 4096;

/// A backend must feed decoded HTTP body chunks here, rather than first
/// collecting an unrestricted body. Content-Length is never an allocation hint.
#[derive(Default)]
pub struct BodyCollector {
    bytes: Vec<u8>,
    failed: bool,
}
impl BodyCollector {
    pub fn push(&mut self, chunk: &[u8]) -> Result<()> {
        if self.failed || chunk.len() > MAX_BODY.saturating_sub(self.bytes.len()) {
            self.failed = true;
            return Err(Error::new(ErrorKind::BodyLimit, "HTTP body limit"));
        }
        self.bytes.extend_from_slice(chunk);
        Ok(())
    }
    pub fn finish(self) -> Result<BoundedBody> {
        if self.failed {
            return Err(Error::new(
                ErrorKind::BodyLimit,
                "HTTP body collector failed",
            ));
        }
        Ok(BoundedBody(self.bytes))
    }
}
pub struct BoundedBody(Vec<u8>);
impl BoundedBody {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// The finite collector reserves a full request allowance before calling this
/// writer. Each individual serialization write is checked before allocation.
pub(super) struct RequestWriter(pub Vec<u8>);
impl Write for RequestWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_REQUEST.saturating_sub(self.0.len()) {
            return Err(io::Error::other("provider request limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
