//! Offline canonical-plain reveal identities and strict structural uncurry.
//!
//! Recognition and argument decoding do not execute CLVM, verify a spend or
//! authenticate any chain, owner, source, hidden-puzzle choice or signature.
mod catalog;
mod index;

pub use catalog::KnownModule;
pub(crate) use catalog::{p2_puzzle_hash_module_matches, standard_module_matches};

use crate::{
    Digest32, Error, ErrorKind, Limits, Result,
    artifact::{ArtifactIdentity, Encoding, VerifiedArtifact, inspect_artifact},
    fs,
};
use serde::Serialize;
use std::path::Path;

pub const MAX_CURRY_ARGUMENTS: usize = 128;
pub const INSPECTION_SCOPE: &str = "Offline bounded canonical-plain artifact identities, exact canonical curry shape and reviewed standard/inner-puzzle-hash argument decoding only; no CLVM execution, source/IR equivalence, authenticated chain inclusion, owner identity, hidden-puzzle choice, signature or spend validation, or suitability for real funds.";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RevealForm {
    Unclassified,
    CanonicalCurry,
    RecognizedRawModule,
}

#[derive(Clone, Debug, Serialize)]
pub struct Subtree {
    pub identity: ArtifactIdentity,
    pub clvm_hex: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StandardArgumentError {
    WrongArity,
    ExpectedAtom,
    WrongPublicKeyLength,
    InvalidPublicKey,
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum StandardArguments {
    NotApplicable,
    NotCurried,
    Invalid {
        reason: StandardArgumentError,
    },
    Decoded {
        synthetic_public_key_hex: String,
        role: &'static str,
        key_is_infinity: bool,
    },
}
/// Errors in the manually reviewed canonical p2_puzzle_hash curry schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PuzzleHashArgumentError {
    WrongArity,
    ExpectedAtom,
    WrongHashLength,
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PuzzleHashArguments {
    NotCurried,
    Invalid {
        reason: PuzzleHashArgumentError,
    },
    Decoded {
        inner_puzzle_hash: Digest32,
        role: &'static str,
    },
}
#[derive(Clone, Debug, Serialize)]
pub struct InspectionReport {
    pub schema_version: u32,
    pub form: RevealForm,
    pub reveal: ArtifactIdentity,
    pub module: Option<Subtree>,
    pub arguments: Vec<Subtree>,
    pub module_recognized: bool,
    pub known_module: Option<KnownModule>,
    pub standard_arguments: StandardArguments,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p2_puzzle_hash_arguments: Option<PuzzleHashArguments>,
    pub scope: &'static str,
}
pub struct Inspection {
    report: InspectionReport,
    reveal: VerifiedArtifact,
    module: Option<VerifiedArtifact>,
    arguments: Vec<VerifiedArtifact>,
    work: usize,
}
impl Inspection {
    pub fn report(&self) -> &InspectionReport {
        &self.report
    }
    pub fn reveal(&self) -> &VerifiedArtifact {
        &self.reveal
    }
    pub fn module(&self) -> Option<&VerifiedArtifact> {
        self.module.as_ref().or_else(|| {
            (self.report.form == RevealForm::RecognizedRawModule).then_some(&self.reveal)
        })
    }
    pub fn arguments(&self) -> &[VerifiedArtifact] {
        &self.arguments
    }
    /// Local conservative inspection bookkeeping, not VM cost or wall time.
    pub fn work(&self) -> usize {
        self.work
    }
}
impl Serialize for Inspection {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        self.report.serialize(serializer)
    }
}
impl std::fmt::Debug for Inspection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inspection")
            .field("form", &self.report.form)
            .field("reveal", self.reveal.identity())
            .field("arguments", &self.arguments.len())
            .field("module_recognized", &self.report.module_recognized)
            .finish()
    }
}

fn resource(message: &str) -> Error {
    Error::new(ErrorKind::ResourceLimit, message)
}
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidManifest, message)
}

pub(super) struct Budget {
    work: usize,
    limit: usize,
    subtree_bytes: usize,
    subtree_limit: usize,
    hex_bytes: usize,
    hex_limit: usize,
}
impl Budget {
    fn new(reveal: &VerifiedArtifact, limits: &Limits) -> Result<Self> {
        let work =
            usize::try_from(reveal.stats().work).map_err(|_| resource("inspection work range"))?;
        // Module/argument spans are disjoint pieces of the reveal. The decoded
        // standard key adds at most one fixed 48-byte atom payload to JSON hex.
        let hex_limit = limits
            .max_blob_bytes
            .checked_mul(2)
            .and_then(|n| n.checked_add(96))
            .ok_or_else(|| resource("inspection output hex configuration"))?;
        Ok(Self {
            work,
            limit: limits.max_work,
            subtree_bytes: 0,
            subtree_limit: limits.max_blob_bytes.min(limits.max_expanded_bytes),
            hex_bytes: 0,
            hex_limit,
        })
    }
    pub(super) fn charge(&mut self, amount: usize) -> Result<()> {
        self.work = self
            .work
            .checked_add(amount)
            .filter(|n| *n <= self.limit)
            .ok_or_else(|| resource("inspection aggregate work limit exceeded"))?;
        Ok(())
    }
    fn admit(&mut self, bytes: &[u8], limits: &Limits) -> Result<VerifiedArtifact> {
        let remaining = self
            .subtree_limit
            .checked_sub(self.subtree_bytes)
            .ok_or_else(|| resource("inspection subtree byte limit exceeded"))?;
        if bytes.len() > remaining {
            return Err(resource("inspection subtree byte limit exceeded"));
        }
        self.subtree_bytes += bytes.len();
        let mut profile = limits.clone();
        profile.max_work = self
            .limit
            .checked_sub(self.work)
            .ok_or_else(|| resource("inspection aggregate work limit exceeded"))?;
        profile.max_blob_bytes = profile.max_blob_bytes.min(remaining);
        profile.max_expanded_bytes = profile.max_expanded_bytes.min(remaining);
        let artifact = inspect_artifact(bytes, Encoding::Plain, &profile)?;
        self.charge(
            usize::try_from(artifact.stats().work)
                .map_err(|_| resource("inspection subtree work range"))?,
        )?;
        Ok(artifact)
    }
    fn hex(&mut self, bytes: &[u8]) -> Result<String> {
        let length = bytes
            .len()
            .checked_mul(2)
            .ok_or_else(|| resource("inspection output hex size"))?;
        self.hex_bytes = self
            .hex_bytes
            .checked_add(length)
            .filter(|n| *n <= self.hex_limit)
            .ok_or_else(|| resource("inspection aggregate output hex limit exceeded"))?;
        // Initialization, hexadecimal writes and UTF-8 validation each touch
        // at most the complete output before String takes ownership of it.
        self.charge(
            length
                .checked_mul(3)
                .ok_or_else(|| resource("inspection hex work size"))?,
        )?;
        let mut encoded = Vec::new();
        encoded
            .try_reserve_exact(length)
            .map_err(|_| resource("allocate inspection output hex"))?;
        encoded.resize(length, 0);
        hex::encode_to_slice(bytes, &mut encoded)
            .map_err(|_| invalid("inspection hex output length mismatch"))?;
        String::from_utf8(encoded).map_err(|_| invalid("inspection hex output must be ASCII"))
    }
    fn subtree(&mut self, artifact: &VerifiedArtifact) -> Result<Subtree> {
        Ok(Subtree {
            identity: artifact.identity().clone(),
            clvm_hex: self.hex(artifact.bytes())?,
        })
    }
}

/// Inspect one entire canonical plain reveal, without executable evaluation.
pub fn inspect_reveal(
    bytes: &[u8],
    limits: &Limits,
    expected_full_tree_hash: Option<Digest32>,
) -> Result<Inspection> {
    let reveal = inspect_artifact(bytes, Encoding::Plain, limits)?;
    let mut budget = Budget::new(&reveal, limits)?;
    if let Some(expected) = expected_full_tree_hash {
        budget.charge(32)?;
        if reveal.identity().module_tree_hash != expected {
            return Err(Error::new(
                ErrorKind::DigestMismatch,
                "reveal differs from expected full puzzle tree hash",
            ));
        }
    }
    budget.charge(catalog::CATALOG_MATCH_WORK)?;
    let raw_module = catalog::known_module(&reveal);
    let raw_known = raw_module.is_some();
    let tree = index::Index::new(reveal.bytes(), reveal.stats(), limits, &mut budget)?;
    // A published compiled module can itself have curry-shaped implementation
    // code. Exact catalog identity takes precedence over interpreting that
    // implementation's constants as caller-supplied instance arguments.
    let shape = if raw_known {
        None
    } else {
        tree.uncurry(&mut budget)?
    };
    let mut module = None;
    let mut arguments = Vec::new();
    let mut argument_reports = Vec::new();
    let mut module_report = None;
    let mut known_module = raw_module;
    let mut form = if raw_known {
        RevealForm::RecognizedRawModule
    } else {
        RevealForm::Unclassified
    };
    let mut standard_arguments = if raw_module == Some(catalog::Module::Standard) {
        StandardArguments::NotCurried
    } else {
        StandardArguments::NotApplicable
    };
    let mut p2_puzzle_hash_arguments = (raw_module == Some(catalog::Module::PuzzleHash))
        .then_some(PuzzleHashArguments::NotCurried);
    if let Some(shape) = shape {
        form = RevealForm::CanonicalCurry;
        let admitted = budget.admit(tree.bytes(shape.module), limits)?;
        budget.charge(catalog::CATALOG_MATCH_WORK)?;
        known_module = catalog::known_module(&admitted);
        let standard = known_module == Some(catalog::Module::Standard);
        let puzzle_hash = known_module == Some(catalog::Module::PuzzleHash);
        module_report = Some(budget.subtree(&admitted)?);
        module = Some(admitted);
        // The shape collector bounded these collections before reserving.
        budget.charge(shape.arguments.len())?;
        arguments
            .try_reserve_exact(shape.arguments.len())
            .map_err(|_| resource("allocate inspection arguments"))?;
        argument_reports
            .try_reserve_exact(shape.arguments.len())
            .map_err(|_| resource("allocate inspection argument reports"))?;
        for &argument in &shape.arguments {
            let admitted = budget.admit(tree.bytes(argument), limits)?;
            argument_reports.push(budget.subtree(&admitted)?);
            arguments.push(admitted);
        }
        if standard {
            standard_arguments = catalog::decode_arguments(&tree, &shape.arguments, &mut budget)?;
        }
        if puzzle_hash {
            p2_puzzle_hash_arguments = Some(catalog::decode_puzzle_hash_arguments(
                &tree,
                &shape.arguments,
                &mut budget,
            )?);
        }
    } else if raw_known {
        module_report = Some(budget.subtree(&reveal)?);
    }
    drop(tree);
    let report = InspectionReport {
        schema_version: 1,
        form,
        reveal: reveal.identity().clone(),
        module: module_report,
        arguments: argument_reports,
        module_recognized: known_module.is_some(),
        known_module: known_module.map(catalog::Module::description),
        standard_arguments,
        p2_puzzle_hash_arguments,
        scope: INSPECTION_SCOPE,
    };
    Ok(Inspection {
        report,
        reveal,
        module,
        arguments,
        work: budget.work,
    })
}

/// Decode an explicitly selected bounded regular hex file. This is transport
/// parsing only: callers must separately admit the complete CLVM serialization.
pub fn read_hex_file(path: &Path, limits: &Limits) -> Result<Vec<u8>> {
    let maximum = limits
        .max_blob_bytes
        .checked_mul(2)
        .and_then(|n| n.checked_add(4))
        .ok_or_else(|| resource("CLVM hex file size configuration"))?;
    // Bound read/trim/validation/decode work before copying the file. This is
    // a separate transport budget, not additional CLVM/VM execution credit.
    const HEX_TRANSPORT_WORK_PER_BYTE: usize = 6;
    let text = fs::read_regular_file(
        path,
        maximum.min(limits.max_work / HEX_TRANSPORT_WORK_PER_BYTE),
    )?;
    let mut raw = text.as_slice();
    while raw.first().is_some_and(u8::is_ascii_whitespace) {
        raw = &raw[1..];
    }
    while raw.last().is_some_and(u8::is_ascii_whitespace) {
        raw = &raw[..raw.len() - 1];
    }
    if raw.starts_with(b"0x") || raw.starts_with(b"0X") {
        raw = &raw[2..];
    }
    if raw.is_empty() || raw.len() % 2 != 0 || !raw.iter().all(u8::is_ascii_hexdigit) {
        return Err(invalid("CLVM hex file requires complete hexadecimal bytes"));
    }
    if raw.len() / 2 > limits.max_blob_bytes {
        return Err(resource("CLVM hex decoded blob limit exceeded"));
    }
    let mut decoded = Vec::new();
    decoded
        .try_reserve_exact(raw.len() / 2)
        .map_err(|_| resource("allocate decoded CLVM hex"))?;
    decoded.resize(raw.len() / 2, 0);
    hex::decode_to_slice(raw, &mut decoded)
        .map_err(|_| invalid("invalid CLVM hexadecimal bytes"))?;
    Ok(decoded)
}

#[cfg(test)]
mod tests;
