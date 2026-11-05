use super::WitnessGenerationLimits;
use crate::codegen::limits::{Work, add, resource, within};
use crate::{Digest32, Error, Result, artifact::ArtifactIdentity};
use std::fmt::{self, Write};

// This is the exact ordinary local Rust body in puzzles/sha256tree/puzzle.rs
// and puzzles/inner-puzzle/puzzle.rs. Its names confer no compiler privilege.
pub(super) const HASH_HELPER: &str = r#"fn sha256tree<'a>(tree: &'a Tree<'a>) -> [u8; 32] {
    match tree {
        Tree::Atom(payload) => sha256_parts(&ATOM_DOMAIN, payload, &[]),
        Tree::Pair(first, rest) => {
            let first_hash = sha256tree(first);
            let rest_hash = sha256tree(rest);
            sha256_parts(&PAIR_DOMAIN, &first_hash, &rest_hash)
        }
    }
}
"#;

const DECLARATIONS: &str = r#"// Supplies a logical program witness and complete opaque environment.
// The guard commits its logical tree, not original wire/envelope/source identity.
// SHA-256 collision/second-preimage assumptions and runtime limits remain.
// No inferred ABI, authorization, complete-spend or preservation claim.
// Local helper: puzzles/sha256tree/puzzle.rs and puzzles/inner-puzzle/puzzle.rs.
// Algorithm: Chia-Network/chia_puzzles@cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8
// puzzles/sha256tree.clib lines 4-10, span SHA-256:
// 946a967edff3fdc1eed3260cc7a1f28972e6d78c3ea248a814a0cfe2599dec7e
// Apache-2.0; retained license and source provenance: puzzles/sha256tree/puzzle.json
// https://github.com/Chia-Network/chia_puzzles/blob/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/LICENSE
use chialisp_oxide::{Tree, apply, sha256_parts};

#[rustfmt::skip]
const ATOM_DOMAIN: [u8; 1] = [1];
#[rustfmt::skip]
const PAIR_DOMAIN: [u8; 1] = [2];
#[rustfmt::skip]
const EXPECTED_PROGRAM_TREE_HASH: [u8; 32] = ["#;

const RUN_HELPER: &str = r#"];

#[rustfmt::skip]
pub(super) fn run<'a>(program: &'a Tree<'a>, environment: &'a Tree<'a>) -> Tree<'static> {
    let program_hash = sha256tree(program);
    assert!(program_hash == EXPECTED_PROGRAM_TREE_HASH);
    apply(program, environment)
}

#[rustfmt::skip]
"#;

pub(super) fn emit(
    identity: &ArtifactIdentity,
    lock: Digest32,
    object: Digest32,
    limits: &WitnessGenerationLimits,
    work: &mut Work,
) -> Result<String> {
    let mut out = Source {
        text: String::new(),
        maximum: limits.max_source_bytes,
        work,
        failure: None,
    };
    out.emit(format_args!("// Generated ordinary Rust; WIP prototype, not ready for use.\n// Recipe: chialisp-oxide-witness-rust/v1\n// Raw lock SHA-256: "))?;
    out.digest(lock)?;
    out.emit(format_args!("\n// Envelope SHA-256: "))?;
    out.digest(object)?;
    out.emit(format_args!("\n// Artifact blob SHA-256: "))?;
    out.digest(identity.blob_sha256)?;
    out.emit(format_args!("\n// Artifact tree hash: "))?;
    out.digest(identity.module_tree_hash)?;
    out.emit(format_args!("\n// Artifact bytes: {}\n", identity.bytes))?;
    out.emit(format_args!("{DECLARATIONS}"))?;
    for (index, byte) in identity.module_tree_hash.as_bytes().iter().enumerate() {
        if index != 0 {
            out.emit(format_args!(", "))?;
        }
        out.emit(format_args!("0x{byte:02x}"))?;
    }
    out.emit(format_args!("{RUN_HELPER}"))?;
    out.emit(format_args!("{HASH_HELPER}"))?;
    Ok(out.text)
}

/// No arbitrary text enters this formatter: only fixed recipe text, numeric
/// byte literals and exact identity hex. Charge each append before allocation.
struct Source<'a> {
    text: String,
    maximum: usize,
    work: &'a mut Work,
    failure: Option<Error>,
}
impl Source<'_> {
    fn emit(&mut self, args: fmt::Arguments<'_>) -> Result<()> {
        self.write_fmt(args).map_err(|_| {
            self.failure
                .take()
                .unwrap_or_else(|| resource("witness source formatting failed"))
        })
    }
    fn digest(&mut self, value: Digest32) -> Result<()> {
        // Display would allocate a temporary hex String without precharging.
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
                add(self.text.len(), text.len(), "witness source size overflow")?,
                self.maximum,
                "witness source byte limit exceeded",
            )?;
            self.work.charge(text.len())?;
            self.text
                .try_reserve(text.len())
                .map_err(|_| resource("cannot allocate witness source bytes"))?;
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
