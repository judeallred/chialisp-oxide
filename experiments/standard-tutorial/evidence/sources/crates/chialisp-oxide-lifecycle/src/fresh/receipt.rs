//! Bounded new-file receipts, including completed observations on failure.
use super::*;
use std::io::{self, Write};
const MAX_RECEIPT: usize = 16 * 1024 * 1024;

struct BoundedWriter<W> {
    inner: W,
    remaining: usize,
}
impl<W: Write> Write for BoundedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.remaining {
            return Err(io::Error::other("fresh receipt byte limit"));
        }
        let count = self.inner.write(bytes)?;
        self.remaining -= count;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
pub(super) fn write_new(path: &Path, value: &impl Serialize) -> Result<()> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let mut writer = BoundedWriter {
        inner: file,
        remaining: MAX_RECEIPT,
    };
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.flush()?;
    Ok(())
}
#[derive(Serialize)]
pub(super) struct Observations<'a> {
    pub processes: &'a [ProcessReceipt],
    pub artifacts: &'a [FreshArtifact],
    pub vm: &'a [VmObservation],
    pub simulator: &'a [SimulatorObservation],
    pub sdk_transaction_attempts: u32,
}
pub(super) fn failed<T>(
    output: &Path,
    stage: &str,
    error: anyhow::Error,
    observations: Observations<'_>,
) -> Result<T> {
    #[derive(Serialize)]
    struct Failure<'a> {
        protocol: &'static str,
        overall_status: &'static str,
        scope: &'static str,
        primary_vm: &'static str,
        primary_vm_flags: u32,
        primary_vm_max_cost: u64,
        sdk: &'static str,
        consensus: &'static str,
        sdk_vm: &'static str,
        rejection_protocol: &'static str,
        network: &'static str,
        stage: &'a str,
        error: String,
        observations: Observations<'a>,
    }
    let receipt = Failure {
        protocol: "chialisp-oxide-lifecycle-failed-observations/v1",
        overall_status: "failed",
        scope: "Completed actual observations retained within bounds; incomplete stages add no execution credit; not a successful fresh check or authentication",
        primary_vm: chialisp_oxide_vm::FAILURE_PROTOCOL,
        primary_vm_flags: clvmr::chia_dialect::MEMPOOL_MODE.bits(),
        primary_vm_max_cost: MAX_COST,
        sdk: chialisp_oxide_simulator::SIMULATOR_VERSION,
        consensus: chialisp_oxide_simulator::CONSENSUS_VERSION,
        sdk_vm: chialisp_oxide_simulator::VM_VERSION,
        rejection_protocol: chialisp_oxide_simulator::REJECTION_PROTOCOL,
        network: chialisp_oxide_simulator::NETWORK,
        stage,
        error: error.to_string().chars().take(8192).collect(),
        observations,
    };
    write_new(&output.join("failed-observations.json"), &receipt)
        .context("retain bounded failure observations")?;
    Err(error)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receipts_are_bounded_and_never_overwrite_existing_files() {
        let mut writer = BoundedWriter {
            inner: vec![],
            remaining: 8,
        };
        assert!(serde_json::to_writer(&mut writer, &"too much output").is_err());
        assert!(writer.inner.len() <= 8);
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("receipt.json");
        write_new(&file, &serde_json::json!({"status":"failed"})).unwrap();
        let before = fs::read(&file).unwrap();
        assert!(write_new(&file, &serde_json::json!({"status":"success"})).is_err());
        assert_eq!(fs::read(file).unwrap(), before);
    }
    #[test]
    fn failed_sidecar_keeps_actual_and_expected_codes_and_overall_failure() {
        let root = tempfile::tempdir().unwrap();
        let failure =
            chialisp_oxide_vm::VmFailure::from_code(chialisp_oxide_vm::FailureCode::CostExceeded);
        let observation = VmObservation {
            lane: "rust_off".into(),
            case: "missing".into(),
            environment_hex: "80".into(),
            cost: None,
            outcome: VmOutcome::Inconclusive { failure },
            expected_outcome: VmOutcome::Rejected {
                failure: chialisp_oxide_vm::VmFailure::from_code(
                    chialisp_oxide_vm::FailureCode::Raise,
                ),
            },
        };
        let result: Result<()> = failed(
            root.path(),
            "vm_oracle",
            anyhow::anyhow!("exact outcome mismatch"),
            Observations {
                processes: &[],
                artifacts: &[],
                vm: &[observation],
                simulator: &[],
                sdk_transaction_attempts: 0,
            },
        );
        assert!(result.is_err());
        let value: Value = serde_json::from_slice(
            &fs::read(root.path().join("failed-observations.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(value["overall_status"], "failed");
        assert_eq!(
            value["observations"]["vm"][0]["outcome"]["failure"]["code"],
            "cost_exceeded"
        );
        assert_eq!(
            value["observations"]["vm"][0]["expected_outcome"]["failure"]["code"],
            "raise"
        );
        assert!(!root.path().join("report.json").exists());
    }
}
