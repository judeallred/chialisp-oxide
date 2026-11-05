//! Actual embedded reference-provider identity, distinct from source manifests.
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(super) fn modern_cl24_record() -> Result<Value> {
    let provider = chialisp_oxide_reference::modern_cl24_provider()?;
    let dialect_hash = hex::encode(Sha256::digest(provider.dialect_content.as_bytes()));
    let macro_hash = hex::encode(Sha256::digest(provider.macro_content.as_bytes()));
    ensure!(
        dialect_hash == provider.dialect_content_sha256,
        "actual embedded dialect digest changed"
    );
    ensure!(
        macro_hash == provider.macro_content_sha256,
        "actual embedded macro digest changed"
    );
    Ok(json!({
        "protocol": provider.protocol,
        "compiler": provider.compiler,
        "crate": {"name": "chialisp", "version": "0.4.6", "checksum": provider.crate_checksum,
            "vcs_revision": provider.vcs_revision, "license": provider.license,
            "compiler_source_sha256": provider.compiler_source_sha256,
            "dialect_source_sha256": provider.dialect_source_sha256,
            "preprocessor_source_sha256": provider.preprocessor_source_sha256,
            "license_sha256": provider.license_sha256},
        "dialect": {"name": provider.dialect_name, "content": provider.dialect_content,
            "sha256": dialect_hash, "stepping": provider.accepted.stepping,
            "strict": provider.accepted.strict, "int_fix": provider.accepted.int_fix,
            "extra_numeric_constants": provider.accepted.extra_numeric_constants},
        "macros": {"name": provider.macro_name, "content": provider.macro_content, "sha256": macro_hash},
        "scope": "Actual embedded API bytes checked against reviewed pins; closed cl24 reads only. Package/source metadata and freshness do not authenticate installed code, execution or compiler preservation. No filesystem include or general modern include support."
    }))
}
