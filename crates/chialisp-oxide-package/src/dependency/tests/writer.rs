use super::*;
use crate::dependency::writer::{Phase, add_with};
use std::path::Path;

#[test]
fn stale_exact_snapshots_abort_new_changed_and_idempotent_updates() {
    for (present, name) in [(false, "new"), (true, "new"), (true, "old")] {
        let fixture = Fixture::new();
        let package = package();
        fixture.publish(&package);
        if present {
            add_local(
                &fixture.registry,
                &fixture.lock,
                "old",
                package.module_tree_hash(),
                None,
            )
            .unwrap();
        }
        let mut newer = if present {
            std::fs::read(&fixture.lock).unwrap()
        } else {
            checked(vec![]).bytes().to_vec()
        };
        newer.push(b' ');
        let mut changed = false;
        let mut hook = |phase, path: &Path| {
            if phase == Phase::AfterResolution {
                std::fs::write(path, &newer).unwrap();
                changed = true;
            }
            Ok(())
        };
        assert_kind(
            add_with(
                &fixture.registry,
                &fixture.lock,
                name,
                package.module_tree_hash(),
                None,
                &mut hook,
            ),
            ErrorKind::Conflict,
        );
        assert!(changed);
        assert_eq!(std::fs::read(&fixture.lock).unwrap(), newer);
        assert!(fixture.stage_files().is_empty());
    }
}

#[test]
fn active_cooperating_writer_returns_conflict_without_touching_destination() {
    let fixture = Fixture::new();
    let package = package();
    fixture.publish(&package);
    let sidecar = fixture.root.join(".chialisp-oxide.lock.write-lock");
    let held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&sidecar)
        .unwrap();
    held.lock().unwrap();
    assert_kind(
        add_local(
            &fixture.registry,
            &fixture.lock,
            "payment",
            package.module_tree_hash(),
            None,
        ),
        ErrorKind::Conflict,
    );
    assert!(!fixture.lock.exists());
    assert!(fixture.stage_files().is_empty());
    // End this external fixture's lock explicitly; a parallel process spawn
    // can briefly retain an inherited description after this handle closes.
    held.unlock().unwrap();
    drop(held);
    add_local(
        &fixture.registry,
        &fixture.lock,
        "payment",
        package.module_tree_hash(),
        None,
    )
    .unwrap();
}

struct WriterProcess {
    child: std::process::Child,
    input: std::process::ChildStdin,
    output: std::io::BufReader<std::process::ChildStdout>,
}
impl WriterProcess {
    fn start(root: &Path, action: &str) -> Self {
        use std::process::{Command, Stdio};
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "dependency::tests::writer::writer_child_entry",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("OXIDE_DEPENDENCY_WRITER_CHILD_ROOT", root)
            .env("OXIDE_DEPENDENCY_WRITER_CHILD_ACTION", action)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut process = Self {
            child,
            input,
            output,
        };
        process.expect("DEPENDENCY_WRITER_LOCKED");
        process
    }
    fn expect(&mut self, marker: &str) {
        use std::io::BufRead;
        for _ in 0..64 {
            let mut line = String::new();
            assert!(
                self.output.read_line(&mut line).unwrap() > 0,
                "child closed before {marker}"
            );
            if line.trim_end().ends_with(marker) {
                return;
            }
        }
        panic!("child did not produce bounded expected marker {marker}");
    }
    fn continue_once(&mut self) {
        use std::io::Write;
        self.input.write_all(b"x").unwrap();
        self.input.flush().unwrap();
    }
    fn finish(mut self) {
        use std::io::Read;
        self.continue_once();
        let status = self.child.wait().unwrap();
        let mut errors = String::new();
        self.child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut errors)
            .unwrap();
        assert!(status.success(), "child writer failed: {errors}");
    }
}
impl Drop for WriterProcess {
    fn drop(&mut self) {
        // A failed parent assertion must not strand a child at its stdin barrier.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn production_writer_releases_for_an_independent_process_before_child_exit() {
    for action in ["success", "error", "idempotent"] {
        let fixture = Fixture::new();
        let package = package();
        fixture.publish(&package);
        if action == "idempotent" {
            add_local(
                &fixture.registry,
                &fixture.lock,
                "payment",
                package.module_tree_hash(),
                None,
            )
            .unwrap();
        }
        let before = std::fs::read(&fixture.lock).ok();
        let mut process = WriterProcess::start(&fixture.root, action);
        assert_kind(
            add_local(
                &fixture.registry,
                &fixture.lock,
                "payment",
                package.module_tree_hash(),
                None,
            ),
            ErrorKind::Conflict,
        );
        assert_eq!(std::fs::read(&fixture.lock).ok(), before);
        process.continue_once();
        process.expect("DEPENDENCY_WRITER_RETURNED");
        // The child is still alive and waiting at its second pipe barrier.
        // Process exit therefore cannot provide this lock-release evidence.
        let outcome = add_local(
            &fixture.registry,
            &fixture.lock,
            "payment",
            package.module_tree_hash(),
            None,
        )
        .unwrap();
        assert_eq!(
            outcome.status,
            if action == "error" {
                AddStatus::Added
            } else {
                AddStatus::AlreadyPresent
            }
        );
        assert_eq!(
            read_lock(&fixture.lock).unwrap().dependencies(),
            &[pin("payment", &package)]
        );
        assert!(fixture.stage_files().is_empty());
        process.finish();
    }
}

/// Subprocess helper, inert during ordinary direct test discovery.
#[test]
fn writer_child_entry() {
    use std::io::{Read, Write};
    let Some(root) = std::env::var_os("OXIDE_DEPENDENCY_WRITER_CHILD_ROOT") else {
        return;
    };
    let action = std::env::var("OXIDE_DEPENDENCY_WRITER_CHILD_ACTION").unwrap();
    assert!(["success", "error", "idempotent"].contains(&action.as_str()));
    let root = PathBuf::from(root);
    let registry = Registry::open(&root.join("registry"), Limits::default()).unwrap();
    let package = package();
    let mut hook = |phase, _: &Path| {
        if phase == Phase::AfterWriterLock {
            println!("DEPENDENCY_WRITER_LOCKED");
            std::io::stdout().flush().unwrap();
            std::io::stdin().read_exact(&mut [0]).unwrap();
            if action == "error" {
                return Err(Error::io(
                    "injected child body failure",
                    std::io::Error::other("fail with production guard held"),
                ));
            }
        }
        Ok(())
    };
    let outcome = add_with(
        &registry,
        &root.join("chialisp-oxide.lock"),
        "payment",
        package.module_tree_hash(),
        None,
        &mut hook,
    );
    if action == "error" {
        assert_kind(outcome, ErrorKind::Io);
    } else {
        assert_eq!(
            outcome.unwrap().status,
            if action == "success" {
                AddStatus::Added
            } else {
                AddStatus::AlreadyPresent
            }
        );
    }
    println!("DEPENDENCY_WRITER_RETURNED");
    std::io::stdout().flush().unwrap();
    std::io::stdin().read_exact(&mut [0]).unwrap();
}

#[test]
fn nonempty_sidecar_cannot_double_as_another_consumer_lock() {
    for existing in [false, true] {
        let fixture = Fixture::new();
        let package = package();
        fixture.publish(&package);
        if existing {
            add_local(
                &fixture.registry,
                &fixture.lock,
                "payment",
                package.module_tree_hash(),
                None,
            )
            .unwrap();
        }
        let before = std::fs::read(&fixture.lock).ok();
        let sidecar = fixture.root.join(".chialisp-oxide.lock.write-lock");
        let different = checked(vec![pin("another", &package)]);
        std::fs::write(&sidecar, different.bytes()).unwrap();
        assert_kind(
            add_local(
                &fixture.registry,
                &fixture.lock,
                "payment",
                package.module_tree_hash(),
                None,
            ),
            ErrorKind::InvalidPath,
        );
        assert_eq!(std::fs::read(&fixture.lock).ok(), before);
        assert_eq!(std::fs::read(&sidecar).unwrap(), different.bytes());
        assert!(fixture.stage_files().is_empty());
    }
}

#[test]
fn injected_stage_failures_preserve_existing_bytes_or_absence_and_cleanup() {
    for existing in [false, true] {
        for failure in [
            Phase::BeforeStageWrite,
            Phase::BeforeStageSync,
            Phase::BeforeCommit,
        ] {
            let fixture = Fixture::new();
            let package = package();
            fixture.publish(&package);
            if existing {
                add_local(
                    &fixture.registry,
                    &fixture.lock,
                    "old",
                    package.module_tree_hash(),
                    None,
                )
                .unwrap();
            }
            let before = std::fs::read(&fixture.lock).ok();
            let mut reached = false;
            let mut hook = |phase, _: &Path| {
                if phase == failure {
                    reached = true;
                    return Err(Error::io(
                        "injected precommit failure",
                        std::io::Error::other("constructed failure"),
                    ));
                }
                Ok(())
            };
            assert_kind(
                add_with(
                    &fixture.registry,
                    &fixture.lock,
                    "new",
                    package.module_tree_hash(),
                    None,
                    &mut hook,
                ),
                ErrorKind::Io,
            );
            assert!(reached);
            assert_eq!(std::fs::read(&fixture.lock).ok(), before);
            assert!(fixture.stage_files().is_empty());
        }
    }
}

#[test]
fn staged_readback_and_actual_missing_stage_commit_failure_preserve_old_lock() {
    for replace_with_valid_lock in [false, true] {
        let fixture = Fixture::new();
        let package = package();
        fixture.publish(&package);
        add_local(
            &fixture.registry,
            &fixture.lock,
            "old",
            package.module_tree_hash(),
            None,
        )
        .unwrap();
        let before = std::fs::read(&fixture.lock).unwrap();
        let mut hook = |phase, path: &Path| {
            if phase == Phase::BeforeStageSync {
                if replace_with_valid_lock {
                    std::fs::write(path, checked(vec![]).bytes()).unwrap();
                } else {
                    std::fs::write(path, b"{}").unwrap();
                }
            }
            Ok(())
        };
        let expected = if replace_with_valid_lock {
            ErrorKind::Conflict
        } else {
            ErrorKind::InvalidManifest
        };
        assert_kind(
            add_with(
                &fixture.registry,
                &fixture.lock,
                "new",
                package.module_tree_hash(),
                None,
                &mut hook,
            ),
            expected,
        );
        assert_eq!(std::fs::read(&fixture.lock).unwrap(), before);
        assert!(fixture.stage_files().is_empty());
    }
    for existing in [false, true] {
        let fixture = Fixture::new();
        let package = package();
        fixture.publish(&package);
        if existing {
            add_local(
                &fixture.registry,
                &fixture.lock,
                "old",
                package.module_tree_hash(),
                None,
            )
            .unwrap();
        }
        let before = std::fs::read(&fixture.lock).ok();
        let mut hook = |phase, path: &Path| {
            if phase == Phase::BeforeCommit {
                std::fs::remove_file(path).unwrap();
            }
            Ok(())
        };
        assert_kind(
            add_with(
                &fixture.registry,
                &fixture.lock,
                "new",
                package.module_tree_hash(),
                None,
                &mut hook,
            ),
            ErrorKind::Io,
        );
        assert_eq!(std::fs::read(&fixture.lock).ok(), before);
        assert!(fixture.stage_files().is_empty());
    }
}

#[test]
fn second_snapshot_check_catches_edits_during_staging_and_postcommit_errors_are_explicit() {
    let fixture = Fixture::new();
    let package = package();
    fixture.publish(&package);
    add_local(
        &fixture.registry,
        &fixture.lock,
        "old",
        package.module_tree_hash(),
        None,
    )
    .unwrap();
    let mut newer = std::fs::read(&fixture.lock).unwrap();
    newer.push(b' ');
    let mut hook = |phase, _: &Path| {
        if phase == Phase::BeforeCommit {
            std::fs::write(&fixture.lock, &newer).unwrap();
        }
        Ok(())
    };
    assert_kind(
        add_with(
            &fixture.registry,
            &fixture.lock,
            "new",
            package.module_tree_hash(),
            None,
            &mut hook,
        ),
        ErrorKind::Conflict,
    );
    assert_eq!(std::fs::read(&fixture.lock).unwrap(), newer);
    assert!(fixture.stage_files().is_empty());
    let mut hook = |phase, _: &Path| {
        if phase == Phase::AfterCommit {
            return Err(Error::io(
                "injected postcommit observation",
                std::io::Error::other("complete new lock already committed"),
            ));
        }
        Ok(())
    };
    assert_kind(
        add_with(
            &fixture.registry,
            &fixture.lock,
            "new",
            package.module_tree_hash(),
            None,
            &mut hook,
        ),
        ErrorKind::Io,
    );
    let committed = read_lock(&fixture.lock).unwrap();
    assert_eq!(committed.dependencies().len(), 2);
    assert!(fixture.stage_files().is_empty());
    assert_eq!(
        add_local(
            &fixture.registry,
            &fixture.lock,
            "new",
            package.module_tree_hash(),
            None
        )
        .unwrap()
        .status,
        AddStatus::AlreadyPresent
    );
}

#[test]
fn reserved_registry_and_nonregular_destinations_fail_before_writing() {
    let fixture = Fixture::new();
    let package = package();
    fixture.publish(&package);
    for path in [
        fixture.root.join("Cargo.toml"),
        fixture.root.join("cArGo.LoCk"),
        fixture.registry.root().join("new.lock"),
        fixture.registry.root().join("objects/new.lock"),
    ] {
        assert_kind(
            add_local(
                &fixture.registry,
                &path,
                "payment",
                package.module_tree_hash(),
                None,
            ),
            ErrorKind::InvalidPath,
        );
        assert!(!path.exists());
    }
    let directory = fixture.root.join("directory.lock");
    std::fs::create_dir(&directory).unwrap();
    assert_kind(
        add_local(
            &fixture.registry,
            &directory,
            "payment",
            package.module_tree_hash(),
            None,
        ),
        ErrorKind::InvalidPath,
    );
    let sidecar = fixture.root.join(".chialisp-oxide.lock.write-lock");
    std::fs::create_dir(&sidecar).unwrap();
    assert_kind(
        add_local(
            &fixture.registry,
            &fixture.lock,
            "payment",
            package.module_tree_hash(),
            None,
        ),
        ErrorKind::InvalidPath,
    );
    assert!(!fixture.lock.exists());
}

#[test]
fn unicode_kelvin_case_alias_cannot_create_reserved_cargo_lock() {
    let fixture = Fixture::new();
    let package = package();
    fixture.publish(&package);
    let alias = fixture.root.join("Cargo.loc\u{212a}");
    let reserved = fixture.root.join("Cargo.lock");
    assert!(!alias.exists());
    assert!(!reserved.exists());
    let inventory = || -> std::collections::BTreeSet<_> {
        std::fs::read_dir(&fixture.root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect()
    };
    let before = inventory();
    assert_kind(
        add_local(
            &fixture.registry,
            &alias,
            "payment",
            package.module_tree_hash(),
            None,
        ),
        ErrorKind::InvalidPath,
    );
    // On case-insensitive APFS these names identify the same absent leaf.
    // Case-sensitive hosts conservatively reserve the Unicode variant too.
    assert!(!alias.exists());
    assert!(!reserved.exists());
    assert_eq!(
        inventory(),
        before,
        "no destination, sidecar or stage may be created"
    );
}

#[test]
fn actual_fifo_and_socket_destinations_and_sidecars_are_never_opened() {
    use std::os::unix::{
        fs::{FileTypeExt, MetadataExt},
        net::UnixListener,
    };
    for socket in [false, true] {
        for sidecar in [false, true] {
            let fixture = Fixture::new();
            let package = package();
            fixture.publish(&package);
            let leaf = if sidecar {
                fixture.root.join(".chialisp-oxide.lock.write-lock")
            } else {
                fixture.lock.clone()
            };
            let _listener = if socket {
                Some(UnixListener::bind(&leaf).unwrap())
            } else {
                assert!(
                    std::process::Command::new("mkfifo")
                        .arg(&leaf)
                        .status()
                        .unwrap()
                        .success(),
                    "create actual FIFO fixture"
                );
                None
            };
            let before = std::fs::symlink_metadata(&leaf).unwrap();
            assert!(if socket {
                before.file_type().is_socket()
            } else {
                before.file_type().is_fifo()
            });
            // No FIFO writer exists. Opening either leaf before inspecting its
            // kind would block rather than return this exact typed error.
            assert_kind(
                add_local(
                    &fixture.registry,
                    &fixture.lock,
                    "payment",
                    package.module_tree_hash(),
                    None,
                ),
                ErrorKind::InvalidPath,
            );
            let after = std::fs::symlink_metadata(&leaf).unwrap();
            assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
            if sidecar {
                assert!(!fixture.lock.exists());
            }
            assert!(fixture.stage_files().is_empty());
        }
    }
}

#[cfg(unix)]
#[test]
fn symlink_and_hardlink_destinations_sidecars_and_parent_aliases_are_rejected() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let package = package();
    fixture.publish(&package);
    let source = fixture.root.join("Cargo.lock");
    let original = checked(vec![]);
    std::fs::write(&source, original.bytes()).unwrap();
    for hardlink in [false, true] {
        if hardlink {
            std::fs::hard_link(&source, &fixture.lock).unwrap();
        } else {
            symlink(&source, &fixture.lock).unwrap();
        }
        assert_kind(
            add_local(
                &fixture.registry,
                &fixture.lock,
                "payment",
                package.module_tree_hash(),
                None,
            ),
            ErrorKind::InvalidPath,
        );
        assert_eq!(std::fs::read(&source).unwrap(), original.bytes());
        std::fs::remove_file(&fixture.lock).unwrap();
        let sidecar = fixture.root.join(".chialisp-oxide.lock.write-lock");
        if hardlink {
            std::fs::hard_link(&source, &sidecar).unwrap();
        } else {
            symlink(&source, &sidecar).unwrap();
        }
        assert_kind(
            add_local(
                &fixture.registry,
                &fixture.lock,
                "payment",
                package.module_tree_hash(),
                None,
            ),
            ErrorKind::InvalidPath,
        );
        std::fs::remove_file(sidecar).unwrap();
    }
    let parent = fixture.root.join("parent-alias");
    symlink(&fixture.root, &parent).unwrap();
    assert_kind(
        add_local(
            &fixture.registry,
            &parent.join("new.lock"),
            "payment",
            package.module_tree_hash(),
            None,
        ),
        ErrorKind::InvalidPath,
    );
    assert!(!fixture.lock.exists());
    assert!(fixture.stage_files().is_empty());
}

#[cfg(unix)]
#[test]
fn case_insensitive_registry_alias_is_checked_by_directory_identity() {
    use std::os::unix::fs::MetadataExt;
    let fixture = Fixture::new();
    let package = package();
    fixture.publish(&package);
    let alias = fixture.root.join("REGISTRY");
    if alias.exists() {
        let a = std::fs::metadata(&alias).unwrap();
        let b = std::fs::metadata(fixture.registry.root()).unwrap();
        assert_eq!((a.dev(), a.ino()), (b.dev(), b.ino()));
        let before: std::collections::BTreeSet<_> =
            std::fs::read_dir(fixture.registry.root().join("objects"))
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .collect();
        let destination = alias.join("objects/consumer.lock");
        assert_kind(
            add_local(
                &fixture.registry,
                &destination,
                "payment",
                package.module_tree_hash(),
                None,
            ),
            ErrorKind::InvalidPath,
        );
        assert!(!destination.exists());
        let after: std::collections::BTreeSet<_> =
            std::fs::read_dir(fixture.registry.root().join("objects"))
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .collect();
        assert_eq!(before, after);
    }
}
