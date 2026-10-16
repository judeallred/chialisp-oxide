// Shared Cargo consumer fixture for integration tests.
#![allow(dead_code)]
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

pub struct Fixture(pub PathBuf);
impl Fixture {
    pub fn new() -> Self {
        Self::with_sdk_alias("chialisp-oxide")
    }
    pub fn with_sdk_alias(alias: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "chialisp-oxide-cargo-driver-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir_all(path.join("src")).unwrap();
        let sdk = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../chialisp-oxide")
            .canonicalize()
            .unwrap();
        std::fs::write(path.join("Cargo.toml"), format!("[package]\nname=\"chialisp-oxide-driver-cargo-fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n[lib]\nname=\"wrapper_fixture\"\n[dependencies]\n{alias}={{package=\"chialisp-oxide\",path={}}}\n", serde_json::to_string(&sdk).unwrap())).unwrap();
        Self(path)
    }
    pub fn source(&self, source: &str) {
        std::fs::write(self.0.join("src/lib.rs"), source).unwrap();
    }
    pub fn compiler(&self) -> PathBuf {
        Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/rustc")
    }
    pub fn host(&self) -> String {
        let output = Command::new(self.compiler()).arg("-vV").output().unwrap();
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .unwrap()
            .into()
    }
    pub fn cargo(&self, wrapped: bool) -> std::process::Output {
        let mut command = Command::new(Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/cargo"));
        command
            .current_dir(&self.0)
            .env("RUSTC", self.compiler())
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER")
            .env_remove("CHIALISP_OXIDE_CARGO_MODE");
        command
            .args(["check", "--lib", "--offline", "--jobs", "1", "--target-dir"])
            .arg(self.0.join(if wrapped {
                "extract-build"
            } else {
                "native-build"
            }));
        if wrapped {
            command
                .args(["--target", &self.host()])
                .env("RUSTC_WRAPPER", env!("CARGO_BIN_EXE_chialisp-oxide-driver"))
                .env("CHIALISP_OXIDE_CARGO_MODE", "1")
                .env("CHIALISP_OXIDE_PRIMARY_PACKAGES", "wrapper_fixture")
                .env("CHIALISP_OXIDE_OUTPUT_DIR", self.0.join("exports"));
        }
        command.output().unwrap()
    }
    pub fn exports(&self) -> Vec<chialisp_oxide_ir::Program> {
        let dir = self.0.join("exports");
        if !dir.exists() {
            return Vec::new();
        }
        std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| {
                serde_json::from_slice(&std::fs::read(entry.unwrap().path()).unwrap()).unwrap()
            })
            .collect()
    }
    pub fn dependency(&self, crate_name: &str, encode_mir: bool) -> PathBuf {
        self.dependency_source(
            crate_name,
            encode_mir,
            "pub fn identity(value:u64)->u64 { value }",
        )
    }
    pub fn dependency_source(&self, crate_name: &str, encode_mir: bool, contents: &str) -> PathBuf {
        let source = self.0.join("external.rs");
        std::fs::write(&source, contents).unwrap();
        let mut command = Command::new(self.compiler());
        command
            .args([
                "--crate-name",
                crate_name,
                "--crate-type",
                "rlib",
                "--edition",
                "2024",
                "--emit",
                "metadata",
                "--out-dir",
            ])
            .arg(&self.0)
            .arg(source);
        if encode_mir {
            command.arg("-Zalways-encode-mir");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        self.0.join(format!("lib{crate_name}.rmeta"))
    }
    pub fn check_external(&self, alias: &str, metadata: &Path) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_chialisp-oxide-driver"))
            .arg("--cargo-wrapper")
            .arg(self.compiler())
            .args([
                "--crate-name",
                "wrapper_fixture",
                "--crate-type",
                "lib",
                "--edition",
                "2024",
                "--emit",
                "metadata",
                "--target",
                &self.host(),
                "--extern",
            ])
            .arg(format!("{alias}={}", metadata.display()))
            .arg("--out-dir")
            .arg(&self.0)
            .arg(self.0.join("src/lib.rs"))
            .env("CARGO_PRIMARY_PACKAGE", "1")
            .env("CHIALISP_OXIDE_OUTPUT_DIR", self.0.join("exports"))
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
