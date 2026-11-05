use super::{
    GenerationMetrics, index,
    limits::{GenerationLimits, Work, add, reserve, resource, within},
};
use crate::{Digest32, Error, ErrorKind, Result, artifact::ArtifactIdentity, digest};
use sha2::{Digest, Sha256};
use std::{
    fmt::{self, Write},
    ops::Range,
};

#[derive(Clone, Copy, Debug)]
pub(super) enum Node {
    Atom(usize),
    Pair { first: usize, rest: usize },
}
pub(super) struct Chunk {
    pub start: usize,
    pub end: usize,
    pub outgoing: Vec<usize>,
}
pub(super) struct Recipe<'a> {
    pub bytes: &'a [u8],
    pub atoms: Vec<Range<usize>>,
    pub nodes: Vec<Node>,
    pub pairs: Vec<usize>,
    pub chunks: Vec<Chunk>,
    pub root: usize,
    pub max_frontier: usize,
    pub total_atom_payload: usize,
    pub max_atom_payload: usize,
}
fn mismatch() -> Error {
    Error::new(
        ErrorKind::DigestMismatch,
        "generated tree recipe differs from the selected admitted artifact",
    )
}
impl<'a> Recipe<'a> {
    pub fn new(
        index: index::Index<'a>,
        limits: &GenerationLimits,
        work: &mut Work,
    ) -> Result<Self> {
        let mut nodes = Vec::new();
        let mut atoms: Vec<Range<usize>> = Vec::new();
        let mut pairs = Vec::new();
        work.charge(index.nodes.len())?;
        reserve(
            &mut nodes,
            index.nodes.len(),
            "cannot allocate recipe nodes",
        )?;
        let mut total_atom_payload = 0usize;
        let mut max_atom_payload = 0usize;
        for node in index.nodes {
            work.charge(1)?;
            match node {
                index::Node::Atom(span) => {
                    let payload = &index.bytes[span.clone()];
                    let mut found = None;
                    for (id, previous) in atoms.iter().enumerate() {
                        // Length tests and every potentially compared payload byte
                        // are charged before equality, including repeated atoms.
                        work.charge(1)?;
                        if span.len() == previous.len() {
                            work.charge(span.len())?;
                            if payload == &index.bytes[previous.clone()] {
                                found = Some(id);
                                break;
                            }
                        }
                    }
                    let id = if let Some(id) = found {
                        id
                    } else {
                        within(
                            add(atoms.len(), 1, "unique atom count overflow")?,
                            limits.max_unique_atoms,
                            "unique atom count limit exceeded",
                        )?;
                        let total = add(
                            total_atom_payload,
                            span.len(),
                            "unique atom payload overflow",
                        )?;
                        within(
                            total,
                            limits.max_total_unique_atom_payload_bytes,
                            "unique atom payload byte limit exceeded",
                        )?;
                        work.charge(1)?;
                        reserve(&mut atoms, 1, "cannot allocate atom recipe")?;
                        let id = atoms.len();
                        total_atom_payload = total;
                        max_atom_payload = max_atom_payload.max(span.len());
                        atoms.push(span);
                        id
                    };
                    nodes.push(Node::Atom(id));
                }
                index::Node::Pair { first, rest } => {
                    work.charge(1)?;
                    reserve(&mut pairs, 1, "cannot allocate pair recipe")?;
                    pairs.push(nodes.len());
                    nodes.push(Node::Pair { first, rest });
                }
            }
        }
        let helper_count = if pairs.is_empty() {
            1
        } else {
            add(
                pairs.len(),
                limits.pairs_per_helper - 1,
                "helper count overflow",
            )? / limits.pairs_per_helper
        };
        within(
            helper_count,
            limits.max_construction_helpers,
            "construction helper limit exceeded",
        )?;
        work.charge(add(
            helper_count,
            pairs.len(),
            "frontier allocation overflow",
        )?)?;
        let mut chunks = Vec::new();
        reserve(&mut chunks, helper_count, "cannot allocate helper recipe")?;
        let mut live = Vec::new();
        reserve(&mut live, pairs.len(), "cannot allocate live pair frontier")?;
        let mut start = 0usize;
        let mut max_frontier = 0usize;
        for (offset, &id) in pairs.iter().enumerate() {
            work.charge(1)?;
            let Node::Pair { first, rest } = nodes[id] else {
                return Err(mismatch());
            };
            for child in [first, rest] {
                work.charge(1)?;
                if matches!(nodes[child], Node::Pair { .. }) {
                    work.charge(add(live.len(), live.len(), "frontier scan work overflow")?)?;
                    let position = live.iter().position(|&n| n == child).ok_or_else(mismatch)?;
                    // Vec removal shifts at most the already charged frontier.
                    live.remove(position);
                }
            }
            work.charge(1)?;
            live.push(id);
            max_frontier = max_frontier.max(live.len());
            // Includes peaks inside a helper, not only forwarded parameters.
            within(
                live.len(),
                limits.max_live_pair_references,
                "live pair reference limit exceeded",
            )?;
            let end = offset + 1;
            if end.is_multiple_of(limits.pairs_per_helper) || end == pairs.len() {
                work.charge(live.len())?;
                let mut outgoing = Vec::new();
                reserve(&mut outgoing, live.len(), "cannot allocate helper frontier")?;
                outgoing.extend_from_slice(&live);
                chunks.push(Chunk {
                    start,
                    end,
                    outgoing,
                });
                start = end;
            }
        }
        if pairs.is_empty() {
            chunks.push(Chunk {
                start: 0,
                end: 0,
                outgoing: Vec::new(),
            });
        }
        Ok(Self {
            bytes: index.bytes,
            atoms,
            nodes,
            pairs,
            chunks,
            root: index.root,
            max_frontier,
            total_atom_payload,
            max_atom_payload,
        })
    }

    /// Rebuild canonical serialization from the finite emitted recipe, not byte
    /// spans or copied serialized subtrees. Hash every recipe node independently.
    pub fn verify(
        &self,
        identity: &ArtifactIdentity,
        limits: &GenerationLimits,
        work: &mut Work,
    ) -> Result<()> {
        work.charge(add(
            self.nodes.len(),
            self.bytes.len(),
            "reconstruction allocation overflow",
        )?)?;
        let mut encoded = Vec::new();
        reserve(
            &mut encoded,
            self.bytes.len(),
            "cannot allocate recipe reconstruction",
        )?;
        let mut stack = Vec::new();
        work.charge(self.nodes.len())?;
        reserve(
            &mut stack,
            self.nodes.len(),
            "cannot allocate reconstruction stack",
        )?;
        stack.push(self.root);
        while let Some(node) = stack.pop() {
            work.charge(1)?;
            match self.nodes[node] {
                Node::Atom(atom) => {
                    let payload = &self.bytes[self.atoms[atom].clone()];
                    let mut header = [0u8; 2];
                    let header_length = if payload.len() == 1 && payload[0] < 0x80 {
                        0
                    } else if payload.len() < 0x40 {
                        header[0] = 0x80 | payload.len() as u8;
                        1
                    } else {
                        header[0] = 0xc0 | (payload.len() >> 8) as u8;
                        header[1] = payload.len() as u8;
                        2
                    };
                    let additional =
                        add(header_length, payload.len(), "atom reconstruction overflow")?;
                    within(
                        add(encoded.len(), additional, "reconstruction overflow")?,
                        limits.max_artifact_bytes,
                        "reconstructed byte limit exceeded",
                    )?;
                    work.charge(additional)?;
                    encoded.extend_from_slice(&header[..header_length]);
                    encoded.extend_from_slice(payload);
                }
                Node::Pair { first, rest } => {
                    within(
                        add(encoded.len(), 1, "reconstruction overflow")?,
                        limits.max_artifact_bytes,
                        "reconstructed byte limit exceeded",
                    )?;
                    work.charge(3)?;
                    encoded.push(0xff);
                    stack.push(rest);
                    stack.push(first);
                }
            }
        }
        let mut hashes: Vec<Digest32> = Vec::new();
        work.charge(self.nodes.len())?;
        reserve(
            &mut hashes,
            self.nodes.len(),
            "cannot allocate recipe hashes",
        )?;
        for node in &self.nodes {
            let mut hash = Sha256::new();
            match *node {
                Node::Atom(atom) => {
                    let payload = &self.bytes[self.atoms[atom].clone()];
                    work.charge(add(1, payload.len(), "atom hash work overflow")?)?;
                    hash.update([1]);
                    hash.update(payload);
                }
                Node::Pair { first, rest } => {
                    work.charge(65)?;
                    hash.update([2]);
                    hash.update(hashes[first].as_bytes());
                    hash.update(hashes[rest].as_bytes());
                }
            }
            hashes.push(Digest32::new(hash.finalize().into()));
        }
        work.charge(add(
            encoded.len(),
            self.bytes.len(),
            "reconstruction comparison overflow",
        )?)?;
        if encoded != self.bytes
            || identity.bytes != encoded.len() as u64
            || digest(&encoded) != identity.blob_sha256
            || hashes[self.root] != identity.module_tree_hash
        {
            return Err(mismatch());
        }
        Ok(())
    }

    pub fn emit(
        &self,
        identity: &ArtifactIdentity,
        lock: Digest32,
        object: Digest32,
        limits: &GenerationLimits,
        work: &mut Work,
    ) -> Result<String> {
        let mut out = Source::new(limits.max_source_bytes, work);
        out.emit(format_args!("// Generated ordinary Rust; WIP prototype, not ready for use.\n// Recipe: chialisp-oxide-generated-tree/v1\n// Raw lock SHA-256: "))?;
        out.digest(lock)?;
        out.emit(format_args!("\n// Envelope SHA-256: "))?;
        out.digest(object)?;
        out.emit(format_args!("\n// Artifact blob SHA-256: "))?;
        out.digest(identity.blob_sha256)?;
        out.emit(format_args!("\n// Artifact tree hash: "))?;
        out.digest(identity.module_tree_hash)?;
        out.emit(format_args!("\n// Artifact bytes: {}\n// Opaque whole environment; no inferred ABI, authorization or preservation.\nuse chialisp_oxide::{{Tree, apply}};\n\n", identity.bytes))?;
        for (id, span) in self.atoms.iter().enumerate() {
            out.emit(format_args!(
                "#[rustfmt::skip]\nconst ATOM_{id}: Tree<'static> = Tree::Atom(&["
            ))?;
            for (offset, byte) in self.bytes[span.clone()].iter().enumerate() {
                if offset != 0 {
                    out.emit(format_args!(", "))?;
                }
                out.emit(format_args!("0x{byte:02x}"))?;
            }
            out.emit(format_args!("]);\n"))?;
        }
        for (stage, chunk) in self.chunks.iter().enumerate() {
            if stage == 0 {
                out.emit(format_args!("\n#[rustfmt::skip]\npub(super) fn run<'a>(environment: &'a Tree<'a>) -> Tree<'static> {{\n"))?;
            } else {
                out.emit(format_args!(
                    "\n#[rustfmt::skip]\nfn stage_{stage}<'a>(environment: &'a Tree<'a>"
                ))?;
                for id in &self.chunks[stage - 1].outgoing {
                    out.emit(format_args!(", node_{id}: &'a Tree<'a>"))?;
                }
                out.emit(format_args!(") -> Tree<'static> {{\n"))?;
            }
            let incoming = if stage == 0 {
                &[][..]
            } else {
                &self.chunks[stage - 1].outgoing
            };
            for &id in &self.pairs[chunk.start..chunk.end] {
                let Node::Pair { first, rest } = self.nodes[id] else {
                    return Err(mismatch());
                };
                out.emit(format_args!("    let node_{id} = Tree::Pair("))?;
                self.reference(first, incoming, &mut out)?;
                out.emit(format_args!(", "))?;
                self.reference(rest, incoming, &mut out)?;
                out.emit(format_args!(");\n"))?;
            }
            if stage + 1 == self.chunks.len() {
                out.emit(format_args!("    apply("))?;
                self.reference(self.root, incoming, &mut out)?;
                out.emit(format_args!(", environment)\n}}\n"))?;
            } else {
                out.emit(format_args!("    stage_{}(environment", stage + 1))?;
                for &id in &chunk.outgoing {
                    out.emit(format_args!(", "))?;
                    self.reference(id, incoming, &mut out)?;
                }
                out.emit(format_args!(")\n}}\n"))?;
            }
        }
        Ok(out.text)
    }
    fn reference(&self, id: usize, incoming: &[usize], out: &mut Source<'_>) -> Result<()> {
        out.work
            .charge(add(1, incoming.len(), "reference scan work overflow")?)?;
        match self.nodes[id] {
            Node::Atom(atom) => out.emit(format_args!("&ATOM_{atom}")),
            Node::Pair { .. } if incoming.contains(&id) => out.emit(format_args!("node_{id}")),
            Node::Pair { .. } => out.emit(format_args!("&node_{id}")),
        }
    }
    pub fn metrics(&self, artifact: &crate::artifact::VerifiedArtifact) -> GenerationMetrics {
        GenerationMetrics {
            artifact_bytes: artifact.identity().bytes,
            expanded_nodes: artifact.stats().nodes,
            pairs: self.pairs.len() as u64,
            atoms: artifact.stats().atoms,
            data_depth_root_one: artifact.stats().depth,
            unique_atoms: self.atoms.len() as u64,
            max_atom_payload_bytes: self.max_atom_payload as u64,
            total_unique_atom_payload_bytes: self.total_atom_payload as u64,
            max_live_pair_references: self.max_frontier as u64,
            construction_helpers: self.chunks.len() as u64,
            generation_work_units: 0,
        }
    }
}

/// Every append is checked and charged before reserving or copying. Formatting
/// arguments contain only fixed syntax, numeric identifiers/bytes and digests.
struct Source<'a> {
    text: String,
    maximum: usize,
    work: &'a mut Work,
    failure: Option<Error>,
}
impl<'a> Source<'a> {
    fn new(maximum: usize, work: &'a mut Work) -> Self {
        Self {
            text: String::new(),
            maximum,
            work,
            failure: None,
        }
    }
    fn emit(&mut self, args: fmt::Arguments<'_>) -> Result<()> {
        self.write_fmt(args).map_err(|_| {
            self.failure
                .take()
                .unwrap_or_else(|| resource("source formatting failed"))
        })
    }
    fn digest(&mut self, value: Digest32) -> Result<()> {
        // Digest32's human Display allocates a temporary hex String. Emit fixed
        // numeric bytes instead, so every source allocation is precharged here.
        for byte in value.as_bytes() {
            self.emit(format_args!("{byte:02x}"))?;
        }
        Ok(())
    }
}
impl Write for Source<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let append = (|| {
            within(
                add(self.text.len(), text.len(), "source size overflow")?,
                self.maximum,
                "source byte limit exceeded",
            )?;
            self.work.charge(text.len())?;
            self.text
                .try_reserve(text.len())
                .map_err(|_| resource("cannot allocate source bytes"))?;
            self.text.push_str(text);
            Ok(())
        })();
        if let Err(error) = append {
            self.failure = Some(error);
            return Err(fmt::Error);
        }
        Ok(())
    }
}
