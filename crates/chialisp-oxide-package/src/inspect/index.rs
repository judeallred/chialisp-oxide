//! Borrowed exact byte-span index, constructed only after plain admission.
use super::{Budget, MAX_CURRY_ARGUMENTS, resource};
use crate::{Error, ErrorKind, Limits, Result, artifact::ArtifactStats};
use std::ops::Range;

enum Kind {
    Atom(Range<usize>),
    Pair { first: usize, rest: usize },
}
struct Node {
    span: Range<usize>,
    kind: Kind,
}
struct Frame {
    node: usize,
    first: Option<usize>,
}
pub(super) struct Index<'a> {
    bytes: &'a [u8],
    nodes: Vec<Node>,
    root: usize,
}
pub(super) struct Curry {
    pub module: usize,
    pub arguments: Vec<usize>,
}
fn malformed() -> Error {
    Error::new(
        ErrorKind::MalformedSerialization,
        "admitted plain tree span mismatch",
    )
}

impl<'a> Index<'a> {
    pub(super) fn new(
        bytes: &'a [u8],
        stats: &ArtifactStats,
        limits: &Limits,
        work: &mut Budget,
    ) -> Result<Self> {
        let count =
            usize::try_from(stats.nodes).map_err(|_| resource("inspection index node range"))?;
        let depth =
            usize::try_from(stats.depth).map_err(|_| resource("inspection index depth range"))?;
        if count == 0 || count > limits.max_nodes || depth > limits.max_depth {
            return Err(resource("inspection index shape limit exceeded"));
        }
        work.charge(
            count
                .checked_add(depth)
                .ok_or_else(|| resource("inspection index allocation work"))?,
        )?;
        let mut nodes = Vec::new();
        nodes
            .try_reserve_exact(count)
            .map_err(|_| resource("allocate inspection span index"))?;
        let mut frames: Vec<Frame> = Vec::new();
        frames
            .try_reserve_exact(depth)
            .map_err(|_| resource("allocate inspection index frames"))?;
        let mut cursor = 0usize;
        let mut root = None;
        loop {
            work.charge(1)?;
            if nodes.len() >= count || frames.len() >= depth {
                return Err(malformed());
            }
            let start = cursor;
            let first = *bytes.get(cursor).ok_or_else(malformed)?;
            let id = nodes.len();
            if first == 0xff {
                cursor += 1;
                nodes.push(Node {
                    span: start..0,
                    kind: Kind::Pair {
                        first: usize::MAX,
                        rest: usize::MAX,
                    },
                });
                work.charge(1)?;
                frames.push(Frame {
                    node: id,
                    first: None,
                });
                continue;
            }
            let payload = atom(bytes, &mut cursor, work)?;
            nodes.push(Node {
                span: start..cursor,
                kind: Kind::Atom(payload),
            });
            let mut completed = id;
            loop {
                work.charge(1)?;
                let Some(frame) = frames.last_mut() else {
                    root = Some(completed);
                    break;
                };
                if frame.first.is_none() {
                    frame.first = Some(completed);
                    break;
                }
                let frame = frames.pop().ok_or_else(malformed)?;
                let start = nodes[frame.node].span.start;
                nodes[frame.node] = Node {
                    span: start..cursor,
                    kind: Kind::Pair {
                        first: frame.first.ok_or_else(malformed)?,
                        rest: completed,
                    },
                };
                completed = frame.node;
            }
            if root.is_some() {
                break;
            }
        }
        if cursor != bytes.len() || nodes.len() != count {
            return Err(malformed());
        }
        Ok(Self {
            bytes,
            nodes,
            root: root.ok_or_else(malformed)?,
        })
    }
    pub(super) fn bytes(&self, node: usize) -> &[u8] {
        &self.bytes[self.nodes[node].span.clone()]
    }
    pub(super) fn atom(&self, node: usize) -> Option<&[u8]> {
        match &self.nodes[node].kind {
            Kind::Atom(span) => Some(&self.bytes[span.clone()]),
            Kind::Pair { .. } => None,
        }
    }
    fn pair(&self, node: usize) -> Option<(usize, usize)> {
        match &self.nodes[node].kind {
            Kind::Pair { first, rest } => Some((*first, *rest)),
            Kind::Atom(_) => None,
        }
    }
    fn list3(&self, node: usize) -> Option<[usize; 3]> {
        let (a, tail) = self.pair(node)?;
        let (b, tail) = self.pair(tail)?;
        let (c, tail) = self.pair(tail)?;
        (self.atom(tail) == Some(&[][..])).then_some([a, b, c])
    }
    fn quoted(&self, node: usize) -> Option<usize> {
        let (op, datum) = self.pair(node)?;
        (self.atom(op) == Some(&[1][..])).then_some(datum)
    }
    pub(super) fn uncurry(&self, work: &mut Budget) -> Result<Option<Curry>> {
        work.charge(16)?;
        let Some([apply, quoted, args]) = self.list3(self.root) else {
            return Ok(None);
        };
        if self.atom(apply) != Some(&[2][..]) {
            return Ok(None);
        }
        let Some(module) = self.quoted(quoted) else {
            return Ok(None);
        };
        let mut rest = args;
        let mut arguments = Vec::new();
        loop {
            work.charge(16)?;
            if self.atom(rest) == Some(&[1][..]) {
                return Ok(Some(Curry { module, arguments }));
            }
            let Some([cons, quoted, next]) = self.list3(rest) else {
                return Ok(None);
            };
            if self.atom(cons) != Some(&[4][..]) {
                return Ok(None);
            }
            let Some(argument) = self.quoted(quoted) else {
                return Ok(None);
            };
            if arguments.len() >= MAX_CURRY_ARGUMENTS {
                return Err(resource("canonical curry argument count limit exceeded"));
            }
            work.charge(1)?;
            arguments
                .try_reserve(1)
                .map_err(|_| resource("allocate canonical curry argument index"))?;
            arguments.push(argument);
            rest = next;
        }
    }
}

fn atom(bytes: &[u8], cursor: &mut usize, work: &mut Budget) -> Result<Range<usize>> {
    let start = *cursor;
    let first = *bytes.get(start).ok_or_else(malformed)?;
    if first < 0x80 {
        *cursor = start + 1;
        return Ok(start..*cursor);
    }
    if first == 0x80 {
        *cursor = start + 1;
        return Ok(*cursor..*cursor);
    }
    let prefix = first.leading_ones() as usize;
    if !(1..=6).contains(&prefix) {
        return Err(malformed());
    }
    work.charge(prefix)?;
    let header = start.checked_add(prefix).ok_or_else(malformed)?;
    let suffix = bytes.get(start + 1..header).ok_or_else(malformed)?;
    let mut length = u64::from(first & (0xff >> prefix));
    for &byte in suffix {
        length = (length << 8) | u64::from(byte);
    }
    let length = usize::try_from(length).map_err(|_| malformed())?;
    let end = header
        .checked_add(length)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(malformed)?;
    *cursor = end;
    Ok(header..end)
}
