use std::{
    path::Path,
    process::{Command, Output},
};

fn invoke(operation: &str, path: &Path, steps: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_chialisp-oxide-lifecycle"));
    command
        .args(["simulator", operation, "--journal"])
        .arg(path);
    if let Some(steps) = steps {
        command.args(["--steps", steps]);
    }
    command.output().unwrap()
}

#[test]
fn cli_partial_run_read_only_status_and_resume_report_actual_children() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .canonicalize()
        .unwrap()
        .join("cli.lifecycle.json");
    let partial = invoke("run", &path, Some("3"));
    assert!(
        partial.status.success(),
        "{}",
        String::from_utf8_lossy(&partial.stderr)
    );
    let before = std::fs::read(&path).unwrap();
    let status = invoke("status", &path, None);
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let resumed = invoke("resume", &path, None);
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    let view: serde_json::Value = serde_json::from_slice(&resumed.stdout).unwrap();
    assert_eq!(view["stage"], "confirmed");
    assert_eq!(view["backend"], "seeded_sdk_simulator_only");
    assert_eq!(view["checks"]["fresh_cargo_build"], "unavailable");
    assert_eq!(view["children"].as_array().unwrap().len(), 1);
    assert_eq!(view["children"][0]["coin"]["amount"], 1000);
    assert_eq!(
        view["children"][0]["coin"]["puzzle_hash"],
        serde_json::json!(vec![0x42; 32])
    );
    let completed = std::fs::read(&path).unwrap();
    assert!(!invoke("run", &path, None).status.success());
    assert_eq!(std::fs::read(&path).unwrap(), completed);
}

#[test]
fn cli_refuses_unrelated_files_and_non_simulator_commands() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .canonicalize()
        .unwrap()
        .join("unrelated.lifecycle.json");
    std::fs::write(&path, b"unrelated bytes").unwrap();
    assert!(!invoke("resume", &path, None).status.success());
    assert!(!invoke("run", &path, None).status.success());
    assert_eq!(std::fs::read(&path).unwrap(), b"unrelated bytes");
    assert!(
        !Command::new(env!("CARGO_BIN_EXE_chialisp-oxide-lifecycle"))
            .args(["mainnet", "run"])
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn fresh_cli_refuses_saved_receipt_shortcuts_and_requires_explicit_checks() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .canonicalize()
        .unwrap()
        .join("preserve.lifecycle.json");
    std::fs::write(&path, b"preserve unrelated bytes").unwrap();
    for options in [
        vec!["--report", "saved-report.json"],
        vec!["--mode", "arbitrary"],
        vec!["--mode", "on", "--mode", "off"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_chialisp-oxide-lifecycle"))
            .args(["fresh", "status", "--journal"])
            .arg(&path)
            .args(options)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), b"preserve unrelated bytes");
    }
}
