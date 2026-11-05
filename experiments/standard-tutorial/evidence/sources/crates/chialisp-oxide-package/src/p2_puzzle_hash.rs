//! Explicit reviewed host binding to canonical `p2_puzzle_hash`.
//!
//! The actual module bytes come from an admitted package. Only the outer
//! commitment/curry/solution shape is typed; inner CLVM remains opaque. This
//! binding does not infer an inner ABI, authorize a spender or execute a VM.
use crate::{
    Digest32, Error, ErrorKind, Limits, Result, VerifiedPackage,
    artifact::{Encoding, VerifiedArtifact, inspect_artifact},
    construction::{Construction, decode, encode_error, plus, sdk_allocator, within},
};
use clvm_traits::{Atom, ClvmEncoder, ToClvm, ToClvmError};
use clvm_utils::CurriedProgram;

/// A caller-declared commitment to a decoded inner program tree.
/// This is neither an envelope/blob digest nor proof of authorization or safety.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InnerPuzzleHash(Digest32);
impl InnerPuzzleHash {
    pub fn from_tree_hash(tree_hash: Digest32) -> Self {
        Self(tree_hash)
    }
    pub fn from_artifact(program: &VerifiedArtifact) -> Self {
        Self(program.identity().module_tree_hash)
    }
    pub fn tree_hash(self) -> Digest32 {
        self.0
    }
}

// The host codec must emit one atom. The generic SDK array codec instead
// emits a list of 32 integers, which has a different ABI and output size.
impl<E: ClvmEncoder> ToClvm<E> for InnerPuzzleHash {
    fn to_clvm(&self, encoder: &mut E) -> std::result::Result<E::Node, ToClvmError> {
        encoder.encode_atom(Atom::Borrowed(self.0.as_bytes()))
    }
}

/// The manually reviewed schema is explicit, never selected by package names.
#[derive(ToClvm)]
#[clvm(curry)]
struct Arguments {
    inner_puzzle_hash: InnerPuzzleHash,
}
#[derive(ToClvm)]
#[clvm(list)]
struct Solution<Program, Environment> {
    inner_puzzle: Program,
    inner_puzzle_solution: Environment,
}

#[derive(Debug)]
pub struct P2PuzzleHashModule {
    object_id: Digest32,
    artifact: VerifiedArtifact,
}
impl P2PuzzleHashModule {
    pub fn from_package(package: &VerifiedPackage, limits: &Limits) -> Result<Self> {
        // Re-admit under this caller's profile; package admission may have used
        // wider limits. Fixed exact-module comparison never grants an inner ABI.
        let artifact = inspect_artifact(package.artifact().bytes(), Encoding::Plain, limits)?;
        if !crate::inspect::p2_puzzle_hash_module_matches(&artifact) {
            return Err(Error::new(
                ErrorKind::DigestMismatch,
                "package does not contain the pinned canonical p2_puzzle_hash module",
            ));
        }
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

    /// Curry the actual retrieved module, committing one explicit inner hash.
    pub fn curry(
        &self,
        commitment: InnerPuzzleHash,
        limits: &Limits,
    ) -> Result<P2PuzzleHashInstance> {
        within(32, limits.max_atom_bytes, "inner puzzle hash atom bytes")?;
        let depth = plus(self.artifact.stats().depth, 3)?.max(7);
        // Canonical one-argument curry overhead for a 32-byte atom: 48
        // serialized bytes, 16 logical nodes and 37 atom payload bytes.
        let mut budget = Construction::new(limits, &[&self.artifact], 48, 16, 37, depth)?;
        let module = budget.admit(&self.artifact)?;
        let mut allocator = sdk_allocator(limits)?;
        let retrieved = decode(&mut allocator, &module)?;
        let instance = CurriedProgram {
            program: retrieved,
            args: Arguments {
                inner_puzzle_hash: commitment,
            },
        }
        .to_clvm(&mut allocator)
        .map_err(encode_error)?;
        let artifact = budget.finish(&allocator, instance)?;
        Ok(P2PuzzleHashInstance {
            module_object_id: self.object_id,
            commitment,
            artifact,
        })
    }
}

#[derive(Debug)]
pub struct P2PuzzleHashInstance {
    module_object_id: Digest32,
    commitment: InnerPuzzleHash,
    artifact: VerifiedArtifact,
}
impl P2PuzzleHashInstance {
    pub fn module_object_id(&self) -> Digest32 {
        self.module_object_id
    }
    pub fn commitment(&self) -> InnerPuzzleHash {
        self.commitment
    }
    pub fn artifact(&self) -> &VerifiedArtifact {
        &self.artifact
    }

    /// Encode `(inner_program inner_solution)` using the actual admitted trees.
    /// A wrong commitment is a host construction error, not consensus rejection.
    /// A matching program can still fail on application or return unsafe output.
    pub fn solution(
        &self,
        inner_program: &VerifiedArtifact,
        inner_solution: &VerifiedArtifact,
        limits: &Limits,
    ) -> Result<VerifiedArtifact> {
        let depth = plus(inner_program.stats().depth, 1)?
            .max(plus(inner_solution.stats().depth, 2)?)
            .max(3);
        let mut budget =
            Construction::new(limits, &[inner_program, inner_solution], 3, 3, 0, depth)?;
        let program = budget.admit(inner_program)?;
        let solution = budget.admit(inner_solution)?;
        if program.identity().module_tree_hash != self.commitment.tree_hash() {
            return Err(Error::new(
                ErrorKind::DigestMismatch,
                "inner program differs from committed tree hash",
            ));
        }
        let mut allocator = sdk_allocator(limits)?;
        let inner_puzzle = decode(&mut allocator, &program)?;
        let inner_puzzle_solution = decode(&mut allocator, &solution)?;
        let node = Solution {
            inner_puzzle,
            inner_puzzle_solution,
        }
        .to_clvm(&mut allocator)
        .map_err(encode_error)?;
        budget.finish(&allocator, node)
    }
}
