//! Bounded Unix child groups. Timeout/output failure is infrastructure failure.
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    io::{Read, Write},
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const MAX_OUTPUT: usize = 1024 * 1024;
pub(super) const DEADLINE: Duration = Duration::from_secs(180);

#[derive(Serialize)]
pub struct ProcessReceipt {
    pub role: String,
    pub argv: Vec<String>,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub exit_code: i32,
}

fn collect(mut input: impl Read, overflow: Arc<AtomicBool>) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            return Ok(bytes);
        }
        if bytes.len() + count > MAX_OUTPUT {
            overflow.store(true, Ordering::Release);
            return Ok(bytes);
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

/// No shell; caller constructs the fixed executable/argv. All group members are
/// killed before output-reader joins, including when the parent exits early.
pub(super) fn run(
    command: &mut Command,
    role: &str,
    deadline: Duration,
    logs: &Path,
) -> Result<ProcessReceipt> {
    ensure!(
        matches!(
            role,
            "native_cargo_test"
                | "rustc_identity"
                | "driver_identity"
                | "rust_off"
                | "rust_on"
                | "control"
        ),
        "unknown child role"
    );
    let argv = std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|part| {
            part.to_str()
                .context("child argv must be UTF-8")
                .map(str::to_owned)
        })
        .collect::<Result<Vec<_>>>()?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command.spawn().with_context(|| format!("start {role}"))?;
    let group = i32::try_from(child.id())?;
    let overflow = Arc::new(AtomicBool::new(false));
    let stdout = child.stdout.take().context("child stdout")?;
    let stderr = child.stderr.take().context("child stderr")?;
    let out_overflow = overflow.clone();
    let err_overflow = overflow.clone();
    let out = thread::spawn(move || collect(stdout, out_overflow));
    let err = thread::spawn(move || collect(stderr, err_overflow));
    let start = Instant::now();
    let mut timed_out = false;
    let mut wait_failed = false;
    let status = loop {
        if start.elapsed() >= deadline || overflow.load(Ordering::Acquire) {
            timed_out = start.elapsed() >= deadline;
            break None;
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                wait_failed = true;
                break None;
            }
        }
    };
    // SAFETY: the positive child PID was checked as i32 and the child was put
    // in its own process group. No user-supplied PID or signal is accepted.
    unsafe {
        libc::kill(-group, libc::SIGKILL);
    }
    let waited = child.wait();
    let stdout = out
        .join()
        .map_err(|_| anyhow::anyhow!("stdout collector panicked"))??;
    let stderr = err
        .join()
        .map_err(|_| anyhow::anyhow!("stderr collector panicked"))??;
    for (suffix, bytes) in [("stdout", &stdout), ("stderr", &stderr)] {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(logs.join(format!("{role}.{suffix}.log")))?;
        file.write_all(bytes)?;
    }
    ensure!(!timed_out, "{role}: child process deadline exceeded");
    ensure!(!wait_failed, "{role}: child wait failed");
    ensure!(
        !overflow.load(Ordering::Acquire),
        "{role}: child output limit exceeded"
    );
    let status = status
        .or_else(|| waited.ok())
        .context("child wait failed")?;
    ensure!(
        status.success(),
        "{role}: child failed; bounded logs retained in run directory"
    );
    Ok(ProcessReceipt {
        role: role.into(),
        argv,
        stdout_sha256: hex::encode(crate::digest(&stdout)),
        stderr_sha256: hex::encode(crate::digest(&stderr)),
        stdout_bytes: stdout.len(),
        stderr_bytes: stderr.len(),
        exit_code: status.code().context("child terminated by signal")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn control(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "fresh::process::tests::child_control",
                "--nocapture",
            ])
            .env("OXIDE_LIFECYCLE_CHILD_CONTROL", mode);
        command
    }
    #[test]
    fn child_control() {
        match std::env::var("OXIDE_LIFECYCLE_CHILD_CONTROL").as_deref() {
            Ok("overflow") => {
                std::io::stdout()
                    .write_all(&vec![b'x'; MAX_OUTPUT + 8192])
                    .unwrap();
            }
            Ok("sleep") => thread::sleep(Duration::from_secs(10)),
            Ok("descendant") => {
                let marker = std::env::var_os("OXIDE_LIFECYCLE_MARKER").unwrap();
                // This control intentionally exits while its descendant is
                // alive so the supervisor's group cleanup is exercised.
                #[allow(
                    clippy::zombie_processes,
                    reason = "intentional early-parent-exit control; supervisor kills inherited group"
                )]
                let _child = control("mark").spawn().unwrap();
                let start = Instant::now();
                while !Path::new(&marker).with_extension("ready").exists() {
                    assert!(start.elapsed() < Duration::from_secs(5));
                    thread::sleep(Duration::from_millis(1));
                }
            }
            Ok("mark") => {
                let marker = std::env::var_os("OXIDE_LIFECYCLE_MARKER").unwrap();
                std::fs::write(Path::new(&marker).with_extension("ready"), b"started").unwrap();
                thread::sleep(Duration::from_secs(1));
                std::fs::write(
                    std::env::var_os("OXIDE_LIFECYCLE_MARKER").unwrap(),
                    b"escaped",
                )
                .unwrap();
            }
            Ok("fail") => std::process::exit(7),
            _ => (),
        }
    }
    #[test]
    fn children_timeout_overflow_failure_and_success_are_bounded() {
        for (mode, expected) in [
            ("sleep", "deadline"),
            ("overflow", "output limit"),
            ("fail", "child failed"),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let deadline = if mode == "sleep" {
                Duration::from_millis(500)
            } else {
                Duration::from_secs(10)
            };
            let error = run(&mut control(mode), "control", deadline, directory.path())
                .err()
                .unwrap();
            assert!(error.to_string().contains(expected), "{mode}: {error}");
            assert!(
                std::fs::metadata(directory.path().join("control.stdout.log"))
                    .unwrap()
                    .len()
                    <= MAX_OUTPUT as u64
            );
        }
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            run(
                &mut control("pass"),
                "control",
                Duration::from_secs(5),
                directory.path()
            )
            .unwrap()
            .exit_code,
            0
        );
    }
    #[test]
    fn parent_exit_kills_started_descendant_group_before_it_can_write() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("marker");
        let mut command = control("descendant");
        command.env("OXIDE_LIFECYCLE_MARKER", &marker);
        assert!(
            run(
                &mut command,
                "control",
                Duration::from_secs(6),
                directory.path()
            )
            .is_ok()
        );
        assert!(marker.with_extension("ready").exists());
        thread::sleep(Duration::from_millis(1200));
        assert!(!marker.exists());
    }
}
