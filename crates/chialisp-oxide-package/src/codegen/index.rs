//! Bounded postorder index over an already admitted canonical plain artifact.
//! Atom spans borrow exact payload bytes; markers inside payloads stay opaque.
use super::limits::{GenerationLimits, Work, add, reserve, resource, within};
use crate::{Error, ErrorKind, Result, artifact::VerifiedArtifact};
use std::ops::Range;

#[derive(Clone, Debug)]
pub(super) enum Node {
    Atom(Range<usize>),
    Pair { first: usize, rest: usize },
}
struct Frame {
    first: Option<usize>,
}
pub(super) struct Index<'a> {
    pub bytes: &'a [u8],
    pub nodes: Vec<Node>,
    pub root: usize,
}
fn malformed() -> Error {
    Error::new(
        ErrorKind::MalformedSerialization,
        "admitted generation tree span mismatch",
    )
}
fn measurement(value: u64) -> Result<usize> {
    usize::try_from(value).map_err(|_| resource("measurement does not fit host range"))
}
impl<'a> Index<'a> {
    pub fn new(
        artifact: &'a VerifiedArtifact,
        limits: &GenerationLimits,
        work: &mut Work,
    ) -> Result<Self> {
        let bytes = artifact.bytes();
        let stats = artifact.stats();
        within(
            bytes.len(),
            limits.max_artifact_bytes,
            "artifact byte limit exceeded",
        )?;
        let count = measurement(stats.nodes)?;
        let pairs = measurement(stats.pairs)?;
        let depth = measurement(stats.depth)?;
        within(
            count,
            limits.max_expanded_nodes,
            "expanded node limit exceeded",
        )?;
        within(pairs, limits.max_pairs, "pair limit exceeded")?;
        within(
            depth,
            limits.max_data_depth_root_one,
            "data depth limit exceeded",
        )?;
        if count == 0 || depth == 0 {
            return Err(malformed());
        }
        // Charge the complete bounded retained index and parse stack before either
        // allocation, rather than trusting lengths from an imported declaration.
        work.charge(add(count, depth, "index allocation arithmetic")?)?;
        let mut nodes = Vec::new();
        reserve(&mut nodes, count, "cannot allocate postorder index")?;
        let mut frames: Vec<Frame> = Vec::new();
        reserve(&mut frames, depth, "cannot allocate parse frontier")?;
        let mut cursor = 0usize;
        let mut seen_pairs = 0usize;
        loop {
            work.charge(1)?;
            if nodes.len() >= count || frames.len() >= depth {
                return Err(malformed());
            }
            let byte = *bytes.get(cursor).ok_or_else(malformed)?;
            if byte == 0xff {
                cursor = add(cursor, 1, "parse position overflow")?;
                seen_pairs = add(seen_pairs, 1, "pair count overflow")?;
                if seen_pairs > pairs {
                    return Err(malformed());
                }
                work.charge(1)?;
                frames.push(Frame { first: None });
                continue;
            }
            let payload = atom(bytes, &mut cursor, work)?;
            within(
                payload.len(),
                limits.max_atom_payload_bytes,
                "atom payload exceeds the generation limit (fixed v1 maximum 256 bytes; atoms are never split)",
            )?;
            let mut completed = nodes.len();
            nodes.push(Node::Atom(payload));
            loop {
                work.charge(1)?;
                let Some(frame) = frames.last_mut() else {
                    if cursor != bytes.len() || nodes.len() != count || seen_pairs != pairs {
                        return Err(malformed());
                    }
                    return Ok(Self {
                        bytes,
                        nodes,
                        root: completed,
                    });
                };
                if frame.first.is_none() {
                    frame.first = Some(completed);
                    break;
                }
                let frame = frames.pop().ok_or_else(malformed)?;
                if nodes.len() >= count {
                    return Err(malformed());
                }
                let next = nodes.len();
                nodes.push(Node::Pair {
                    first: frame.first.ok_or_else(malformed)?,
                    rest: completed,
                });
                completed = next;
            }
        }
    }
}
fn atom(bytes: &[u8], cursor: &mut usize, work: &mut Work) -> Result<Range<usize>> {
    let start = *cursor;
    let first = *bytes.get(start).ok_or_else(malformed)?;
    if first < 0x80 {
        *cursor = add(start, 1, "atom position overflow")?;
        return Ok(start..*cursor);
    }
    if first == 0x80 {
        *cursor = add(start, 1, "atom position overflow")?;
        return Ok(*cursor..*cursor);
    }
    let prefix = first.leading_ones() as usize;
    if !(1..=6).contains(&prefix) {
        return Err(malformed());
    }
    work.charge(prefix)?;
    let header = add(start, prefix, "atom header overflow")?;
    let suffix = bytes.get(start + 1..header).ok_or_else(malformed)?;
    let mut length = u64::from(first & (0xff >> prefix));
    for &byte in suffix {
        length = (length << 8) | u64::from(byte);
    }
    let length = usize::try_from(length).map_err(|_| malformed())?;
    let end = add(header, length, "atom length overflow")?;
    if end > bytes.len() {
        return Err(malformed());
    }
    work.charge(length)?;
    *cursor = end;
    Ok(header..end)
}
