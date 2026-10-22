//! The same filesystem contract through both concrete immutable entrypoints.
use super::*;
use crate::codegen::witness::{
    GeneratedWitnessRust, OutputIntegrityFault, WitnessGenerationLimits,
    generate_locked_witness_rust,
};

enum Module {
    Static(GeneratedRust),
    Witness(GeneratedWitnessRust),
}
impl Module {
    fn new(fixture: &Fixture, alias: &str, witness: bool) -> Self {
        if witness {
            let lock = dependency::read_lock(&fixture.lock).unwrap();
            Self::Witness(
                generate_locked_witness_rust(
                    &lock,
                    &fixture.registry,
                    alias,
                    &WitnessGenerationLimits::default(),
                )
                .unwrap(),
            )
        } else {
            Self::Static(fixture.generated(alias))
        }
    }
    fn source(&self) -> &str {
        match self {
            Self::Static(value) => value.source(),
            Self::Witness(value) => value.source(),
        }
    }
    fn limit(&self) -> usize {
        match self {
            Self::Static(value) => value.record().limits.max_source_bytes,
            Self::Witness(value) => value.record().limits.max_source_bytes,
        }
    }
    fn write(
        &self,
        fixture: &Fixture,
        output: &Path,
        lock: &Path,
        check: bool,
    ) -> Result<OutputRecord> {
        match self {
            Self::Static(value) => {
                write_generated_rust(value, output, lock, &fixture.registry, check)
            }
            Self::Witness(value) => {
                write_generated_witness_rust(value, output, lock, &fixture.registry, check)
            }
        }
    }
    #[cfg(unix)]
    fn hook(
        &self,
        fixture: &Fixture,
        hook: &mut impl FnMut(Phase, &Path) -> Result<()>,
    ) -> Result<OutputRecord> {
        match self {
            Self::Static(value) => write_with(
                value,
                &fixture.output,
                &fixture.lock,
                &fixture.registry,
                false,
                hook,
            ),
            Self::Witness(value) => write_witness_with(
                value,
                &fixture.output,
                &fixture.lock,
                &fixture.registry,
                false,
                hook,
            ),
        }
    }
    fn corrupt(self, fault: OutputIntegrityFault) -> Self {
        match self {
            Self::Witness(value) => Self::Witness(value.with_output_fault(fault)),
            Self::Static(mut value) => {
                match fault {
                    OutputIntegrityFault::Text => value.source.push(' '),
                    OutputIntegrityFault::Hash => {
                        value.record.source_sha256 = Digest32::new([0; 32])
                    }
                    OutputIntegrityFault::Length => value.record.source_bytes += 1,
                    OutputIntegrityFault::Limit => {
                        value.record.limits.max_source_bytes = value.source.len() - 1
                    }
                }
                Self::Static(value)
            }
        }
    }
}

#[test]
fn both_entrypoints_check_readonly_and_reject_forged_internal_source_records() {
    for witness in [false, true] {
        let fixture = Fixture::new();
        let generated = Module::new(&fixture, "first", witness);
        std::fs::write(&fixture.output, generated.source()).unwrap();
        let before = std::fs::metadata(&fixture.output)
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(
            generated
                .write(&fixture, &fixture.output, &fixture.lock, true)
                .unwrap()
                .status,
            OutputStatus::Checked
        );
        assert_eq!(
            std::fs::metadata(&fixture.output)
                .unwrap()
                .modified()
                .unwrap(),
            before
        );
        kind(
            generated.write(
                &fixture,
                &fixture.root.join("missing/module.rs"),
                &fixture.lock,
                true,
            ),
            ErrorKind::Io,
        );
        assert!(!fixture.root.join("missing").exists());
        for fault in [
            OutputIntegrityFault::Text,
            OutputIntegrityFault::Hash,
            OutputIntegrityFault::Length,
            OutputIntegrityFault::Limit,
        ] {
            let invalid = Module::new(&fixture, "first", witness).corrupt(fault);
            kind(
                invalid.write(&fixture, &fixture.output, &fixture.lock, true),
                ErrorKind::DigestMismatch,
            );
            assert_eq!(
                std::fs::read(&fixture.output).unwrap(),
                generated.source().as_bytes()
            );
        }
        let oversized = vec![b'x'; generated.limit() + 1];
        std::fs::write(&fixture.output, &oversized).unwrap();
        kind(
            generated.write(&fixture, &fixture.output, &fixture.lock, true),
            ErrorKind::ResourceLimit,
        );
        assert_eq!(std::fs::read(&fixture.output).unwrap(), oversized);
        assert!(fixture.stages().is_empty());
    }
}

#[cfg(unix)]
#[test]
fn both_entrypoints_preserve_new_identical_conflicting_and_permission_contracts() {
    use std::os::unix::fs::PermissionsExt;
    for witness in [false, true] {
        let fixture = Fixture::new();
        let generated = Module::new(&fixture, "first", witness);
        let lock = std::fs::read(&fixture.lock).unwrap();
        kind(
            generated.write(&fixture, &fixture.output, &fixture.lock, true),
            ErrorKind::Conflict,
        );
        let result = generated
            .write(&fixture, &fixture.output, &fixture.lock, false)
            .unwrap();
        assert_eq!(result.status, OutputStatus::Created);
        assert_eq!(result.source_sha256, digest(generated.source().as_bytes()));
        assert_eq!(result.source_bytes, generated.source().len() as u64);
        assert_eq!(
            std::fs::metadata(&fixture.output)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            generated
                .write(&fixture, &fixture.output, &fixture.lock, false)
                .unwrap()
                .status,
            OutputStatus::AlreadyPresent
        );
        for check in [false, true] {
            let different = Module::new(&fixture, "second", witness);
            kind(
                different.write(&fixture, &fixture.output, &fixture.lock, check),
                ErrorKind::Conflict,
            );
        }
        assert_eq!(
            std::fs::read(&fixture.output).unwrap(),
            generated.source().as_bytes()
        );
        assert_eq!(std::fs::read(&fixture.lock).unwrap(), lock);
        assert!(fixture.stages().is_empty());
    }
}

#[cfg(unix)]
#[test]
fn both_entrypoints_check_stale_exact_lock_before_staging_and_commit() {
    for witness in [false, true] {
        for at_commit in [false, true] {
            let fixture = Fixture::new();
            let generated = Module::new(&fixture, "first", witness);
            let change = || {
                let mut bytes = std::fs::read(&fixture.lock).unwrap();
                bytes.push(b' ');
                assert!(verify_lock(&bytes).is_ok());
                std::fs::write(&fixture.lock, bytes).unwrap();
            };
            if !at_commit {
                change();
            }
            let mut reached = false;
            kind(
                generated.hook(&fixture, &mut |at, _| {
                    if at_commit && at == Phase::BeforeCommit {
                        change();
                        reached = true;
                    }
                    Ok(())
                }),
                ErrorKind::Conflict,
            );
            assert_eq!(reached, at_commit);
            assert!(!fixture.output.exists());
            assert!(fixture.stages().is_empty());
        }
    }
}

#[cfg(unix)]
#[test]
fn both_entrypoints_protect_cargo_unicode_lock_registry_and_os_path_aliases() {
    use std::os::unix::ffi::OsStringExt;
    for witness in [false, true] {
        let fixture = Fixture::new();
        let generated = Module::new(&fixture, "first", witness);
        let before = std::fs::read(&fixture.lock).unwrap();
        std::fs::create_dir(fixture.registry.root().join("nested")).unwrap();
        for output in [
            fixture.lock.clone(),
            fixture.root.join("Cargo.toml"),
            fixture.root.join("Cargo.lock"),
            fixture.root.join("CARGO.LOCK"),
            fixture.root.join("Cargo.locK"),
            fixture.registry.root().join("module.rs"),
            fixture.registry.root().join("nested/module.rs"),
        ] {
            kind(
                generated.write(&fixture, &output, &fixture.lock, false),
                ErrorKind::InvalidPath,
            );
        }
        #[cfg(target_os = "macos")]
        if fixture.root.join("REGISTRY").exists() {
            kind(
                generated.write(
                    &fixture,
                    &fixture.root.join("REGISTRY/module.rs"),
                    &fixture.lock,
                    false,
                ),
                ErrorKind::InvalidPath,
            );
        }
        assert_eq!(std::fs::read(&fixture.lock).unwrap(), before);
        let output = fixture
            .root
            .join(OsString::from_vec(b"module-\xff.rs".to_vec()));
        match generated.write(&fixture, &output, &fixture.lock, false) {
            Ok(result) => {
                assert_eq!(result.status, OutputStatus::Created);
                assert_eq!(
                    generated
                        .write(&fixture, &output, &fixture.lock, true)
                        .unwrap()
                        .status,
                    OutputStatus::Checked
                );
            }
            Err(error) => assert_eq!(
                error.kind(),
                ErrorKind::Io,
                "filesystem path support remains an I/O result"
            ),
        }
        assert!(fixture.stages().is_empty());
    }
}

#[cfg(unix)]
#[test]
fn both_entrypoints_reject_symlinks_hardlinks_directories_sockets_and_fifos() {
    use std::os::unix::{fs::symlink, net::UnixListener};
    for witness in [false, true] {
        let fixture = Fixture::new();
        let generated = Module::new(&fixture, "first", witness);
        let protected = fixture.root.join("protected.rs");
        std::fs::write(&protected, b"unchanged original").unwrap();
        for target in [&protected, &fixture.root.join("absent.rs")] {
            symlink(target, &fixture.output).unwrap();
            kind(
                generated.write(&fixture, &fixture.output, &fixture.lock, false),
                ErrorKind::InvalidPath,
            );
            std::fs::remove_file(&fixture.output).unwrap();
        }
        for target in [&protected, &fixture.lock] {
            std::fs::hard_link(target, &fixture.output).unwrap();
            kind(
                generated.write(&fixture, &fixture.output, &fixture.lock, false),
                ErrorKind::InvalidPath,
            );
            std::fs::remove_file(&fixture.output).unwrap();
        }
        std::fs::create_dir(&fixture.output).unwrap();
        kind(
            generated.write(&fixture, &fixture.output, &fixture.lock, false),
            ErrorKind::InvalidPath,
        );
        std::fs::remove_dir(&fixture.output).unwrap();
        let listener = UnixListener::bind(&fixture.output).unwrap();
        kind(
            generated.write(&fixture, &fixture.output, &fixture.lock, false),
            ErrorKind::InvalidPath,
        );
        drop(listener);
        std::fs::remove_file(&fixture.output).unwrap();
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fixture.output)
                .status()
                .unwrap()
                .success()
        );
        kind(
            generated.write(&fixture, &fixture.output, &fixture.lock, false),
            ErrorKind::InvalidPath,
        );
        std::fs::remove_file(&fixture.output).unwrap();
        symlink(&fixture.root, fixture.root.join("linked-parent")).unwrap();
        kind(
            generated.write(
                &fixture,
                &fixture.root.join("linked-parent/child.rs"),
                &fixture.lock,
                false,
            ),
            ErrorKind::InvalidPath,
        );
        symlink(&fixture.lock, fixture.root.join("linked-lock")).unwrap();
        kind(
            generated.write(
                &fixture,
                &fixture.output,
                &fixture.root.join("linked-lock"),
                false,
            ),
            ErrorKind::InvalidPath,
        );
        kind(
            generated.write(&fixture, Path::new("/dev/null"), &fixture.lock, false),
            ErrorKind::InvalidPath,
        );
        assert_eq!(std::fs::read(&protected).unwrap(), b"unchanged original");
        assert!(!fixture.root.join("absent.rs").exists());
        assert!(!fixture.root.join("child.rs").exists());
        assert!(fixture.stages().is_empty());
    }
}

#[cfg(unix)]
#[test]
fn both_entrypoints_preserve_absence_on_injected_precommit_failures() {
    for witness in [false, true] {
        for failure in [
            Phase::BeforeStageWrite,
            Phase::BeforeStageSync,
            Phase::BeforeReadback,
            Phase::BeforeCommit,
        ] {
            let fixture = Fixture::new();
            let generated = Module::new(&fixture, "first", witness);
            let mut reached = false;
            kind(
                generated.hook(&fixture, &mut |at, _| {
                    if at == failure {
                        reached = true;
                        return Err(Error::io(
                            "constructed precommit failure",
                            io::Error::other("test control"),
                        ));
                    }
                    Ok(())
                }),
                ErrorKind::Io,
            );
            assert!(reached);
            assert!(!fixture.output.exists());
            assert!(fixture.stages().is_empty());
        }
    }
}

#[cfg(unix)]
#[test]
fn both_entrypoints_check_actual_readback_stage_removal_and_concurrent_winners() {
    for witness in [false, true] {
        for action in ["readback", "missing", "different", "identical"] {
            let fixture = Fixture::new();
            let generated = Module::new(&fixture, "first", witness);
            let result = generated.hook(&fixture, &mut |at, path| {
                match (action, at) {
                    ("readback", Phase::BeforeReadback) => {
                        std::fs::write(path, b"corrupted source").unwrap()
                    }
                    ("missing", Phase::BeforeCommit) => std::fs::remove_file(path).unwrap(),
                    ("different", Phase::BeforeCommit) => {
                        std::fs::write(&fixture.output, b"other source").unwrap()
                    }
                    ("identical", Phase::BeforeCommit) => {
                        std::fs::write(&fixture.output, generated.source()).unwrap()
                    }
                    _ => (),
                }
                Ok(())
            });
            if action == "identical" {
                assert_eq!(result.unwrap().status, OutputStatus::AlreadyPresent);
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
                    assert_eq!(std::fs::read(&fixture.output).unwrap(), b"other source");
                } else {
                    assert!(!fixture.output.exists());
                }
            }
            assert!(fixture.stages().is_empty());
        }
    }
}

#[cfg(unix)]
#[test]
fn both_entrypoints_report_postcommit_sync_cleanup_and_orphan_errors_exactly() {
    for witness in [false, true] {
        for failure in [
            Phase::AfterCommit,
            Phase::BeforeStageRemove,
            Phase::BeforeParentSync,
        ] {
            let fixture = Fixture::new();
            let generated = Module::new(&fixture, "first", witness);
            let mut reached = false;
            kind(
                generated.hook(&fixture, &mut |at, _| {
                    if at == failure {
                        reached = true;
                        return Err(Error::io(
                            "constructed postcommit failure",
                            io::Error::other("complete source may be visible"),
                        ));
                    }
                    Ok(())
                }),
                ErrorKind::Io,
            );
            assert!(reached);
            assert_eq!(
                std::fs::read(&fixture.output).unwrap(),
                generated.source().as_bytes()
            );
            assert!(fixture.stages().is_empty());
            assert_eq!(
                generated
                    .write(&fixture, &fixture.output, &fixture.lock, false)
                    .unwrap()
                    .status,
                OutputStatus::AlreadyPresent
            );
        }
        let fixture = Fixture::new();
        let generated = Module::new(&fixture, "first", witness);
        kind(
            generated.hook(&fixture, &mut |at, path| {
                if at == Phase::BeforeStageRemove {
                    std::fs::remove_file(path).unwrap();
                    std::fs::create_dir(path).unwrap();
                }
                Ok(())
            }),
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
            generated
                .write(&fixture, &fixture.output, &fixture.lock, false)
                .unwrap()
                .status,
            OutputStatus::AlreadyPresent
        );
    }
}

#[cfg(unix)]
fn child(fixture: &Fixture, alias: &str) -> ChildWriter {
    use std::io::BufRead;
    let mut process = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "codegen::output::tests::witness::concurrent_witness_child_entry",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("OXIDE_WITNESS_WRITER_CHILD_ROOT", &fixture.root)
        .env("OXIDE_WITNESS_WRITER_CHILD_ALIAS", alias)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let output = process.stdout.take().unwrap();
    let (sender, messages) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(output).lines() {
            let Ok(line) = line else {
                break;
            };
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let input = process.stdin.take().unwrap();
    let mut child = ChildWriter {
        child: process,
        input,
        messages,
    };
    child.expect("CODEGEN_BEFORE_COMMIT");
    child
}

#[cfg(unix)]
#[test]
fn actual_witness_processes_never_replace_identical_or_conflicting_source() {
    for alias in ["first", "second"] {
        let fixture = Fixture::new();
        let generated = Module::new(&fixture, "first", true);
        let process = child(&fixture, alias);
        assert!(!fixture.output.exists());
        assert_eq!(
            generated
                .write(&fixture, &fixture.output, &fixture.lock, false)
                .unwrap()
                .status,
            OutputStatus::Created
        );
        process.finish(if alias == "first" {
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

#[cfg(unix)]
#[test]
fn concurrent_witness_child_entry() {
    use std::io::Read;
    let Some(root) = std::env::var_os("OXIDE_WITNESS_WRITER_CHILD_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let registry = Registry::open(&root.join("registry"), Limits::default()).unwrap();
    let lock = root.join("chialisp-oxide.lock");
    let alias = std::env::var("OXIDE_WITNESS_WRITER_CHILD_ALIAS").unwrap();
    let generated = generate_locked_witness_rust(
        &dependency::read_lock(&lock).unwrap(),
        &registry,
        &alias,
        &WitnessGenerationLimits::default(),
    )
    .unwrap();
    let result = write_witness_with(
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

#[cfg(not(unix))]
#[test]
fn witness_writes_fail_before_mutation_on_unsupported_hosts() {
    let fixture = Fixture::new();
    let generated = Module::new(&fixture, "first", true);
    kind(
        generated.write(&fixture, &fixture.output, &fixture.lock, false),
        ErrorKind::Io,
    );
    assert!(!fixture.output.exists());
    assert!(fixture.stages().is_empty());
}
