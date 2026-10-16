//! Solver failures must never turn an unresolved obligation into a proof.
#![cfg(unix)]

use chialisp_oxide_ir::{Expr, Parameter, Program, Type};
use chialisp_oxide_verifier::{Obligation, Predicate, SolverConfig, Specification, verify};
use std::{
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

struct Script(PathBuf);
impl Script {
    fn new(body: &str) -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "oxide-test-solver-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}
impl Drop for Script {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn attempt(
    script: &Script,
    timeout: Duration,
) -> anyhow::Result<chialisp_oxide_verifier::Verification> {
    let program = Program {
        name: "identity".into(),
        parameters: vec![Parameter {
            name: "value".into(),
            ty: Type::Int {
                bits: 8,
                signed: true,
            },
        }],
        result: Type::Int {
            bits: 8,
            signed: true,
        },
        body: Expr::Arg { index: 0 },
    };
    verify(
        &program,
        &Specification {
            assumptions: Predicate::default(),
            obligation: Obligation::OnSuccess {
                property: Predicate::default(),
            },
        },
        &SolverConfig {
            executable: script.0.clone(),
            timeout,
        },
    )
}

#[test]
fn unknown_malformed_missing_witness_and_solver_errors_fail_closed() {
    for body in [
        "cat >/dev/null\nprintf 'unknown\\n'",
        "cat >/dev/null\nprintf '(error broken)\\n'",
        "cat >/dev/null\nprintf 'sat\\nsat\\n'",
        "cat >/dev/null\nprintf 'sat\\n'", // No requested model.
        "cat >/dev/null\nprintf 'sat\\n' >&2; exit 1",
    ] {
        let script = Script::new(body);
        assert!(attempt(&script, Duration::from_secs(2)).is_err(), "{body}");
    }
}

#[test]
fn wall_timeout_kills_solver_even_if_it_ignores_smt_timeout() {
    let script = Script::new("exec sleep 30");
    let start = Instant::now();
    let error = attempt(&script, Duration::from_millis(5)).unwrap_err();
    assert!(error.to_string().contains("wall-clock timeout"), "{error}");
    assert!(start.elapsed() < Duration::from_secs(5));
}
