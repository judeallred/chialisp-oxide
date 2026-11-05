//! Bounded construction of reviewed closed host-binding tree shapes.
//! Preflight all input traversals and output dimensions before SDK allocation.
use crate::{
    Error, ErrorKind, Limits, Result,
    artifact::{Encoding, VerifiedArtifact, inspect_artifact},
};
use clvmr_sdk::{
    Allocator, NodePtr,
    serde::{node_from_bytes, node_to_bytes_limit},
};

pub(crate) fn resource(label: &str) -> Error {
    Error::new(
        ErrorKind::ResourceLimit,
        format!("binding {label} limit exceeded"),
    )
}
pub(crate) fn plus(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right)
        .ok_or_else(|| resource("measurement"))
}
pub(crate) fn within(value: u64, limit: usize, label: &str) -> Result<()> {
    if value > u64::try_from(limit).unwrap_or(u64::MAX) {
        return Err(resource(label));
    }
    Ok(())
}
pub(crate) fn sdk_allocator(limits: &Limits) -> Result<Allocator> {
    if limits.max_atom_heap_bytes == 0 || limits.max_atom_heap_bytes > u32::MAX as usize {
        return Err(resource("SDK atom heap configuration"));
    }
    // Preserve the artifact admission profile. The older pinned SDK allocator
    // also subtracts one from this positive,32-bit heap ceiling.
    Ok(Allocator::new_limited(limits.max_atom_heap_bytes))
}
pub(crate) fn decode(allocator: &mut Allocator, artifact: &VerifiedArtifact) -> Result<NodePtr> {
    node_from_bytes(allocator, artifact.bytes()).map_err(codec_error)
}
pub(crate) fn encode_error(error: clvm_traits::ToClvmError) -> Error {
    match error {
        clvm_traits::ToClvmError::OutOfMemory => resource("SDK allocation"),
        error => Error::io(
            "unexpected SDK encoder failure",
            std::io::Error::other(error),
        ),
    }
}
fn codec_error(error: clvmr_sdk::error::EvalErr) -> Error {
    match error {
        clvmr_sdk::error::EvalErr::OutOfMemory
        | clvmr_sdk::error::EvalErr::TooManyAtoms
        | clvmr_sdk::error::EvalErr::TooManyPairs => resource("SDK codec allocation"),
        error => Error::io(
            "SDK codec failed on an admitted tree",
            std::io::Error::other(error),
        ),
    }
}

/// Account for all operands and the closed encoder shape before SDK allocation.
/// Work is conservative local admission/codec work, never CLVM execution cost.
pub(crate) struct Construction<'a> {
    limits: &'a Limits,
    remaining_work: usize,
    output_bytes: usize,
}
impl<'a> Construction<'a> {
    pub(crate) fn new(
        limits: &'a Limits,
        inputs: &[&VerifiedArtifact],
        extra_bytes: u64,
        extra_nodes: u64,
        extra_atom_bytes: u64,
        depth: u64,
    ) -> Result<Self> {
        let mut bytes = extra_bytes;
        let mut nodes = extra_nodes;
        let mut atoms = extra_atom_bytes;
        let mut input_work = 0;
        for input in inputs {
            bytes = plus(bytes, input.identity().bytes)?;
            nodes = plus(nodes, input.stats().nodes)?;
            atoms = plus(atoms, input.stats().atom_bytes)?;
            input_work = plus(input_work, input.stats().work)?;
        }
        within(bytes, limits.max_blob_bytes, "serialized bytes")?;
        within(bytes, limits.max_expanded_bytes, "expanded bytes")?;
        within(nodes, limits.max_nodes, "nodes")?;
        within(depth, limits.max_depth, "depth")?;
        within(plus(atoms, 1)?, limits.max_atom_heap_bytes, "atom heap")?;
        // Closed encoders add only the stated shallow pairs. Charge bounded
        // decoder/encoder traversal and serialization before creating any nodes.
        let codec_work = plus(plus(nodes, nodes)?, plus(bytes, atoms)?)?;
        within(plus(input_work, codec_work)?, limits.max_work, "work")?;
        let remaining_work = limits
            .max_work
            .checked_sub(usize::try_from(codec_work).map_err(|_| resource("work"))?)
            .ok_or_else(|| resource("work"))?;
        Ok(Self {
            limits,
            remaining_work,
            output_bytes: usize::try_from(bytes).map_err(|_| resource("serialized bytes"))?,
        })
    }
    pub(crate) fn admit(&mut self, input: &VerifiedArtifact) -> Result<VerifiedArtifact> {
        let profile = Limits {
            max_work: self.remaining_work,
            ..self.limits.clone()
        };
        let admitted = inspect_artifact(input.bytes(), Encoding::Plain, &profile)?;
        self.remaining_work = self
            .remaining_work
            .checked_sub(usize::try_from(admitted.stats().work).map_err(|_| resource("work"))?)
            .ok_or_else(|| resource("work"))?;
        Ok(admitted)
    }
    pub(crate) fn finish(self, allocator: &Allocator, node: NodePtr) -> Result<VerifiedArtifact> {
        let bytes = node_to_bytes_limit(allocator, node, self.output_bytes).map_err(codec_error)?;
        if bytes.len() != self.output_bytes {
            return Err(Error::new(
                ErrorKind::InvalidManifest,
                "binding encoder shape differs from reviewed bounds",
            ));
        }
        let profile = Limits {
            max_work: self.remaining_work,
            ..self.limits.clone()
        };
        inspect_artifact(&bytes, Encoding::Plain, &profile)
    }
}
