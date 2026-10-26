//! Bounded, shared transport for the complete three-action registry protocol.
use crate::{PuzzleReport, action_registry};
use anyhow::{Result, ensure};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;

const SLUGS: [&str; 3] = [
    "registry-initialize",
    "registry-set-alpha",
    "registry-set-beta",
];
const MODES: [(&str, bool); 5] = [
    ("rust_mir", false),
    ("rust_mir", true),
    ("chialisp", true),
    ("rue_source", false),
    ("rue_source", true),
];
const MEMBER_LIMIT: usize = 2 * 1024 * 1024;
const TOTAL_LIMIT: usize = 16 * 1024 * 1024;
const INDEX: &str = "registry-slot-protocol/index.json";

/// Exact relative names and bytes; the CLI materializes these beside its report.
/// Every journal is serialized and charged before the report acquires references.
pub struct RegistrySidecars {
    pub files: BTreeMap<String, Vec<u8>>,
}

fn encoded(value: &impl Serialize, remaining: usize) -> Result<Vec<u8>> {
    struct Bounded {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.limit - self.bytes.len() {
                return Err(std::io::Error::other(
                    "registry sidecar member exceeds 2MiB",
                ));
            }
            self.bytes
                .try_reserve_exact(bytes.len())
                .map_err(std::io::Error::other)?;
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Bounded {
        bytes: Vec::new(),
        limit: MEMBER_LIMIT.min(remaining),
    };
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.write_all(b"\n")?;
    Ok(writer.bytes)
}

fn descriptor(path: &str, bytes: &[u8]) -> serde_json::Value {
    json!({"path": path, "bytes": bytes.len(), "sha256": hex::encode(Sha256::digest(bytes))})
}

/// A selected registry view still requires the complete three-action group.
/// All fifteen artifact bytes are admitted by the producer before any SDK run.
pub fn registry_sidecars(reports: &mut [PuzzleReport]) -> Result<Option<RegistrySidecars>> {
    let selected = reports
        .iter()
        .filter(|r| SLUGS.contains(&r.slug.as_str()))
        .count();
    if selected == 0 {
        return Ok(None);
    }
    ensure!(
        selected == 3,
        "registry validation requires initialize, set-alpha and set-beta together"
    );
    let mut selected_artifacts = Vec::new();
    let mut remaining_module_bytes = 9 * 1024 * 1024;
    for slug in SLUGS {
        let matches = reports
            .iter()
            .filter(|r| r.slug == slug)
            .collect::<Vec<_>>();
        ensure!(
            matches.len() == 1,
            "registry action must occur exactly once"
        );
        let report = matches[0];
        ensure!(
            report.artifacts.len() == MODES.len(),
            "registry complete artifact inventory"
        );
        let mut actions = Vec::new();
        for (backend, optimize) in MODES {
            let artifacts = report
                .artifacts
                .iter()
                .filter(|a| a.backend == backend && a.optimize_lir == optimize)
                .collect::<Vec<_>>();
            ensure!(artifacts.len() == 1, "registry exact artifact modes");
            let artifact = artifacts[0];
            ensure!(
                artifact.bytes > 0
                    && artifact.bytes <= 1024 * 1024
                    && artifact.bytes <= remaining_module_bytes
                    && artifact.clvm_hex.len() == 2 * artifact.bytes,
                "registry artifact allocation precharge"
            );
            remaining_module_bytes -= artifact.bytes;
            actions.push(artifact);
        }
        selected_artifacts.push(actions);
    }
    // Finish the complete inventory preflight before any decoded-byte allocation.
    let modules = selected_artifacts
        .iter()
        .map(|actions| {
            actions
                .iter()
                .map(|artifact| hex::decode(&artifact.clvm_hex))
                .collect::<std::result::Result<Vec<_>, _>>()
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let builds = MODES
        .iter()
        .enumerate()
        .map(
            |(i, (backend, optimize_lir))| action_registry::RegistryBuild {
                backend,
                optimize_lir: *optimize_lir,
                initialize: &modules[0][i],
                set_alpha: &modules[1][i],
                set_beta: &modules[2][i],
            },
        )
        .collect::<Vec<_>>();
    let record = action_registry::validate(&builds)?;
    let mut files = BTreeMap::new();
    let mut inventory = Vec::new();
    let mut total = 0usize;
    for build in record.builds {
        let mut journals = BTreeMap::new();
        for lane in ["ordinary", "adversarial"] {
            let bytes = if lane == "ordinary" {
                encoded(&build.ordinary, TOTAL_LIMIT - total)?
            } else {
                encoded(&build.adversarial, TOTAL_LIMIT - total)?
            };
            let path = format!("registry-slot-protocol/{}-{lane}.json", build.mode);
            total = total
                .checked_add(bytes.len())
                .filter(|n| *n <= TOTAL_LIMIT)
                .ok_or_else(|| anyhow::anyhow!("registry sidecar aggregate exceeds 16MiB"))?;
            journals.insert(lane, descriptor(&path, &bytes));
            ensure!(
                files.insert(path, bytes).is_none(),
                "duplicate registry journal"
            );
        }
        inventory.push(json!({"backend":build.backend,"optimize_lir":build.optimize_lir,"mode":build.mode,"artifacts":build.artifacts,"counts":build.counts,"journals":journals}));
    }
    let index = encoded(
        &json!({"schema_version":1,"transport":"registry-slot-sidecars/v1","header":record.header,"builds":inventory}),
        TOTAL_LIMIT - total,
    )?;
    ensure!(
        total
            .checked_add(index.len())
            .is_some_and(|n| n <= TOTAL_LIMIT),
        "registry sidecar aggregate including index"
    );
    let index_descriptor = descriptor(INDEX, &index);
    files.insert(INDEX.into(), index);
    ensure!(files.len() == 11, "registry closed eleven-file transport");
    for report in reports
        .iter_mut()
        .filter(|r| SLUGS.contains(&r.slug.as_str()))
    {
        report.protocol_validation = Some(
            json!({"protocol":"registry-slot-sidecars/v1","role":report.slug,"index":index_descriptor}),
        );
        report.validated = true;
    }
    Ok(Some(RegistrySidecars { files }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialization_charges_escaping_and_final_newline_against_remaining_bytes() {
        let value = "\"\\\n";
        let bytes = encoded(&value, MEMBER_LIMIT).unwrap();
        assert_eq!(bytes, b"\"\\\"\\\\\\n\"\n");
        assert_eq!(encoded(&value, bytes.len()).unwrap(), bytes);
        assert!(encoded(&value, bytes.len() - 1).is_err());
    }

    #[test]
    fn member_limit_stays_distinct_from_shared_aggregate_budget() {
        let value = "a".repeat(MEMBER_LIMIT);
        assert!(encoded(&value, TOTAL_LIMIT).is_err());
        assert!(encoded(&"small", 0).is_err());
    }
}
