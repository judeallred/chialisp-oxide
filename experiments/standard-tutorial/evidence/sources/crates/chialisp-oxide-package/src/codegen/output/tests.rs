//! Actual host filesystem controls; these establish no compiler or VM result.
use super::*;
use crate::{
    Limits,
    codegen::{GenerationLimits, generate_locked_rust},
    dependency::verify_lock,
    verify_package,
};
use serde_json::json;

#[path = "tests/witness.rs"]
mod witness;

struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    registry: Registry,
    lock: PathBuf,
    output: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let registry = Registry::create(&root.join("registry"), Limits::default()).unwrap();
        let mut pins = Vec::new();
        for (name, byte, leaf) in [("first", 0x80_u8, &[][..]), ("second", 1, &[1][..])] {
            let mut leaf_input = vec![1];
            leaf_input.extend_from_slice(leaf);
            let package = json!({"schema_version":1,"kind":"clvm_artifact",
                "artifact":{"identity":{"encoding":"clvm-legacy-plain/v1","bytes":1,
                    "blob_sha256":digest(&[byte]),"module_tree_hash":digest(&leaf_input)},
                    "clvm_hex":hex::encode([byte])},
                "rust":null,"source_scope":"not_recorded","sources":[],"provenance":null});
            let package =
                verify_package(&serde_json::to_vec(&package).unwrap(), &Limits::default()).unwrap();
            registry.publish(&package).unwrap();
            pins.push(json!({"name":name,"object_id":package.object_id(),"artifact":package.artifact().identity(),"binding":null}));
        }
        let lock = root.join("chialisp-oxide.lock");
        std::fs::write(&lock, serde_json::to_vec(&json!({"schema_version":1,
            "protocol":"chialisp-oxide-dependencies/v1","scope":"local_exact_artifact_pins","dependencies":pins})).unwrap()).unwrap();
        Self {
            output: root.join("module.rs"),
            root,
            registry,
            lock,
            _temporary: temporary,
        }
    }
    fn generated(&self, alias: &str) -> GeneratedRust {
        let lock = dependency::read_lock(&self.lock).unwrap();
        generate_locked_rust(&lock, &self.registry, alias, &GenerationLimits::default()).unwrap()
    }
    fn write(&self, generated: &GeneratedRust, check: bool) -> Result<OutputRecord> {
        write_generated_rust(generated, &self.output, &self.lock, &self.registry, check)
    }
    fn stages(&self) -> Vec<PathBuf> {
        std::fs::read_dir(&self.root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .as_encoded_bytes()
                    .windows(7)
                    .any(|bytes| bytes == b".stage-")
            })
            .collect()
    }
}
fn kind<T>(result: Result<T>, expected: ErrorKind) {
    match result {
        Ok(_) => panic!("expected {expected:?}"),
        Err(error) => assert_eq!(error.kind(), expected, "{error}"),
    }
}

#[cfg(unix)]
#[test]
fn absent_identical_different_and_readonly_check_have_exact_immutable_results() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let generated = fixture.generated("first");
    let lock_before = std::fs::read(&fixture.lock).unwrap();
    kind(fixture.write(&generated, true), ErrorKind::Conflict);
    assert!(!fixture.output.exists());
    let first = fixture.write(&generated, false).unwrap();
    assert_eq!(first.status, OutputStatus::Created);
    assert_eq!(first.source_sha256, digest(generated.source().as_bytes()));
    assert_eq!(first.source_bytes, generated.source().len() as u64);
    assert_eq!(
        std::fs::metadata(&fixture.output)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let metadata = std::fs::metadata(&fixture.output).unwrap();
    assert_eq!(
        fixture.write(&generated, false).unwrap().status,
        OutputStatus::AlreadyPresent
    );
    assert_eq!(
        fixture.write(&generated, true).unwrap().status,
        OutputStatus::Checked
    );
    assert_eq!(
        std::fs::metadata(&fixture.output)
            .unwrap()
            .modified()
            .unwrap(),
        metadata.modified().unwrap()
    );
    let different = fixture.generated("second");
    for check in [false, true] {
        kind(fixture.write(&different, check), ErrorKind::Conflict);
    }
    assert_eq!(
        std::fs::read(&fixture.output).unwrap(),
        generated.source().as_bytes()
    );
    assert_eq!(std::fs::read(&fixture.lock).unwrap(), lock_before);
    assert!(fixture.stages().is_empty());
}

#[test]
fn readonly_check_is_portable_and_does_not_create_missing_parents() {
    let fixture = Fixture::new();
    let generated = fixture.generated("first");
    std::fs::write(&fixture.output, generated.source()).unwrap();
    assert_eq!(
        fixture.write(&generated, true).unwrap().status,
        OutputStatus::Checked
    );
    let path = fixture.root.join("missing/module.rs");
    assert!(
        write_generated_rust(&generated, &path, &fixture.lock, &fixture.registry, true).is_err()
    );
    assert!(!fixture.root.join("missing").exists());
    assert!(fixture.stages().is_empty());
}

#[cfg(unix)]
#[test]
fn supplied_lock_bytes_are_bound_before_initial_output_and_again_before_commit() {
    for phase in [None, Some(Phase::BeforeCommit)] {
        let fixture = Fixture::new();
        let generated = fixture.generated("first");
        let change = || {
            let mut bytes = std::fs::read(&fixture.lock).unwrap();
            bytes.push(b' ');
            assert!(verify_lock(&bytes).is_ok());
            std::fs::write(&fixture.lock, bytes).unwrap();
        };
        if phase.is_none() {
            change();
        }
        let mut reached = false;
        let result = write_with(
            &generated,
            &fixture.output,
            &fixture.lock,
            &fixture.registry,
            false,
            &mut |at, _| {
                if Some(at) == phase {
                    change();
                    reached = true;
                }
                Ok(())
            },
        );
        kind(result, ErrorKind::Conflict);
        assert_eq!(reached, phase.is_some());
        assert!(!fixture.output.exists());
        assert!(fixture.stages().is_empty());
    }
}

#[cfg(unix)]
#[test]
fn reserved_cargo_registry_and_dependency_lock_destinations_fail_before_staging() {
    let fixture = Fixture::new();
    let generated = fixture.generated("first");
    let before = std::fs::read(&fixture.lock).unwrap();
    std::fs::create_dir(fixture.registry.root().join("nested")).unwrap();
    for path in [
        fixture.lock.clone(),
        fixture.root.join("Cargo.toml"),
        fixture.root.join("Cargo.lock"),
        fixture.root.join("CARGO.LOCK"),
        fixture.root.join("Cargo.locK"),
        fixture.registry.root().join("module.rs"),
        fixture.registry.root().join("nested/module.rs"),
    ] {
        kind(
            write_generated_rust(&generated, &path, &fixture.lock, &fixture.registry, false),
            ErrorKind::InvalidPath,
        );
    }
    assert_eq!(std::fs::read(&fixture.lock).unwrap(), before);
    assert!(!fixture.root.join("Cargo.toml").exists());
    assert!(!fixture.root.join("Cargo.lock").exists());
    assert!(fixture.stages().is_empty());
    #[cfg(target_os = "macos")]
    {
        // This actual filesystem may be case-sensitive. If it aliases this
        // alternate spelling, dev+ino containment still rejects it.
        let alias = fixture.root.join("REGISTRY");
        if alias.exists() {
            kind(
                write_generated_rust(
                    &generated,
                    &alias.join("module.rs"),
                    &fixture.lock,
                    &fixture.registry,
                    false,
                ),
                ErrorKind::InvalidPath,
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn symlink_components_hardlink_aliases_and_nonregular_outputs_are_never_opened() {
    use std::os::unix::{fs::symlink, net::UnixListener};
    let fixture = Fixture::new();
    let generated = fixture.generated("first");
    let protected = fixture.root.join("protected.rs");
    std::fs::write(&protected, b"protected original bytes").unwrap();
    symlink(&protected, &fixture.output).unwrap();
    kind(fixture.write(&generated, false), ErrorKind::InvalidPath);
    std::fs::remove_file(&fixture.output).unwrap();
    symlink(fixture.root.join("missing.rs"), &fixture.output).unwrap();
    kind(fixture.write(&generated, false), ErrorKind::InvalidPath);
    std::fs::remove_file(&fixture.output).unwrap();
    std::fs::hard_link(&protected, &fixture.output).unwrap();
    kind(fixture.write(&generated, false), ErrorKind::InvalidPath);
    std::fs::remove_file(&fixture.output).unwrap();
    std::fs::hard_link(&fixture.lock, &fixture.output).unwrap();
    kind(fixture.write(&generated, false), ErrorKind::InvalidPath);
    std::fs::remove_file(&fixture.output).unwrap();
    std::fs::create_dir(&fixture.output).unwrap();
    kind(fixture.write(&generated, false), ErrorKind::InvalidPath);
    std::fs::remove_dir(&fixture.output).unwrap();
    let socket = UnixListener::bind(&fixture.output).unwrap();
    kind(fixture.write(&generated, false), ErrorKind::InvalidPath);
    drop(socket);
    std::fs::remove_file(&fixture.output).unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fixture.output)
            .status()
            .unwrap()
            .success()
    );
    kind(fixture.write(&generated, false), ErrorKind::InvalidPath);
    std::fs::remove_file(&fixture.output).unwrap();
    symlink(&fixture.root, fixture.root.join("parent-link")).unwrap();
    kind(
        write_generated_rust(
            &generated,
            &fixture.root.join("parent-link/child.rs"),
            &fixture.lock,
            &fixture.registry,
            false,
        ),
        ErrorKind::InvalidPath,
    );
    symlink(&fixture.lock, fixture.root.join("lock-link")).unwrap();
    kind(
        write_generated_rust(
            &generated,
            &fixture.output,
            &fixture.root.join("lock-link"),
            &fixture.registry,
            false,
        ),
        ErrorKind::InvalidPath,
    );
    kind(
        write_generated_rust(
            &generated,
            Path::new("/dev/null"),
            &fixture.lock,
            &fixture.registry,
            false,
        ),
        ErrorKind::InvalidPath,
    );
    assert_eq!(
        std::fs::read(&protected).unwrap(),
        b"protected original bytes"
    );
    assert!(!fixture.root.join("missing.rs").exists());
    assert!(!fixture.root.join("child.rs").exists());
    assert!(fixture.stages().is_empty());
}

#[cfg(unix)]
#[test]
fn injected_precommit_failures_preserve_absence_and_cleanup_real_stages() {
    for failure in [
        Phase::BeforeStageWrite,
        Phase::BeforeStageSync,
        Phase::BeforeReadback,
        Phase::BeforeCommit,
    ] {
        let fixture = Fixture::new();
        let generated = fixture.generated("first");
        let mut reached = false;
        let result = write_with(
            &generated,
            &fixture.output,
            &fixture.lock,
            &fixture.registry,
            false,
            &mut |at, _| {
                if at == failure {
                    reached = true;
                    return Err(Error::io(
                        "injected precommit IO failure",
                        io::Error::other("constructed test failure"),
                    ));
                }
                Ok(())
            },
        );
        kind(result, ErrorKind::Io);
        assert!(reached);
        assert!(!fixture.output.exists());
        assert!(fixture.stages().is_empty());
    }
}

#[cfg(unix)]
#[test]
fn actual_corrupt_readback_missing_stage_and_concurrent_different_file_do_not_commit() {
    for action in ["readback", "missing", "different", "identical"] {
        let fixture = Fixture::new();
        let generated = fixture.generated("first");
        let result = write_with(
            &generated,
            &fixture.output,
            &fixture.lock,
            &fixture.registry,
            false,
            &mut |at, path| {
                match (action, at) {
                    ("readback", Phase::BeforeReadback) => {
                        std::fs::write(path, b"corrupted source").unwrap()
                    }
                    ("missing", Phase::BeforeCommit) => std::fs::remove_file(path).unwrap(),
                    ("different", Phase::BeforeCommit) => {
                        std::fs::write(&fixture.output, b"other writer").unwrap()
                    }
                    ("identical", Phase::BeforeCommit) => {
                        std::fs::write(&fixture.output, generated.source()).unwrap()
                    }
                    _ => (),
                }
                Ok(())
            },
        );
        if action == "identical" {
            assert_eq!(result.unwrap().status, OutputStatus::AlreadyPresent);
            assert_eq!(
                std::fs::read(&fixture.output).unwrap(),
                generated.source().as_bytes()
            );
        } else {
            kind(
                result,
                if action == "missing" {
                    ErrorKind::Io
                } else {
                    ErrorKind::Conflict
                },
            );
            if action == "different" {
                assert_eq!(std::fs::read(&fixture.output).unwrap(), b"other writer");
            } else {
                assert!(!fixture.output.exists());
            }
        }
        assert!(fixture.stages().is_empty());
    }
}

#[cfg(unix)]
#[test]
fn postcommit_sync_and_cleanup_errors_leave_complete_source_and_allow_exact_retry() {
    for failure in [
        Phase::AfterCommit,
        Phase::BeforeStageRemove,
        Phase::BeforeParentSync,
    ] {
        let fixture = Fixture::new();
        let generated = fixture.generated("first");
        let mut reached = false;
        kind(
            write_with(
                &generated,
                &fixture.output,
                &fixture.lock,
                &fixture.registry,
                false,
                &mut |at, _| {
                    if at == failure {
                        reached = true;
                        return Err(Error::io(
                            "injected postcommit IO failure",
                            io::Error::other("complete file may be visible"),
                        ));
                    }
                    Ok(())
                },
            ),
            ErrorKind::Io,
        );
        assert!(reached);
        assert_eq!(
            std::fs::read(&fixture.output).unwrap(),
            generated.source().as_bytes()
        );
        assert!(fixture.stages().is_empty());
        assert_eq!(
            fixture.write(&generated, false).unwrap().status,
            OutputStatus::AlreadyPresent
        );
    }
    // Force a real remove_file error by replacing the stage entry with a
    // directory after publication. The destination still has complete bytes;
    // error cleanup cannot remove the directory and must not claim success.
    let fixture = Fixture::new();
    let generated = fixture.generated("first");
    kind(
        write_with(
            &generated,
            &fixture.output,
            &fixture.lock,
            &fixture.registry,
            false,
            &mut |at, path| {
                if at == Phase::BeforeStageRemove {
                    std::fs::remove_file(path).unwrap();
                    std::fs::create_dir(path).unwrap();
                }
                Ok(())
            },
        ),
        ErrorKind::Io,
    );
    assert_eq!(
        std::fs::read(&fixture.output).unwrap(),
        generated.source().as_bytes()
    );
    let stages = fixture.stages();
    assert_eq!(stages.len(), 1);
    std::fs::remove_dir(&stages[0]).unwrap();
    assert_eq!(
        fixture.write(&generated, false).unwrap().status,
        OutputStatus::AlreadyPresent
    );
}

#[cfg(unix)]
struct ChildWriter {
    child: std::process::Child,
    input: std::process::ChildStdin,
    messages: std::sync::mpsc::Receiver<String>,
}
#[cfg(unix)]
impl ChildWriter {
    fn start(fixture: &Fixture, alias: &str) -> Self {
        use std::io::BufRead;
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "codegen::output::tests::concurrent_child_entry",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("OXIDE_CODEGEN_WRITER_CHILD_ROOT", &fixture.root)
            .env("OXIDE_CODEGEN_WRITER_CHILD_ALIAS", alias)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, messages) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let input = child.stdin.take().unwrap();
        let mut writer = Self {
            child,
            input,
            messages,
        };
        writer.expect("CODEGEN_BEFORE_COMMIT");
        writer
    }
    fn expect(&mut self, expected: &str) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let line = self
                .messages
                .recv_timeout(remaining)
                .expect("bounded child writer barrier");
            if line.contains(expected) {
                return;
            }
        }
    }
    fn finish(mut self, expected: &str) {
        use std::io::Read;
        self.input.write_all(&[0]).unwrap();
        self.input.flush().unwrap();
        self.expect(expected);
        let status = self.child.wait().unwrap();
        let mut stderr = String::new();
        self.child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        assert!(status.success(), "child writer failed: {stderr}");
    }
}
#[cfg(unix)]
impl Drop for ChildWriter {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(unix)]
#[test]
fn actual_independent_processes_commit_without_replacing_identical_or_different_source() {
    for alias in ["first", "second"] {
        let fixture = Fixture::new();
        let generated = fixture.generated("first");
        let child = ChildWriter::start(&fixture, alias);
        assert!(
            !fixture.output.exists(),
            "child is paused before atomic publication"
        );
        assert_eq!(
            fixture.write(&generated, false).unwrap().status,
            OutputStatus::Created
        );
        child.finish(if alias == "first" {
            "CODEGEN_ALREADY_PRESENT"
        } else {
            "CODEGEN_CONFLICT"
        });
        assert_eq!(
            std::fs::read(&fixture.output).unwrap(),
            generated.source().as_bytes()
        );
        assert!(fixture.stages().is_empty());
    }
}

/// Inert without the explicit child environment; parent test uses pipe barriers.
#[cfg(unix)]
#[test]
fn concurrent_child_entry() {
    use std::io::Read;
    let Some(root) = std::env::var_os("OXIDE_CODEGEN_WRITER_CHILD_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let alias = std::env::var("OXIDE_CODEGEN_WRITER_CHILD_ALIAS").unwrap();
    let registry = Registry::open(&root.join("registry"), Limits::default()).unwrap();
    let lock = root.join("chialisp-oxide.lock");
    let checked = dependency::read_lock(&lock).unwrap();
    let generated =
        generate_locked_rust(&checked, &registry, &alias, &GenerationLimits::default()).unwrap();
    let result = write_with(
        &generated,
        &root.join("module.rs"),
        &lock,
        &registry,
        false,
        &mut |at, _| {
            if at == Phase::BeforeCommit {
                println!("CODEGEN_BEFORE_COMMIT");
                std::io::stdout().flush().unwrap();
                std::io::stdin().read_exact(&mut [0]).unwrap();
            }
            Ok(())
        },
    );
    if alias == "first" {
        assert_eq!(result.unwrap().status, OutputStatus::AlreadyPresent);
        println!("CODEGEN_ALREADY_PRESENT");
    } else {
        kind(result, ErrorKind::Conflict);
        println!("CODEGEN_CONFLICT");
    }
    std::io::stdout().flush().unwrap();
}

#[cfg(unix)]
#[test]
fn oversized_existing_source_is_bounded_before_read_and_is_never_replaced() {
    let fixture = Fixture::new();
    let generated = fixture.generated("first");
    let bytes = vec![b'x'; generated.record().limits.max_source_bytes + 1];
    std::fs::write(&fixture.output, &bytes).unwrap();
    for check in [false, true] {
        kind(fixture.write(&generated, check), ErrorKind::ResourceLimit);
        assert_eq!(std::fs::read(&fixture.output).unwrap(), bytes);
    }
    assert!(fixture.stages().is_empty());
}
