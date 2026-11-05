//! Local SDK capture for eight actual Rust condition-omission artifacts.
//! Complete Cargo admission belongs to the caller.
use anyhow::{Context, Result, ensure};
use chialisp_oxide_corpus::{
    RegistryControlBuild, RegistryControlKind, RegistryControlReferences, RegistryControlSide,
    RegistrySourceControlRecord, validate_registry_source_controls,
};
use chialisp_oxide_simulator::{
    CONSENSUS_VERSION, NETWORK, REJECTION_PROTOCOL, SIMULATOR_VERSION, VM_VERSION,
};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

const REQUEST_PROTOCOL: &str = "chialisp-oxide-registry-source-controls-request/v1";
const CAPTURE_PROTOCOL: &str = "chialisp-oxide-registry-source-controls-capture/v1";
const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;
const MAX_MODULE_BYTES: usize = 1024 * 1024;
const MAX_AGGREGATE_MODULE_BYTES: usize = 9 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const REFERENCES: [&str; 3] = [
    include_str!("../../../puzzles/registry-initialize/reference.clsp"),
    include_str!("../../../puzzles/registry-set-alpha/reference.clsp"),
    include_str!("../../../puzzles/registry-set-beta/reference.clsp"),
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: String,
    // Fixed arrays reject partial/extra inventories while parsing.
    runs: [Input; 8],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    name: String,
    optimize_lir: bool,
    entry: String,
    module_hex: String,
}
#[derive(Serialize)]
struct Capture {
    protocol: &'static str,
    sdk: &'static str,
    consensus: &'static str,
    vm: &'static str,
    network: &'static str,
    rejection_protocol: &'static str,
    record: RegistrySourceControlRecord,
}

const CONTROLS: [(&str, &str, RegistryControlSide, RegistryControlKind); 4] = [
    (
        "alpha-drop-send",
        "chialisp_oxide_registry_controls::alpha_drop_send::puzzle",
        RegistryControlSide::Alpha,
        RegistryControlKind::DropSend,
    ),
    (
        "alpha-drop-create",
        "chialisp_oxide_registry_controls::alpha_drop_create::puzzle",
        RegistryControlSide::Alpha,
        RegistryControlKind::DropCreate,
    ),
    (
        "beta-drop-send",
        "chialisp_oxide_registry_controls::beta_drop_send::puzzle",
        RegistryControlSide::Beta,
        RegistryControlKind::DropSend,
    ),
    (
        "beta-drop-create",
        "chialisp_oxide_registry_controls::beta_drop_create::puzzle",
        RegistryControlSide::Beta,
        RegistryControlKind::DropCreate,
    ),
];

/// Buffer the complete bounded encoding before touching stdout. Serialization,
/// allocation and stdout failures are infrastructure errors, never rejections.
struct BoundedOutput {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("registry control output byte bound"));
        }
        self.bytes
            .try_reserve_exact(bytes.len())
            .map_err(io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args_os();
    let _executable = args.next();
    let path = args
        .next()
        .context("usage: registry_controls REQUEST.json")?;
    ensure!(
        args.next().is_none(),
        "usage: registry_controls REQUEST.json"
    );
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_REQUEST_BYTES as u64,
        "request must be a bounded regular file"
    );
    let mut bytes = Vec::new();
    file.take((MAX_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_REQUEST_BYTES,
        "request grew beyond byte bound"
    );
    // serde_json's default recursion bound stays enabled.
    let request: Request = serde_json::from_slice(&bytes)?;
    ensure!(
        request.protocol == REQUEST_PROTOCOL,
        "exact request protocol"
    );
    let mut remaining = MAX_AGGREGATE_MODULE_BYTES;
    // Validate the complete ordered inventory and charge all candidate lengths
    // before any hex decode, candidate parsing, reference compilation or SDK work.
    for (index, input) in request.runs.iter().enumerate() {
        let (name, entry, _, _) = CONTROLS[index / 2];
        ensure!(
            input.name == name && input.entry == entry && input.optimize_lir == (index % 2 == 1),
            "exact ordered control identity and optimization mode"
        );
        let hex = input.module_hex.as_bytes();
        ensure!(
            !hex.is_empty()
                && hex.len() % 2 == 0
                && hex.len() <= 2 * MAX_MODULE_BYTES
                && hex.len() / 2 <= remaining,
            "candidate module size/aggregate predecode bound"
        );
        remaining -= hex.len() / 2;
        ensure!(
            hex.iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b)),
            "canonical lowercase module hex"
        );
    }
    // References are fixed compiled-in source, never request-selected programs.
    // The producer independently recompiles each once more and compares bytes.
    let mut references = Vec::with_capacity(3);
    for source in REFERENCES {
        let module = chialisp_oxide_reference::compile(source)?;
        ensure!(
            !module.is_empty() && module.len() <= MAX_MODULE_BYTES && module.len() <= remaining,
            "reference module size/aggregate bound"
        );
        remaining -= module.len();
        references.push(module);
    }
    // All eleven lengths have now been charged, before the first hex allocation.
    let modules = request
        .runs
        .iter()
        .map(|input| hex::decode(&input.module_hex).context("candidate module hex"))
        .collect::<Result<Vec<_>>>()?;
    let builds = modules
        .iter()
        .enumerate()
        .map(|(index, module)| {
            let (_, entry, side, kind) = CONTROLS[index / 2];
            RegistryControlBuild {
                side,
                kind,
                backend: "rust_mir",
                optimize_lir: index % 2 == 1,
                entry,
                module,
            }
        })
        .collect::<Vec<_>>();
    let record = validate_registry_source_controls(
        &builds,
        RegistryControlReferences {
            initialize: &references[0],
            set_alpha: &references[1],
            set_beta: &references[2],
        },
    )?;
    let capture = Capture {
        protocol: CAPTURE_PROTOCOL,
        sdk: SIMULATOR_VERSION,
        consensus: CONSENSUS_VERSION,
        vm: VM_VERSION,
        network: NETWORK,
        rejection_protocol: REJECTION_PROTOCOL,
        record,
    };
    // Keep each journal inside the existing two-MiB transport role.
    // This is a serialization feasibility check, not another SDK execution.
    for group in &capture.record.groups {
        let mut group_output = BoundedOutput {
            bytes: Vec::new(),
            limit: 2 * 1024 * 1024,
        };
        serde_json::to_writer_pretty(&mut group_output, group)?;
        group_output.write_all(b"\n")?;
    }
    let mut output = BoundedOutput {
        bytes: Vec::new(),
        limit: MAX_OUTPUT_BYTES,
    };
    serde_json::to_writer_pretty(&mut output, &capture)?;
    output.write_all(b"\n")?;
    let mut stdout = io::stdout().lock();
    stdout.write_all(&output.bytes)?;
    stdout.flush()?;
    Ok(())
}
