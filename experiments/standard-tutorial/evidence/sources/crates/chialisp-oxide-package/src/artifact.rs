//! Bounded identity checks for complete canonical plain CLVM serializations.
//!
//! These checks do not execute a program, infer its ABI, or establish source
//! equivalence, authorization, compiler preservation, or fitness for funds.
use crate::{Digest32, Error, ErrorKind, Limits, Result, digest};
use clvmr::{
    Allocator, NodePtr,
    allocator::SExp,
    error::EvalErr,
    serde::{SERDE_2026_MAGIC_PREFIX, node_from_bytes},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Explicit supported serialization. No alternate decoder is tried on failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum Encoding {
    #[serde(rename = "clvm-legacy-plain/v1")]
    Plain,
}
impl<'de> Deserialize<'de> for Encoding {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        crate::string_enum::deserialize(
            deserializer,
            &["clvm-legacy-plain/v1"],
            |value| match value {
                "clvm-legacy-plain/v1" => Some(Self::Plain),
                _ => None,
            },
        )
    }
}

/// Exact byte identity and decoded module-tree identity are separate commitments.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactIdentity {
    pub encoding: Encoding,
    pub bytes: u64,
    pub blob_sha256: Digest32,
    pub module_tree_hash: Digest32,
}

/// Measurements of logical plain-tree occurrences, never VM cost or process RAM.
/// Root depth is one. Work charges scans, parser work, stack operations, hashing
/// and the retained byte copy; it is a deterministic conservative local budget.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactStats {
    pub nodes: u64,
    pub pairs: u64,
    pub atoms: u64,
    pub atom_bytes: u64,
    pub depth: u64,
    pub expanded_bytes: u64,
    /// The pinned allocator's atom accounting, including its initial ghost byte.
    /// This excludes pair vectors, parser stacks, and other host allocations.
    pub allocator_atom_heap_bytes: u64,
    pub work: u64,
}

/// Only successful complete bounded inspection constructs this value.
#[derive(Clone, Debug)]
pub struct VerifiedArtifact {
    identity: ArtifactIdentity,
    bytes: Vec<u8>,
    stats: ArtifactStats,
}
impl VerifiedArtifact {
    pub fn identity(&self) -> &ArtifactIdentity {
        &self.identity
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn stats(&self) -> &ArtifactStats {
        &self.stats
    }
}

fn resource(label: &str) -> Error {
    Error::new(
        ErrorKind::ResourceLimit,
        format!("artifact {label} limit exceeded"),
    )
}
fn malformed(message: &str) -> Error {
    Error::new(ErrorKind::MalformedSerialization, message)
}
fn add(first: usize, second: usize, label: &str) -> Result<usize> {
    first.checked_add(second).ok_or_else(|| resource(label))
}
fn within(value: usize, limit: usize, label: &str) -> Result<()> {
    if value > limit {
        return Err(resource(label));
    }
    Ok(())
}
fn number(value: usize) -> Result<u64> {
    u64::try_from(value).map_err(|_| resource("measurement range"))
}
fn reserve<T>(stack: &mut Vec<T>, additional: usize, limit: usize, label: &str) -> Result<()> {
    within(add(stack.len(), additional, label)?, limit, label)?;
    stack.try_reserve(additional).map_err(|_| resource(label))
}

struct Work {
    used: usize,
    limit: usize,
}
impl Work {
    fn charge(&mut self, units: usize) -> Result<()> {
        let next = add(self.used, units, "work")?;
        within(next, self.limit, "work")?;
        self.used = next;
        Ok(())
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
struct Shape {
    nodes: usize,
    pairs: usize,
    atoms: usize,
    atom_bytes: usize,
    depth: usize,
    expanded_bytes: usize,
    atom_heap_bytes: usize,
}

/// Reads only a bounded atom header/slice. The actual pinned decoder is still
/// required for admission. No marker inside an atom payload is interpreted.
fn atom<'a>(bytes: &'a [u8], cursor: &mut usize, work: &mut Work) -> Result<&'a [u8]> {
    let start = *cursor;
    let first = *bytes
        .get(start)
        .ok_or_else(|| malformed("truncated CLVM atom"))?;
    *cursor = add(start, 1, "input position")?;
    if first < 0x80 {
        return Ok(&bytes[start..*cursor]);
    }
    if first == 0x80 {
        return Ok(&[]);
    }
    if first == 0xfe || bytes[start..].starts_with(&SERDE_2026_MAGIC_PREFIX) {
        return Err(Error::new(
            ErrorKind::UnsupportedEncoding,
            "backreferences and serde2026 are unsupported; expected clvm-legacy-plain/v1",
        ));
    }
    let prefix = first.leading_ones() as usize;
    if !(1..=6).contains(&prefix) {
        return Err(malformed("invalid CLVM atom length prefix"));
    }
    let header_end = add(start, prefix, "atom header")?;
    if header_end > bytes.len() {
        return Err(malformed("truncated CLVM atom length"));
    }
    work.charge(prefix - 1)?;
    let mut length = u64::from(first & (0xff >> prefix));
    for byte in &bytes[*cursor..header_end] {
        length = (length << 8) | u64::from(*byte);
    }
    if length >= 0x400000000 {
        return Err(malformed("CLVM atom length exceeds the legacy format"));
    }
    let minimum = if prefix == 1 {
        0
    } else {
        1_u64 << (7 * prefix - 8)
    };
    if length < minimum {
        return Err(Error::new(
            ErrorKind::NonCanonicalSerialization,
            "CLVM atom uses a nonminimal length prefix",
        ));
    }
    let length = usize::try_from(length).map_err(|_| resource("atom length range"))?;
    let end = add(header_end, length, "atom length")?;
    if end > bytes.len() {
        return Err(malformed("truncated CLVM atom payload"));
    }
    let value = &bytes[header_end..end];
    if length == 1 && value[0] < 0x80 {
        return Err(Error::new(
            ErrorKind::NonCanonicalSerialization,
            "small CLVM atom must use its single-byte encoding",
        ));
    }
    *cursor = end;
    Ok(value)
}

/// Bounds every future parser stack/pair/atom allocation before invoking clvmr.
fn preflight(bytes: &[u8], limits: &Limits, work: &mut Work) -> Result<Shape> {
    within(bytes.len(), limits.max_blob_bytes, "blob bytes")?;
    within(bytes.len(), limits.max_expanded_bytes, "expanded bytes")?;
    if bytes.is_empty() {
        return Err(malformed("empty CLVM serialization"));
    }
    within(1, limits.max_nodes, "nodes")?;
    within(1, limits.max_depth, "depth")?;
    within(1, limits.max_atom_heap_bytes, "atom heap bytes")?;
    // This is a caller-selected bound, not a cast or an assertion inside clvmr.
    if limits.max_atom_heap_bytes > u32::MAX as usize {
        return Err(resource("allocator heap configuration"));
    }
    let mut pending = Vec::new();
    work.charge(1)?;
    reserve(&mut pending, 1, limits.max_nodes, "parser stack")?;
    pending.push(1_usize);
    let mut shape = Shape {
        atom_heap_bytes: 1,
        ..Shape::default()
    };
    let mut cursor = 0;
    while let Some(depth) = pending.pop() {
        work.charge(1)?;
        shape.nodes = add(shape.nodes, 1, "nodes")?;
        within(shape.nodes, limits.max_nodes, "nodes")?;
        shape.depth = shape.depth.max(depth);
        let first = *bytes
            .get(cursor)
            .ok_or_else(|| malformed("truncated CLVM tree"))?;
        if first == 0xff {
            cursor = add(cursor, 1, "input position")?;
            shape.pairs = add(shape.pairs, 1, "pairs")?;
            let child_depth = add(depth, 1, "depth")?;
            within(child_depth, limits.max_depth, "depth")?;
            // Each pending slot requires at least one more node. Reject before
            // pushing slots that cannot fit, even for a truncated all-ff input.
            within(
                add(add(shape.nodes, pending.len(), "nodes")?, 2, "nodes")?,
                limits.max_nodes,
                "nodes",
            )?;
            work.charge(2)?;
            reserve(&mut pending, 2, limits.max_nodes, "parser stack")?;
            pending.push(child_depth);
            pending.push(child_depth);
        } else {
            let value = atom(bytes, &mut cursor, work)?;
            within(value.len(), limits.max_atom_bytes, "atom bytes")?;
            shape.atoms = add(shape.atoms, 1, "atoms")?;
            shape.atom_bytes = add(shape.atom_bytes, value.len(), "atom bytes")?;
            // node_from_stream returns the allocator's existing nil/one nodes.
            // Every other atom (including small-number ghosts) charges its bytes.
            if !value.is_empty() && value != [1] {
                shape.atom_heap_bytes = add(shape.atom_heap_bytes, value.len(), "atom heap bytes")?;
                within(
                    shape.atom_heap_bytes,
                    limits.max_atom_heap_bytes,
                    "atom heap bytes",
                )?;
            }
        }
    }
    if cursor != bytes.len() {
        return Err(malformed("trailing CLVM serialized bytes"));
    }
    shape.expanded_bytes = bytes.len();
    Ok(shape)
}

fn decoder_error(error: EvalErr) -> Error {
    let kind = match error {
        EvalErr::NonCanonicalSerialization => ErrorKind::NonCanonicalSerialization,
        EvalErr::OutOfMemory | EvalErr::TooManyAtoms | EvalErr::TooManyPairs => {
            ErrorKind::ResourceLimit
        }
        _ => ErrorKind::MalformedSerialization,
    };
    Error::new(kind, format!("pinned CLVM artifact decoder: {error}"))
}

fn atom_serialized_length(value: &[u8]) -> Result<usize> {
    if value.is_empty() || (value.len() == 1 && value[0] < 0x80) {
        return Ok(1);
    }
    let length = number(value.len())?;
    let prefix = if length < 0x40 {
        1
    } else if length < 0x2000 {
        2
    } else if length < 0x100000 {
        3
    } else if length < 0x8000000 {
        4
    } else {
        5
    };
    add(value.len(), prefix, "expanded bytes")
}

enum Fold {
    Node(NodePtr, usize),
    Pair,
}

/// Plain parsing creates no shared pair subtrees. Count logical occurrences,
/// including allocator-reused nil/small atoms, rather than deduplicating them.
fn tree_identity(
    a: &Allocator,
    root: NodePtr,
    expected: &Shape,
    limits: &Limits,
    work: &mut Work,
) -> Result<Digest32> {
    let max_ops = add(expected.nodes, expected.pairs, "hash stack")?;
    let mut pending = Vec::new();
    let mut hashes: Vec<[u8; 32]> = Vec::new();
    work.charge(1)?;
    reserve(&mut pending, 1, max_ops, "hash stack")?;
    pending.push(Fold::Node(root, 1));
    let mut measured = Shape {
        atom_heap_bytes: 1,
        ..Shape::default()
    };
    while let Some(next) = pending.pop() {
        work.charge(1)?;
        match next {
            Fold::Node(node, depth) => {
                measured.nodes = add(measured.nodes, 1, "nodes")?;
                within(measured.nodes, limits.max_nodes, "nodes")?;
                within(depth, limits.max_depth, "depth")?;
                measured.depth = measured.depth.max(depth);
                match a.sexp(node) {
                    SExp::Atom => {
                        let value = a.atom(node);
                        let value = value.as_ref();
                        within(value.len(), limits.max_atom_bytes, "atom bytes")?;
                        measured.atoms = add(measured.atoms, 1, "atoms")?;
                        measured.atom_bytes = add(measured.atom_bytes, value.len(), "atom bytes")?;
                        if !value.is_empty() && value != [1] {
                            measured.atom_heap_bytes =
                                add(measured.atom_heap_bytes, value.len(), "atom heap bytes")?;
                            within(
                                measured.atom_heap_bytes,
                                limits.max_atom_heap_bytes,
                                "atom heap bytes",
                            )?;
                        }
                        measured.expanded_bytes = add(
                            measured.expanded_bytes,
                            atom_serialized_length(value)?,
                            "expanded bytes",
                        )?;
                        within(
                            measured.expanded_bytes,
                            limits.max_expanded_bytes,
                            "expanded bytes",
                        )?;
                        work.charge(add(value.len(), 2, "work")?)?;
                        reserve(&mut hashes, 1, expected.atoms, "hash values")?;
                        let mut hash = Sha256::new();
                        hash.update([1]);
                        hash.update(value);
                        hashes.push(hash.finalize().into());
                    }
                    SExp::Pair(first, rest) => {
                        measured.pairs = add(measured.pairs, 1, "pairs")?;
                        measured.expanded_bytes =
                            add(measured.expanded_bytes, 1, "expanded bytes")?;
                        within(
                            measured.expanded_bytes,
                            limits.max_expanded_bytes,
                            "expanded bytes",
                        )?;
                        let child_depth = add(depth, 1, "depth")?;
                        within(child_depth, limits.max_depth, "depth")?;
                        work.charge(3)?;
                        reserve(&mut pending, 3, max_ops, "hash stack")?;
                        pending.push(Fold::Pair);
                        pending.push(Fold::Node(rest, child_depth));
                        pending.push(Fold::Node(first, child_depth));
                    }
                }
            }
            Fold::Pair => {
                work.charge(66)?;
                let rest = hashes
                    .pop()
                    .ok_or_else(|| malformed("missing decoded right subtree"))?;
                let first = hashes
                    .pop()
                    .ok_or_else(|| malformed("missing decoded left subtree"))?;
                reserve(&mut hashes, 1, expected.atoms, "hash values")?;
                let mut hash = Sha256::new();
                hash.update([2]);
                hash.update(first);
                hash.update(rest);
                hashes.push(hash.finalize().into());
            }
        }
    }
    if measured != *expected || hashes.len() != 1 {
        return Err(malformed(
            "pinned decoder tree differs from resource preflight",
        ));
    }
    Ok(Digest32::new(hashes[0]))
}

/// Inspect one complete, canonical plain artifact without executing it.
pub fn inspect_artifact(
    bytes: &[u8],
    encoding: Encoding,
    limits: &Limits,
) -> Result<VerifiedArtifact> {
    let mut work = Work {
        used: 0,
        limit: limits.max_work,
    };
    let shape = preflight(bytes, limits, &mut work)?;
    // Account for the pinned parser's input/atom copying and pair continuations
    // before it creates its allocator or private stacks.
    work.charge(add(
        add(shape.nodes, shape.pairs, "work")?,
        shape.atom_bytes,
        "work",
    )?)?;
    let mut allocator = Allocator::new_limited(limits.max_atom_heap_bytes);
    let root = match encoding {
        Encoding::Plain => node_from_bytes(&mut allocator, bytes).map_err(decoder_error)?,
    };
    let module_tree_hash = tree_identity(&allocator, root, &shape, limits, &mut work)?;
    // Reserve both complete-blob hashing and retention before either operation.
    work.charge(bytes.len())?;
    work.charge(bytes.len())?;
    let mut retained = Vec::new();
    retained
        .try_reserve_exact(bytes.len())
        .map_err(|_| resource("retained bytes"))?;
    let blob_sha256 = digest(bytes);
    retained.extend_from_slice(bytes);
    Ok(VerifiedArtifact {
        identity: ArtifactIdentity {
            encoding,
            bytes: number(bytes.len())?,
            blob_sha256,
            module_tree_hash,
        },
        bytes: retained,
        stats: ArtifactStats {
            nodes: number(shape.nodes)?,
            pairs: number(shape.pairs)?,
            atoms: number(shape.atoms)?,
            atom_bytes: number(shape.atom_bytes)?,
            depth: number(shape.depth)?,
            expanded_bytes: number(shape.expanded_bytes)?,
            allocator_atom_heap_bytes: number(shape.atom_heap_bytes)?,
            work: number(work.used)?,
        },
    })
}

/// Check the exact blob commitment as well as the separately declared tree hash.
pub fn verify_artifact(
    bytes: &[u8],
    expected: &ArtifactIdentity,
    limits: &Limits,
) -> Result<VerifiedArtifact> {
    within(bytes.len(), limits.max_blob_bytes, "blob bytes")?;
    if number(bytes.len())? != expected.bytes {
        return Err(Error::new(
            ErrorKind::DigestMismatch,
            "artifact declared byte count differs",
        ));
    }
    let verified = inspect_artifact(bytes, expected.encoding, limits)?;
    if verified.identity().blob_sha256 != expected.blob_sha256 {
        return Err(Error::new(
            ErrorKind::DigestMismatch,
            "artifact blob SHA-256 differs",
        ));
    }
    if verified.identity().module_tree_hash != expected.module_tree_hash {
        return Err(Error::new(
            ErrorKind::DigestMismatch,
            "artifact module tree hash differs",
        ));
    }
    Ok(verified)
}

#[cfg(test)]
mod tests;
