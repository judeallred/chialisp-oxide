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
    construction::{Construction, decode, encode_error, plus, sdk_allocator, within},
};
use chia_bls::PublicKey;
use chia_puzzle_types::standard::{StandardArgs, StandardSolution};
use clvm_traits::ToClvm;
use clvm_utils::CurriedProgram;

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
