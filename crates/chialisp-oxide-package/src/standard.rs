//! Reviewed host binding to the pinned canonical standard-wallet module.
//!
//! The module bytes come from a verified package. Only its outer curry and
//! delegated-solution schemas are typed here; inner programs remain opaque.
//! These APIs neither derive keys nor authenticate ownership or a coin spend.
#[cfg(test)]
mod tests;

use crate::{
    Digest32, Error, ErrorKind, Limits, Result, VerifiedPackage,
    artifact::{Encoding, VerifiedArtifact, inspect_artifact},
};
use chia_bls::PublicKey;
use chia_puzzle_types::standard::{StandardArgs, StandardSolution};
use clvm_traits::ToClvm;
use clvm_utils::CurriedProgram;
use clvmr_sdk::{
    Allocator, NodePtr,
    serde::{node_from_bytes, node_to_bytes_limit},
};

/// A caller-declared synthetic-key role, with actual compressed-point parsing.
/// This does not prove synthetic derivation, a hidden-puzzle choice or ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyntheticPublicKey(PublicKey);
impl SyntheticPublicKey {
    /// New instances deliberately exclude the identity point. The inspector
    /// still preserves historical identity-point arguments as observed data.
    pub fn from_bytes(bytes: [u8; 48]) -> Result<Self> {
        let key = PublicKey::from_bytes(&bytes).map_err(|error| {
            Error::new(
                ErrorKind::InvalidManifest,
                format!("invalid synthetic public key: {error}"),
            )
        })?;
        if key.is_inf() {
            return Err(Error::new(
                ErrorKind::InvalidManifest,
                "new standard instances require a non-identity synthetic public key",
            ));
        }
        Ok(Self(key))
    }
    pub fn to_bytes(self) -> [u8; 48] {
        self.0.to_bytes()
    }
    pub fn public_key(self) -> PublicKey {
        self.0
    }
}

#[derive(Debug)]
pub struct StandardModule {
    object_id: Digest32,
    artifact: VerifiedArtifact,
}
impl StandardModule {
    /// Select the explicit reviewed binding only for the exact canonical module.
    /// Envelope source/ABI declarations are not used to infer this schema.
    pub fn from_package(package: &VerifiedPackage, limits: &Limits) -> Result<Self> {
        if !crate::inspect::standard_module_matches(package.artifact()) {
            return Err(Error::new(
                ErrorKind::DigestMismatch,
                "package does not contain the pinned canonical standard module",
            ));
        }
        let artifact = inspect_artifact(package.artifact().bytes(), Encoding::Plain, limits)?;
        Ok(Self {
            object_id: package.object_id(),
            artifact,
        })
    }
    pub fn object_id(&self) -> Digest32 {
        self.object_id
    }
    pub fn artifact(&self) -> &VerifiedArtifact {
        &self.artifact
    }

    /// Curry the actual retrieved module; no SDK embedded module is substituted.
    pub fn curry(&self, key: &SyntheticPublicKey, limits: &Limits) -> Result<StandardInstance> {
        within(48, limits.max_atom_bytes, "atom bytes")?;
        let depth = plus(self.artifact.stats().depth, 3)?.max(7);
        let mut budget = Construction::new(limits, &[&self.artifact], 64, 16, 53, depth)?;
        let module = budget.admit(&self.artifact)?;
        let mut allocator = sdk_allocator(limits)?;
        let retrieved = decode(&mut allocator, &module)?;
        let node = CurriedProgram {
            program: retrieved,
            args: StandardArgs::new(key.0),
        }
        .to_clvm(&mut allocator)
        .map_err(encode_error)?;
        let artifact = budget.finish(&allocator, node)?;
        Ok(StandardInstance {
            module_object_id: self.object_id,
            key: *key,
            artifact,
        })
    }
}

#[derive(Debug)]
pub struct StandardInstance {
    module_object_id: Digest32,
    key: SyntheticPublicKey,
    artifact: VerifiedArtifact,
}
impl StandardInstance {
    pub fn module_object_id(&self) -> Digest32 {
        self.module_object_id
    }
    pub fn synthetic_key(&self) -> SyntheticPublicKey {
        self.key
    }
    pub fn artifact(&self) -> &VerifiedArtifact {
        &self.artifact
    }

    /// Build the delegated branch: nil original key, actual inner program and
    /// its supplied solution tree. No inner ABI or signature is inferred.
    pub fn delegated_solution(
        &self,
        inner_program: &VerifiedArtifact,
        inner_solution: &VerifiedArtifact,
        limits: &Limits,
    ) -> Result<VerifiedArtifact> {
        let depth = plus(inner_program.stats().depth, 2)?
            .max(plus(inner_solution.stats().depth, 3)?)
            .max(4);
        let mut budget =
            Construction::new(limits, &[inner_program, inner_solution], 5, 5, 0, depth)?;
        let program = budget.admit(inner_program)?;
        let solution = budget.admit(inner_solution)?;
        let mut allocator = sdk_allocator(limits)?;
        let delegated_puzzle = decode(&mut allocator, &program)?;
        let solution = decode(&mut allocator, &solution)?;
        let node = StandardSolution {
            original_public_key: None,
            delegated_puzzle,
            solution,
        }
        .to_clvm(&mut allocator)
        .map_err(encode_error)?;
        budget.finish(&allocator, node)
    }
}

fn resource(label: &str) -> Error {
    Error::new(
        ErrorKind::ResourceLimit,
        format!("standard binding {label} limit exceeded"),
    )
}
fn plus(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right)
        .ok_or_else(|| resource("measurement"))
}
fn within(value: u64, limit: usize, label: &str) -> Result<()> {
    if value > u64::try_from(limit).unwrap_or(u64::MAX) {
        return Err(resource(label));
    }
    Ok(())
}
fn sdk_allocator(limits: &Limits) -> Result<Allocator> {
    if limits.max_atom_heap_bytes == 0 || limits.max_atom_heap_bytes > u32::MAX as usize {
        return Err(resource("SDK atom heap configuration"));
    }
    // Preserve the artifact admission profile. The older pinned SDK allocator
    // also subtracts one from this positive,32-bit heap ceiling.
    Ok(Allocator::new_limited(limits.max_atom_heap_bytes))
}
fn decode(allocator: &mut Allocator, artifact: &VerifiedArtifact) -> Result<NodePtr> {
    node_from_bytes(allocator, artifact.bytes()).map_err(codec_error)
}
fn encode_error(error: clvm_traits::ToClvmError) -> Error {
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
struct Construction<'a> {
    limits: &'a Limits,
    remaining_work: usize,
    output_bytes: usize,
}
impl<'a> Construction<'a> {
    fn new(
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
    fn admit(&mut self, input: &VerifiedArtifact) -> Result<VerifiedArtifact> {
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
    fn finish(self, allocator: &Allocator, node: NodePtr) -> Result<VerifiedArtifact> {
        let bytes = node_to_bytes_limit(allocator, node, self.output_bytes).map_err(codec_error)?;
        if bytes.len() != self.output_bytes {
            return Err(Error::new(
                ErrorKind::InvalidManifest,
                "standard encoder shape differs from reviewed bounds",
            ));
        }
        let profile = Limits {
            max_work: self.remaining_work,
            ..self.limits.clone()
        };
        inspect_artifact(&bytes, Encoding::Plain, &profile)
    }
}
