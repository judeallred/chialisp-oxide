//! Shared actual Cargo/native fixture plumbing; no source-semantics shortcut.
use super::{support::Fixture, vm};
use chialisp_oxide_backend_experiment::Outcome;
use std::{path::Path, process::Command};

pub fn fixture(source: &str) -> Fixture {
    let fixture = Fixture::new();
    fixture.source(source);
    let manifest = fixture.0.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    let text = text.replace("path=", "features=[\"alloc\"],path=");
    let dependency = text
        .lines()
        .find(|line| line.starts_with("chialisp-oxide="))
        .unwrap()
        .replace("[\"alloc\"]", "[\"alloc\",\"vm\"]");
    std::fs::write(
        manifest,
        format!("{text}[dev-dependencies]\n{dependency}\n"),
    )
    .unwrap();
    fixture
}
pub fn native(fixture: &Fixture, groups: usize) {
    let output = Command::new(Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/cargo"))
        .current_dir(&fixture.0)
        .env("RUSTC", fixture.compiler())
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CHIALISP_OXIDE_CARGO_MODE")
        .args(["test", "--lib", "--offline", "--jobs", "2", "--target-dir"])
        .arg(fixture.0.join("native-build"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "native stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!(
        "native fixture {}\n{}",
        fixture.0.display(),
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(&format!("{groups} passed")));
}
pub fn expected(value: &str) -> Outcome {
    let code = chialisp_oxide_reference::compile(&format!("(mod () (q . {value}))")).unwrap();
    vm::execute(&code, "()", 20_000_000).unwrap().outcome
}
