use super::{Budget, StandardArgumentError, StandardArguments, index::Index};
use crate::{Digest32, Result, artifact::VerifiedArtifact};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct KnownModule {
    pub name: &'static str,
    pub module_crate: &'static str,
    pub module_crate_version: &'static str,
    pub source_revision: &'static str,
    pub source_url: &'static str,
    pub license: &'static str,
    pub arguments_schema: &'static str,
    pub arguments_schema_revision: &'static str,
}
pub(super) const STANDARD_MODULE: KnownModule = KnownModule {
    name: "p2_delegated_puzzle_or_hidden_puzzle",
    module_crate: "chia-puzzles",
    module_crate_version: "0.20.3",
    source_revision: "d122456feeef55708d354a065623037bfa010714",
    source_url: "https://github.com/Chia-Network/chia_puzzles/blob/d122456feeef55708d354a065623037bfa010714/puzzles/p2_delegated_puzzle_or_hidden_puzzle.clsp",
    license: "Apache-2.0",
    arguments_schema: "chia-puzzle-types 0.36.1 StandardArgs",
    arguments_schema_revision: "56f6b676a96de8659fe05d7e4f705df553f37fa5",
};
const STANDARD_BLOB_SHA: Digest32 = Digest32::new([
    0x21, 0x74, 0x7e, 0xf0, 0x70, 0x37, 0x60, 0x12, 0xf2, 0x2f, 0x7b, 0x42, 0x6e, 0xda, 0xa3, 0x6a,
    0x24, 0x4a, 0x77, 0xf8, 0x82, 0x5d, 0x13, 0xd5, 0x52, 0x99, 0x63, 0xb9, 0x21, 0xad, 0x1d, 0xa7,
]);
const STANDARD_TREE_HASH: Digest32 = Digest32::new([
    0xe9, 0xaa, 0xa4, 0x9f, 0x45, 0xba, 0xd5, 0xc8, 0x89, 0xb8, 0x6e, 0xe3, 0x34, 0x15, 0x50, 0xc1,
    0x55, 0xcf, 0xdd, 0x10, 0xc3, 0xa6, 0x75, 0x7d, 0xe6, 0x18, 0xd2, 0x06, 0x12, 0xff, 0xfd, 0x52,
]);
pub(super) const CATALOG_MATCH_WORK: usize = 227 + 64 + 143 + 64;
pub(crate) fn standard_module_matches(artifact: &VerifiedArtifact) -> bool {
    let identity = artifact.identity();
    identity.bytes == 227
        && identity.blob_sha256 == STANDARD_BLOB_SHA
        && identity.module_tree_hash == STANDARD_TREE_HASH
        && artifact.bytes() == chia_puzzles::P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE.as_slice()
}

pub(super) fn decode_arguments(
    tree: &Index<'_>,
    arguments: &[usize],
    work: &mut Budget,
) -> Result<StandardArguments> {
    let invalid = |reason| StandardArguments::Invalid { reason };
    if arguments.len() != 1 {
        return Ok(invalid(StandardArgumentError::WrongArity));
    }
    let Some(atom) = tree.atom(arguments[0]) else {
        return Ok(invalid(StandardArgumentError::ExpectedAtom));
    };
    let Ok(key_bytes) = <&[u8; 48]>::try_from(atom) else {
        return Ok(invalid(StandardArgumentError::WrongPublicKeyLength));
    };
    // Fixed-size group decoding has a fixed local bookkeeping charge. This is
    // neither a cryptographic proof nor a VM/consensus cost measurement.
    const PUBLIC_KEY_DECODE_WORK: usize = 4096;
    work.charge(PUBLIC_KEY_DECODE_WORK)?;
    let Ok(key) = chia_bls::PublicKey::from_bytes(key_bytes) else {
        return Ok(invalid(StandardArgumentError::InvalidPublicKey));
    };
    Ok(StandardArguments::Decoded {
        synthetic_public_key_hex: work.hex(atom)?,
        role: "synthetic_public_key",
        key_is_infinity: key.is_inf(),
    })
}

pub(super) const P2_PUZZLE_HASH_MODULE: KnownModule = KnownModule {
    name: "p2_puzzle_hash",
    module_crate: "chia-puzzles",
    module_crate_version: "0.20.3",
    source_revision: "d122456feeef55708d354a065623037bfa010714",
    source_url: "https://github.com/Chia-Network/chia_puzzles/blob/d122456feeef55708d354a065623037bfa010714/puzzles/p2_puzzle_hash.clsp",
    license: "Apache-2.0",
    arguments_schema: "chialisp-oxide-package p2-puzzle-hash/v1 (inner_puzzle_hash: Bytes32)",
    arguments_schema_revision: "d122456feeef55708d354a065623037bfa010714",
};
const P2_PUZZLE_HASH_BLOB_SHA: Digest32 = Digest32::new([
    0x67, 0x8b, 0xc9, 0x4e, 0x1a, 0x0f, 0xa4, 0x39, 0xba, 0x21, 0xeb, 0x11, 0xcc, 0xfb, 0x7d, 0x1c,
    0xea, 0x5a, 0xc9, 0xee, 0x6e, 0x59, 0xdc, 0xf5, 0x11, 0x75, 0xf3, 0xab, 0x64, 0x2a, 0xca, 0x64,
]);
const P2_PUZZLE_HASH_TREE: Digest32 = Digest32::new([
    0x13, 0xe2, 0x9a, 0x62, 0xb4, 0x2c, 0xd2, 0xef, 0x72, 0xa7, 0x9e, 0x4b, 0xac, 0xdc, 0x59, 0x73,
    0x3c, 0xa6, 0x31, 0x0d, 0x65, 0xaf, 0x83, 0xd3, 0x49, 0x36, 0x0d, 0x36, 0xec, 0x62, 0x23, 0x63,
]);
pub(crate) fn p2_puzzle_hash_module_matches(artifact: &VerifiedArtifact) -> bool {
    let identity = artifact.identity();
    identity.bytes == 143
        && identity.blob_sha256 == P2_PUZZLE_HASH_BLOB_SHA
        && identity.module_tree_hash == P2_PUZZLE_HASH_TREE
        && artifact.bytes() == chia_puzzles::P2_PUZZLE_HASH.as_slice()
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Module {
    Standard,
    PuzzleHash,
}
impl Module {
    pub(super) fn description(self) -> KnownModule {
        match self {
            Self::Standard => STANDARD_MODULE,
            Self::PuzzleHash => P2_PUZZLE_HASH_MODULE,
        }
    }
}
pub(super) fn known_module(artifact: &VerifiedArtifact) -> Option<Module> {
    if standard_module_matches(artifact) {
        Some(Module::Standard)
    } else if p2_puzzle_hash_module_matches(artifact) {
        Some(Module::PuzzleHash)
    } else {
        None
    }
}
pub(super) fn decode_puzzle_hash_arguments(
    tree: &Index<'_>,
    arguments: &[usize],
    work: &mut Budget,
) -> Result<super::PuzzleHashArguments> {
    use super::{PuzzleHashArgumentError, PuzzleHashArguments};
    let invalid = |reason| PuzzleHashArguments::Invalid { reason };
    if arguments.len() != 1 {
        return Ok(invalid(PuzzleHashArgumentError::WrongArity));
    }
    let Some(atom) = tree.atom(arguments[0]) else {
        return Ok(invalid(PuzzleHashArgumentError::ExpectedAtom));
    };
    let Ok(bytes) = <[u8; 32]>::try_from(atom) else {
        return Ok(invalid(PuzzleHashArgumentError::WrongHashLength));
    };
    // Fixed copied digest and eventual hexadecimal serialization; the existing
    // report allowance for a 48-byte key also covers this 32-byte digest.
    work.charge(32 + 64 * 3)?;
    Ok(PuzzleHashArguments::Decoded {
        inner_puzzle_hash: Digest32::new(bytes),
        role: "inner_puzzle_tree_hash",
    })
}
