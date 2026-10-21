use super::*;
use std::{
    io::{BufRead, BufReader, Read},
    process::{Child, ChildStdin, Command, Stdio},
};

fn fixture_bytes() -> Vec<u8> {
    br#"{"schema_version":1,"kind":"clvm_artifact","artifact":{"identity":{"encoding":"clvm-legacy-plain/v1","bytes":1,"blob_sha256":"76be8b528d0075f7aae98d6fa57a6d3c83ae480a8469e668d7b0af968995ac71","module_tree_hash":"4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"},"clvm_hex":"80"},"rust":null,"source_scope":"not_recorded","sources":[],"provenance":null}"#.to_vec()
}

fn package() -> VerifiedPackage {
    verify_package(&fixture_bytes(), &Limits::default()).unwrap()
}

fn fixture() -> (tempfile::TempDir, PathBuf, Registry) {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap().join("registry");
    let registry = Registry::create(&root, Limits::default()).unwrap();
    (temporary, root, registry)
}

fn artifact_package(blob: &[u8]) -> VerifiedPackage {
    let artifact = crate::artifact::inspect_artifact(
        blob,
        crate::artifact::Encoding::Plain,
        &Limits::default(),
    )
    .unwrap();
    let mut manifest: serde_json::Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    manifest["artifact"]["identity"] = serde_json::to_value(artifact.identity()).unwrap();
    manifest["artifact"]["clvm_hex"] = hex::encode(blob).into();
    verify_package(&serde_json::to_vec(&manifest).unwrap(), &Limits::default()).unwrap()
}

fn declared_variant(package: &VerifiedPackage) -> VerifiedPackage {
    let mut manifest: serde_json::Value = serde_json::from_slice(package.bytes()).unwrap();
    manifest["provenance"] = serde_json::json!({
        "label": "independent local declaration",
        "source_url": null,
        "revision": null,
        "license": null
    });
    verify_package(&serde_json::to_vec(&manifest).unwrap(), &Limits::default()).unwrap()
}

#[test]
fn dependency_scan_retains_the_exact_union_once_and_leaves_missing_ids_to_caller() {
    let (_temporary, root, registry) = fixture();
    assert_eq!(registry.root(), root);
    let first = package();
    let variant = declared_variant(&first);
    let requested_other = artifact_package(&[1]);
    let unrelated = artifact_package(&[2]);
    assert_ne!(first.object_id(), variant.object_id());
    assert_eq!(first.artifact().identity(), variant.artifact().identity());
    for package in [&first, &variant, &requested_other, &unrelated] {
        registry.publish(package).unwrap();
    }
    let missing = Digest32::new([9; 32]);
    let requested = BTreeSet::from([variant.object_id(), requested_other.object_id(), missing]);
    let matches = registry
        .scan_for_dependencies(Some(first.module_tree_hash()), &requested)
        .unwrap();
    let mut expected = vec![
        first.object_id(),
        variant.object_id(),
        requested_other.object_id(),
    ];
    expected.sort();
    assert_eq!(
        matches
            .iter()
            .map(VerifiedPackage::object_id)
            .collect::<Vec<_>>(),
        expected
    );
    assert!(matches.iter().all(|package| package.object_id() != missing));
    let retrieved_variant = matches
        .iter()
        .find(|package| package.object_id() == variant.object_id())
        .unwrap();
    assert_eq!(retrieved_variant.bytes(), variant.bytes());
    assert_eq!(
        registry
            .scan_for_dependencies(None, &BTreeSet::from([requested_other.object_id()]))
            .unwrap()
            .iter()
            .map(VerifiedPackage::object_id)
            .collect::<Vec<_>>(),
        vec![requested_other.object_id()]
    );
    assert!(
        registry
            .scan_for_dependencies(None, &BTreeSet::new())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn dependency_scan_charges_every_object_against_one_aggregate_budget() {
    let (_temporary, root, registry) = fixture();
    let first = package();
    let unrelated = artifact_package(&[1]);
    registry.publish(&first).unwrap();
    registry.publish(&unrelated).unwrap();
    let requested = BTreeSet::from([first.object_id()]);
    let total_bytes = first.bytes().len() + unrelated.bytes().len();
    let max_envelope_bytes = first.bytes().len().max(unrelated.bytes().len());
    let limits = Limits {
        max_objects: 2,
        max_lookup_bytes: total_bytes,
        max_envelope_bytes,
        ..Limits::default()
    };
    let exact = Registry::open(&root, limits.clone()).unwrap();
    assert_eq!(
        exact
            .scan_for_dependencies(None, &requested)
            .unwrap()
            .iter()
            .map(VerifiedPackage::object_id)
            .collect::<Vec<_>>(),
        vec![first.object_id()]
    );
    for limited in [
        Limits {
            max_lookup_bytes: total_bytes - 1,
            ..limits.clone()
        },
        Limits {
            max_envelope_bytes: max_envelope_bytes - 1,
            ..limits.clone()
        },
        Limits {
            max_objects: 1,
            ..limits
        },
    ] {
        assert_eq!(
            Registry::open(&root, limited)
                .unwrap()
                .scan_for_dependencies(None, &requested)
                .unwrap_err()
                .kind(),
            ErrorKind::ResourceLimit
        );
    }
}

#[test]
fn dependency_scan_rejects_unrelated_corruption_even_without_requests() {
    let (_temporary, root, registry) = fixture();
    let first = package();
    registry.publish(&first).unwrap();
    let malformed = b"{";
    let corrupt_identity = digest(malformed);
    assert_ne!(corrupt_identity, first.object_id());
    std::fs::write(registry.object_path(corrupt_identity), malformed).unwrap();
    for (module_hash, requested) in [
        (None, BTreeSet::new()),
        (None, BTreeSet::from([first.object_id()])),
        (Some(first.module_tree_hash()), BTreeSet::new()),
    ] {
        assert_eq!(
            registry
                .scan_for_dependencies(module_hash, &requested)
                .unwrap_err()
                .kind(),
            ErrorKind::CorruptObject
        );
    }
    assert_eq!(std::fs::read_dir(root.join("objects")).unwrap().count(), 2);
    assert_eq!(
        std::fs::read(registry.object_path(corrupt_identity)).unwrap(),
        malformed
    );
}

#[test]
fn dependency_request_count_is_bounded_before_store_inspection() {
    let (_temporary, root, registry) = fixture();
    let mut requested = (0..128)
        .map(|byte| Digest32::new([byte; 32]))
        .collect::<BTreeSet<_>>();
    assert_eq!(requested.len(), 128);
    assert!(
        registry
            .scan_for_dependencies(None, &requested)
            .unwrap()
            .is_empty()
    );
    requested.insert(Digest32::new([128; 32]));
    // An invalid layout would fail first if the request count were not checked
    // before enumerating or allocating store entries.
    std::fs::remove_dir(root.join("objects")).unwrap();
    let error = registry
        .scan_for_dependencies(None, &requested)
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::ResourceLimit);
    assert_eq!(
        error.to_string(),
        "dependency object request count exceeds limit"
    );
}

#[test]
fn exact_objects_are_idempotent_and_module_lookup_keeps_variants() {
    let (_temporary, root, registry) = fixture();
    let first = package();
    assert_eq!(
        registry.publish(&first).unwrap().status,
        PublishStatus::Published
    );
    let before = std::fs::read(registry.object_path(first.object_id())).unwrap();
    assert_eq!(
        registry.publish(&first).unwrap().status,
        PublishStatus::AlreadyPresent
    );
    assert_eq!(
        std::fs::read(registry.object_path(first.object_id())).unwrap(),
        before
    );
    let mut second_bytes = vec![b' '];
    second_bytes.extend_from_slice(first.bytes());
    let second = verify_package(&second_bytes, &Limits::default()).unwrap();
    assert_ne!(first.object_id(), second.object_id());
    assert_eq!(first.module_tree_hash(), second.module_tree_hash());
    assert_eq!(
        registry.publish(&second).unwrap().status,
        PublishStatus::Published
    );
    let mut expected = vec![first.object_id(), second.object_id()];
    expected.sort();
    let objects = Registry::open(&root, Limits::default())
        .unwrap()
        .lookup(first.module_tree_hash())
        .unwrap();
    assert_eq!(
        objects
            .iter()
            .map(VerifiedPackage::object_id)
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        registry.get(first.object_id()).unwrap().bytes(),
        first.bytes()
    );
    assert!(registry.lookup(Digest32::new([9; 32])).unwrap().is_empty());
    assert_eq!(
        registry.get(Digest32::new([9; 32])).unwrap_err().kind(),
        ErrorKind::UnknownObject
    );
    assert!(
        std::fs::read_dir(root.join(".staging"))
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn corrupt_existing_object_is_never_replaced_or_reported_present() {
    let (_temporary, _root, registry) = fixture();
    let package = package();
    let destination = registry.object_path(package.object_id());
    std::fs::write(&destination, b"partial object").unwrap();
    assert_eq!(
        registry.publish(&package).unwrap_err().kind(),
        ErrorKind::CorruptObject
    );
    assert_eq!(
        registry.get(package.object_id()).unwrap_err().kind(),
        ErrorKind::CorruptObject
    );
    assert_eq!(
        registry
            .lookup(package.module_tree_hash())
            .unwrap_err()
            .kind(),
        ErrorKind::CorruptObject
    );
    assert_eq!(std::fs::read(destination).unwrap(), b"partial object");
    assert!(
        std::fs::read_dir(registry.root.join(".staging"))
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn object_count_aggregate_read_and_store_local_admission_limits_are_enforced() {
    let (_temporary, root, registry) = fixture();
    let package = package();
    registry.publish(&package).unwrap();
    let limits = Limits {
        max_objects: 0,
        ..Limits::default()
    };
    let bounded = Registry::open(&root, limits).unwrap();
    assert_eq!(
        bounded
            .lookup(package.module_tree_hash())
            .unwrap_err()
            .kind(),
        ErrorKind::ResourceLimit
    );
    let limits = Limits {
        max_lookup_bytes: package.bytes().len() - 1,
        ..Limits::default()
    };
    assert_eq!(
        Registry::open(&root, limits)
            .unwrap()
            .lookup(package.module_tree_hash())
            .unwrap_err()
            .kind(),
        ErrorKind::ResourceLimit
    );
    let limits = Limits {
        max_envelope_bytes: package.bytes().len() - 1,
        ..Limits::default()
    };
    assert_eq!(
        Registry::open(&root, limits)
            .unwrap()
            .publish(&package)
            .unwrap_err()
            .kind(),
        ErrorKind::ResourceLimit
    );
    let limits = Limits {
        max_objects: 1,
        ..Limits::default()
    };
    let bounded = Registry::open(&root, limits).unwrap();
    assert_eq!(
        bounded.publish(&package).unwrap().status,
        PublishStatus::AlreadyPresent
    );
    let mut variant = vec![b' '];
    variant.extend_from_slice(package.bytes());
    let variant = verify_package(&variant, &Limits::default()).unwrap();
    assert_eq!(
        bounded.publish(&variant).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    assert_eq!(std::fs::read_dir(root.join("objects")).unwrap().count(), 1);
}

#[test]
fn lookup_rejects_unexpected_or_misnamed_objects_and_ignores_staging() {
    let (_temporary, root, registry) = fixture();
    let package = package();
    std::fs::write(root.join(".staging/interrupted.tmp"), b"{").unwrap();
    assert!(
        registry
            .lookup(package.module_tree_hash())
            .unwrap()
            .is_empty()
    );
    assert!(
        !root.join(".publish.lock").exists(),
        "read-only lookup must not create a lock"
    );
    std::fs::write(root.join("objects/not-a-hash.json"), package.bytes()).unwrap();
    assert_eq!(
        registry
            .lookup(package.module_tree_hash())
            .unwrap_err()
            .kind(),
        ErrorKind::CorruptObject
    );
    std::fs::remove_file(root.join("objects/not-a-hash.json")).unwrap();
    std::fs::write(
        registry.object_path(Digest32::new([9; 32])),
        package.bytes(),
    )
    .unwrap();
    assert_eq!(
        registry
            .lookup(package.module_tree_hash())
            .unwrap_err()
            .kind(),
        ErrorKind::CorruptObject
    );
    std::fs::remove_file(registry.object_path(Digest32::new([9; 32]))).unwrap();
    std::fs::create_dir(root.join("objects/directory.json")).unwrap();
    assert_eq!(
        registry
            .lookup(package.module_tree_hash())
            .unwrap_err()
            .kind(),
        ErrorKind::CorruptObject
    );
}

#[cfg(unix)]
#[test]
fn store_paths_lock_and_objects_reject_symlinks() {
    use std::os::unix::fs::symlink;
    let (_temporary, root, registry) = fixture();
    let package = package();
    symlink("objects", root.join("alias")).unwrap();
    assert_eq!(
        Registry::open(&root.join("alias/.."), Limits::default())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidPath
    );
    let external = root.parent().unwrap().join("external.json");
    std::fs::write(&external, package.bytes()).unwrap();
    symlink(&external, registry.object_path(package.object_id())).unwrap();
    assert_eq!(
        registry.get(package.object_id()).unwrap_err().kind(),
        ErrorKind::InvalidPath
    );
    assert_eq!(
        registry
            .lookup(package.module_tree_hash())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidPath
    );
    assert_eq!(
        registry.publish(&package).unwrap_err().kind(),
        ErrorKind::InvalidPath
    );
    std::fs::remove_file(registry.object_path(package.object_id())).unwrap();
    // The rejected publication above acquired its ordinary lock before finding
    // the invalid object. Replace that regular file with this separate fixture.
    let lock_path = root.join(".publish.lock");
    assert!(std::fs::symlink_metadata(&lock_path).unwrap().is_file());
    std::fs::remove_file(&lock_path).unwrap();
    symlink(&external, root.join(".publish.lock")).unwrap();
    assert_eq!(
        registry.publish(&package).unwrap_err().kind(),
        ErrorKind::InvalidPath
    );
    std::fs::remove_file(root.join(".publish.lock")).unwrap();
    std::fs::rename(root.join(".staging"), root.join("real-staging")).unwrap();
    symlink("real-staging", root.join(".staging")).unwrap();
    assert_eq!(
        Registry::open(&root, Limits::default()).unwrap_err().kind(),
        ErrorKind::InvalidPath
    );
}

struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<std::process::ChildStdout>,
}

impl Worker {
    fn start(root: &Path, action: &str, variant: bool) -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "store::tests::registry_child_entry",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("OXIDE_REGISTRY_TEST_ROOT", root)
            .env("OXIDE_REGISTRY_TEST_ACTION", action)
            .env(
                "OXIDE_REGISTRY_TEST_VARIANT",
                if variant { "1" } else { "0" },
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut worker = Self {
            child,
            input,
            output,
        };
        worker.line("REGISTRY_READY");
        worker
    }

    fn line(&mut self, expected: &str) {
        let mut line = String::new();
        loop {
            line.clear();
            assert!(
                self.output.read_line(&mut line).unwrap() > 0,
                "child closed before {expected}"
            );
            // The ordinary Rust test harness can prefix the first line.
            if line.trim_end().ends_with(expected) {
                return;
            }
        }
    }

    fn release(&mut self) {
        self.input.write_all(b"x").unwrap();
        self.input.flush().unwrap();
    }

    fn finish(mut self) -> String {
        let mut output = String::new();
        self.output.read_to_string(&mut output).unwrap();
        let mut error = String::new();
        self.child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut error)
            .unwrap();
        assert!(
            self.child.wait().unwrap().success(),
            "child failed: {output}\n{error}"
        );
        output
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[test]
fn registry_child_entry() {
    let Some(root) = std::env::var_os("OXIDE_REGISTRY_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let action = std::env::var("OXIDE_REGISTRY_TEST_ACTION").unwrap();
    let mut bytes = fixture_bytes();
    if std::env::var("OXIDE_REGISTRY_TEST_VARIANT").unwrap() == "1" {
        bytes.insert(0, b' ');
    }
    let package = verify_package(&bytes, &Limits::default()).unwrap();
    let registry = Registry::open(&root, Limits::default()).unwrap();
    println!("REGISTRY_READY");
    io::stdout().flush().unwrap();
    let mut signal = [0];
    io::stdin().read_exact(&mut signal).unwrap();
    if action == "partial" {
        let path = root.join(".staging/interrupted-child.tmp");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        file.write_all(&bytes[..bytes.len() / 2]).unwrap();
        file.sync_all().unwrap();
        println!("REGISTRY_PARTIAL");
        io::stdout().flush().unwrap();
        io::stdin().read_exact(&mut signal).unwrap();
        panic!("parent must terminate interrupted writer before commit");
    }
    let outcome = registry.publish(&package).unwrap();
    println!(
        "REGISTRY_OUTCOME {}",
        serde_json::to_string(&outcome).unwrap()
    );
    io::stdout().flush().unwrap();
}

#[test]
fn independent_processes_publish_same_object_once_without_overwrite() {
    let (_temporary, root, registry) = fixture();
    let mut first = Worker::start(&root, "publish", false);
    let mut second = Worker::start(&root, "publish", false);
    first.release();
    second.release();
    let outputs = [first.finish(), second.finish()];
    assert_eq!(
        outputs
            .iter()
            .filter(|output| output.contains("\"status\":\"published\""))
            .count(),
        1
    );
    assert_eq!(
        outputs
            .iter()
            .filter(|output| output.contains("\"status\":\"already_present\""))
            .count(),
        1
    );
    assert_eq!(
        registry.lookup(package().module_tree_hash()).unwrap().len(),
        1
    );
    assert_eq!(std::fs::read_dir(root.join("objects")).unwrap().count(), 1);
}

#[test]
fn independent_process_variants_both_survive_under_one_module_hash() {
    let (_temporary, root, registry) = fixture();
    let mut first = Worker::start(&root, "publish", false);
    let mut second = Worker::start(&root, "publish", true);
    first.release();
    second.release();
    for output in [first.finish(), second.finish()] {
        assert!(output.contains("\"status\":\"published\""));
    }
    let matches = registry.lookup(package().module_tree_hash()).unwrap();
    assert_eq!(matches.len(), 2);
    assert!(matches[0].object_id() < matches[1].object_id());
}

#[test]
fn terminated_partial_writer_has_no_visible_object_and_does_not_block_publication() {
    let (_temporary, root, registry) = fixture();
    let mut writer = Worker::start(&root, "partial", false);
    writer.release();
    writer.line("REGISTRY_PARTIAL");
    assert!(
        registry
            .lookup(package().module_tree_hash())
            .unwrap()
            .is_empty()
    );
    writer.child.kill().unwrap();
    let status = writer.child.wait().unwrap();
    assert!(!status.success());
    assert_eq!(std::fs::read_dir(root.join(".staging")).unwrap().count(), 1);
    assert_eq!(
        registry.publish(&package()).unwrap().status,
        PublishStatus::Published
    );
    assert_eq!(
        registry.lookup(package().module_tree_hash()).unwrap().len(),
        1
    );
    assert_eq!(std::fs::read_dir(root.join(".staging")).unwrap().count(), 1);
}
