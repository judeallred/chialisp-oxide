use super::*;
use collector::{Disposition, ReadKind};
use std::sync::{
    Barrier,
    atomic::{AtomicUsize, Ordering},
};

fn capture(limits: CaptureLimits) -> CaptureHandle {
    CaptureHandle::new(
        UnitIdentity {
            crate_name: "fixture".into(),
            analysis_target: "host".into(),
            features: vec![],
            library_source: "package/lib.rs".into(),
        },
        limits,
    )
    .unwrap()
}
fn complete(handle: &CaptureHandle) {
    handle.finish_features();
    handle.record_export(
        "fixture::puzzle",
        &format!("{}.ir.json", collector::sha(b"fixture::puzzle")),
        b"{}",
    );
}
fn observe(handle: &CaptureHandle, label: &str, kind: ReadKind, bytes: &[u8]) {
    handle
        .begin_read()
        .succeeded(Ok(Disposition::Attach(label.into())), kind, bytes);
}
fn ready(limits: CaptureLimits) -> CaptureHandle {
    let handle = capture(limits);
    observe(&handle, "package/lib.rs", ReadKind::Text, b"abc");
    complete(&handle);
    handle
}

struct FakeClassifier;
impl Classifier for FakeClassifier {
    fn classify(&self, path: &Path) -> Result<Disposition> {
        let name = path.file_name().unwrap().to_str().unwrap();
        Ok(Disposition::Attach(
            if name == "binary" {
                "package/data.bin"
            } else {
                "package/lib.rs"
            }
            .into(),
        ))
    }
    fn working_directory(&self, _: &Path) -> Result<()> {
        Ok(())
    }
}
struct FakeLoader {
    text: String,
    binary: Arc<[u8]>,
    calls: Arc<AtomicUsize>,
    reentrant: Option<CaptureHandle>,
    block: Option<Arc<Barrier>>,
}
impl FileLoader for FakeLoader {
    fn file_exists(&self, _: &Path) -> bool {
        self.calls.fetch_add(1, Ordering::SeqCst);
        false
    }
    fn current_directory(&self) -> io::Result<PathBuf> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok("/virtual-cwd".into())
    }
    fn read_file(&self, path: &Path) -> io::Result<String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if path.ends_with("fail") {
            return Err(io::Error::from_raw_os_error(13));
        }
        if let Some(capture) = &self.reentrant {
            observe(capture, "package/helper.rs", ReadKind::Text, b"helper");
        }
        if let Some(barrier) = &self.block {
            barrier.wait();
            barrier.wait();
        }
        Ok(self.text.clone())
    }
    fn read_binary_file(&self, _: &Path) -> io::Result<Arc<[u8]>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.binary.clone())
    }
}

#[test]
fn delegated_raw_text_binary_errors_and_arc_identity_are_unchanged() {
    let handle = capture(CaptureLimits::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let binary: Arc<[u8]> = Arc::from([0xff, 0, 0x80]);
    let text = "\u{feff}line\r\n";
    let loader = RecordingLoader {
        inner: Box::new(FakeLoader {
            text: text.into(),
            binary: binary.clone(),
            calls: calls.clone(),
            reentrant: None,
            block: None,
        }),
        capture: handle.clone(),
        classifier: Arc::new(FakeClassifier),
    };
    assert!(!loader.file_exists(Path::new("absent")));
    assert_eq!(
        loader.current_directory().unwrap(),
        Path::new("/virtual-cwd")
    );
    assert_eq!(
        loader.read_file(Path::new("lib.rs")).unwrap().as_bytes(),
        text.as_bytes()
    );
    let returned = loader.read_binary_file(Path::new("binary")).unwrap();
    assert!(Arc::ptr_eq(&returned, &binary));
    let error = loader.read_file(Path::new("fail")).unwrap_err();
    assert_eq!(error.raw_os_error(), Some(13));
    assert_eq!(calls.load(Ordering::SeqCst), 5);
    complete(&handle);
    let sealed = handle.try_seal().unwrap();
    assert_eq!(sealed.receipt.files.len(), 2);
    let text_index = sealed
        .receipt
        .files
        .iter()
        .position(|row| row.path == "package/lib.rs")
        .unwrap();
    assert_eq!(sealed.contents[text_index], text.as_bytes());
    let binary_index = sealed
        .receipt
        .files
        .iter()
        .position(|row| row.path == "package/data.bin")
        .unwrap();
    assert_eq!(sealed.contents[binary_index], binary.as_ref());
    assert_eq!(sealed.receipt.omitted.outside_roots_reads, 0);
}

#[test]
fn empty_binary_is_retained_without_becoming_text_or_nil() {
    let handle = ready(CaptureLimits::default());
    let original: Arc<[u8]> = Arc::from([]);
    let loader = RecordingLoader {
        inner: Box::new(FakeLoader {
            text: String::new(),
            binary: original.clone(),
            calls: Arc::new(AtomicUsize::new(0)),
            reentrant: None,
            block: None,
        }),
        capture: handle.clone(),
        classifier: Arc::new(FakeClassifier),
    };
    assert!(Arc::ptr_eq(
        &original,
        &loader.read_binary_file(Path::new("binary")).unwrap()
    ));
    let sealed = handle.try_seal().unwrap();
    let index = sealed
        .receipt
        .files
        .iter()
        .position(|row| row.path == "package/data.bin")
        .unwrap();
    assert_eq!(sealed.receipt.files[index].bytes, 0);
    assert!(sealed.contents[index].is_empty());
    assert_eq!(
        sealed.receipt.files[index].sha256,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn repeat_kinds_counts_features_and_known_hashes_are_deterministic() {
    let handle = ready(CaptureLimits::default());
    observe(&handle, "package/lib.rs", ReadKind::Binary, b"abc");
    observe(&handle, "package/lib.rs", ReadKind::Text, b"abc");
    handle.record_feature("z");
    handle.record_feature("a");
    handle.record_feature("z");
    let sealed = handle.try_seal().unwrap();
    assert_eq!(sealed.receipt.unit.features, ["a", "z"]);
    let row = &sealed.receipt.files[0];
    assert_eq!(row.read_kinds, [ReadKind::Binary, ReadKind::Text]);
    assert_eq!(row.read_count, 3);
    assert_eq!(
        row.sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sealed.receipt.exports[0].sha256,
        "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
    );
    assert_eq!(handle.try_seal().err().unwrap().kind, ErrorKind::Sealed);
}

#[test]
fn sticky_capture_failure_does_not_change_later_loader_results() {
    let handle = ready(CaptureLimits::default());
    observe(&handle, "package/lib.rs", ReadKind::Text, b"different");
    let calls = Arc::new(AtomicUsize::new(0));
    let raw: Arc<[u8]> = Arc::from([0xff]);
    let loader = RecordingLoader {
        inner: Box::new(FakeLoader {
            text: "still returned".into(),
            binary: raw.clone(),
            calls: calls.clone(),
            reentrant: None,
            block: None,
        }),
        capture: handle.clone(),
        classifier: Arc::new(FakeClassifier),
    };
    assert_eq!(
        loader.read_file(Path::new("lib.rs")).unwrap(),
        "still returned"
    );
    assert!(Arc::ptr_eq(
        &raw,
        &loader.read_binary_file(Path::new("binary")).unwrap()
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        handle.try_seal().err().unwrap().kind,
        ErrorKind::ConflictingRead
    );
}

#[test]
fn reentrant_delegate_observation_has_no_collector_lock_deadlock() {
    let handle = capture(CaptureLimits::default());
    let loader = RecordingLoader {
        inner: Box::new(FakeLoader {
            text: "abc".into(),
            binary: Arc::from([]),
            calls: Arc::new(AtomicUsize::new(0)),
            reentrant: Some(handle.clone()),
            block: None,
        }),
        capture: handle.clone(),
        classifier: Arc::new(FakeClassifier),
    };
    assert_eq!(loader.read_file(Path::new("lib.rs")).unwrap(), "abc");
    complete(&handle);
    assert_eq!(handle.try_seal().unwrap().receipt.files.len(), 2);
}

#[test]
fn sealing_during_actual_blocked_delegate_fails_without_waiting() {
    let handle = capture(CaptureLimits::default());
    complete(&handle);
    let barrier = Arc::new(Barrier::new(2));
    let loader = RecordingLoader {
        inner: Box::new(FakeLoader {
            text: "abc".into(),
            binary: Arc::from([]),
            calls: Arc::new(AtomicUsize::new(0)),
            reentrant: None,
            block: Some(barrier.clone()),
        }),
        capture: handle.clone(),
        classifier: Arc::new(FakeClassifier),
    };
    let worker = std::thread::spawn(move || loader.read_file(Path::new("lib.rs")));
    barrier.wait();
    assert_eq!(
        handle.try_seal().err().unwrap().kind,
        ErrorKind::PendingReads
    );
    barrier.wait();
    assert_eq!(worker.join().unwrap().unwrap(), "abc");
    assert_eq!(
        handle.try_seal().err().unwrap().kind,
        ErrorKind::PendingReads
    );
}

#[test]
fn concurrent_matching_and_conflicting_reads_are_bounded_and_order_independent() {
    for conflicting in [false, true] {
        let handle = ready(CaptureLimits::default());
        let barrier = Arc::new(Barrier::new(3));
        let workers: Vec<_> = [false, true]
            .into_iter()
            .map(|second| {
                let handle = handle.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let ticket = handle.begin_read();
                    barrier.wait();
                    ticket.succeeded(
                        Ok(Disposition::Attach("package/shared.bin".into())),
                        ReadKind::Binary,
                        if conflicting && second { b"b" } else { b"a" },
                    );
                })
            })
            .collect();
        barrier.wait();
        for worker in workers {
            worker.join().unwrap();
        }
        let sealed = handle.try_seal();
        if conflicting {
            assert_eq!(sealed.err().unwrap().kind, ErrorKind::ConflictingRead);
        } else {
            let sealed = sealed.unwrap();
            assert_eq!(sealed.receipt.files[1].path, "package/shared.bin");
            assert_eq!(sealed.receipt.files[1].read_count, 2);
        }
    }
}

#[test]
fn read_guards_release_during_unwind_and_ordinary_failures() {
    let handle = ready(CaptureLimits::default());
    {
        let _failed = handle.begin_read();
    }
    let worker = {
        let handle = handle.clone();
        std::thread::spawn(move || {
            let _guard = handle.begin_read();
            panic!("unwind control");
        })
    };
    assert!(worker.join().is_err());
    assert!(handle.try_seal().is_ok());
}

#[test]
fn collection_count_byte_label_feature_export_and_work_limits_fail_closed() {
    for limits in [
        CaptureLimits {
            reads: 0,
            ..CaptureLimits::default()
        },
        CaptureLimits {
            files: 0,
            ..CaptureLimits::default()
        },
        CaptureLimits {
            file_bytes: 2,
            ..CaptureLimits::default()
        },
        CaptureLimits {
            total_bytes: 2,
            ..CaptureLimits::default()
        },
        CaptureLimits {
            labels: 1,
            ..CaptureLimits::default()
        },
        CaptureLimits {
            work: 0,
            ..CaptureLimits::default()
        },
        CaptureLimits {
            exports: 0,
            ..CaptureLimits::default()
        },
        CaptureLimits {
            ir_bytes: 1,
            ..CaptureLimits::default()
        },
        CaptureLimits {
            total_ir_bytes: 1,
            ..CaptureLimits::default()
        },
    ] {
        assert_eq!(
            ready(limits).try_seal().err().unwrap().kind,
            ErrorKind::ResourceLimit
        );
    }
    let handle = ready(CaptureLimits {
        features: 0,
        ..CaptureLimits::default()
    });
    handle.record_feature("x");
    assert_eq!(
        handle.try_seal().err().unwrap().kind,
        ErrorKind::ResourceLimit
    );
    let handle = ready(CaptureLimits {
        feature_bytes: 1,
        ..CaptureLimits::default()
    });
    handle.record_feature("xx");
    assert_eq!(
        handle.try_seal().err().unwrap().kind,
        ErrorKind::ResourceLimit
    );
    let handle = ready(CaptureLimits {
        reads: 1,
        ..CaptureLimits::default()
    });
    observe(&handle, "package/lib.rs", ReadKind::Text, b"abc");
    assert_eq!(
        handle.try_seal().err().unwrap().kind,
        ErrorKind::ResourceLimit
    );
    let exact = ready(CaptureLimits {
        file_bytes: 3,
        total_bytes: 3,
        files: 1,
        exports: 1,
        reads: 1,
        ..CaptureLimits::default()
    });
    assert_eq!(exact.try_seal().unwrap().contents, [b"abc".to_vec()]);
}

struct Temporary(PathBuf);
impl Temporary {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oxide-source-receipt-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn roots_include_exact_external_library_but_omit_its_siblings_and_prioritize_out_dir() {
    let temporary = Temporary::new();
    let package = temporary.0.join("package");
    let generated = package.join("generated");
    let external = temporary.0.join("external");
    std::fs::create_dir_all(&generated).unwrap();
    std::fs::create_dir(&external).unwrap();
    let library = external.join("library.rs");
    std::fs::write(&library, b"abc").unwrap();
    std::fs::write(external.join("sibling.rs"), b"external contents").unwrap();
    std::fs::write(generated.join("file.rs"), b"generated").unwrap();
    let roots = SourceRoots::new(&package, Some(&generated), &library).unwrap();
    assert_eq!(roots.library_label().unwrap(), "entry/library.rs");
    assert!(matches!(
        roots.classify(&external.join("sibling.rs")).unwrap(),
        Disposition::OutsideRoots
    ));
    assert!(
        matches!(roots.classify(&generated.join("file.rs")).unwrap(),Disposition::Attach(label) if label=="generated/file.rs")
    );
    let relative = std::env::current_dir().unwrap().join(".");
    roots.working_directory(&relative).unwrap();
    assert!(roots.working_directory(&temporary.0).is_err());
}

#[test]
fn relative_cwd_drift_is_rejected_without_changing_file_loader_results() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "source_receipt::tests::relative_cwd_drift_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("CHIALISP_OXIDE_SOURCE_CWD_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "cwd child failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}

#[test]
#[ignore = "changes cwd only in the explicitly isolated child process"]
fn relative_cwd_drift_child() {
    if std::env::var_os("CHIALISP_OXIDE_SOURCE_CWD_CHILD").as_deref()
        != Some(std::ffi::OsStr::new("1"))
    {
        return;
    }
    let original_cwd = std::env::current_dir().unwrap();
    let temporary = Temporary::new();
    let package = temporary.0.join("package");
    let external = temporary.0.join("external");
    std::fs::create_dir(&package).unwrap();
    std::fs::create_dir(&external).unwrap();
    let library = package.join("lib.rs");
    std::fs::write(&library, b"abc").unwrap();
    std::fs::write(package.join("data.bin"), [1, 2, 3]).unwrap();
    std::fs::write(external.join("lib.rs"), b"outside\r\n").unwrap();
    std::fs::write(external.join("data.bin"), [0xff, 0]).unwrap();
    std::env::set_current_dir(&package).unwrap();
    let roots = Arc::new(SourceRoots::new(&package, None, &library).unwrap());
    let text = capture(CaptureLimits::default());
    let binary = capture(CaptureLimits::default());
    let absolute = capture(CaptureLimits::default());
    let make_loader = |capture: &CaptureHandle| RecordingLoader {
        inner: Box::new(RealFileLoader),
        capture: capture.clone(),
        classifier: roots.clone(),
    };
    let text_loader = make_loader(&text);
    let binary_loader = make_loader(&binary);
    let absolute_loader = make_loader(&absolute);
    for (loader, handle) in [
        (&text_loader, &text),
        (&binary_loader, &binary),
        (&absolute_loader, &absolute),
    ] {
        assert_eq!(loader.read_file(Path::new("lib.rs")).unwrap(), "abc");
        complete(handle);
    }
    assert_eq!(
        binary_loader
            .read_binary_file(Path::new("data.bin"))
            .unwrap()
            .as_ref(),
        [1, 2, 3],
    );
    std::env::set_current_dir(&external).unwrap();
    assert_eq!(
        text_loader.read_file(Path::new("lib.rs")).unwrap(),
        "outside\r\n",
    );
    assert_eq!(
        binary_loader
            .read_binary_file(Path::new("data.bin"))
            .unwrap()
            .as_ref(),
        [0xff, 0],
    );
    for handle in [&text, &binary] {
        assert_eq!(
            handle.try_seal().err().unwrap().kind,
            ErrorKind::Configuration,
        );
    }
    assert_eq!(
        text_loader
            .read_file(Path::new("missing"))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::NotFound,
    );
    assert_eq!(absolute_loader.read_file(&library).unwrap(), "abc");
    assert!(absolute.try_seal().is_ok());
    std::env::set_current_dir(original_cwd).unwrap();
}

#[cfg(unix)]
#[test]
fn source_path_aliases_resolve_without_widening_external_roots() {
    use std::os::unix::fs::symlink;
    let temporary = Temporary::new();
    let package = temporary.0.join("package");
    std::fs::create_dir(&package).unwrap();
    let library = package.join("lib.rs");
    std::fs::write(&library, b"abc").unwrap();
    let external = temporary.0.join("external");
    std::fs::write(&external, b"external").unwrap();
    symlink(&external, package.join("escape")).unwrap();
    symlink("lib.rs", package.join("alias.rs")).unwrap();
    let roots = SourceRoots::new(&package, None, &library).unwrap();
    assert!(matches!(
        roots.classify(&package.join("escape")).unwrap(),
        Disposition::OutsideRoots
    ));
    assert!(
        matches!(roots.classify(&package.join("alias.rs")).unwrap(),Disposition::Attach(label) if label=="package/lib.rs")
    );
    assert!(roots.classify(&package.join("missing")).is_err());
}

#[test]
fn outside_observations_remain_opaque_and_invalid_labels_do_not_seal() {
    let handle = ready(CaptureLimits::default());
    handle.begin_read().succeeded(
        Ok(Disposition::OutsideRoots),
        ReadKind::Binary,
        b"secret-external-body",
    );
    let sealed = handle.try_seal().unwrap();
    let json = serde_json::to_string(&sealed.receipt).unwrap();
    assert_eq!(sealed.receipt.omitted.outside_roots_reads, 1);
    assert_eq!(sealed.receipt.omitted.outside_roots_bytes, 20);
    assert!(!json.contains("secret-external-body"));
    assert_eq!(sealed.contents, [b"abc".to_vec()]);
    for label in [
        "../escape",
        "package/../escape",
        "package//file",
        "package/line\nfile",
        "entry/other.rs",
    ] {
        let handle = ready(CaptureLimits::default());
        observe(&handle, label, ReadKind::Text, b"x");
        assert_eq!(
            handle.try_seal().err().unwrap().kind,
            ErrorKind::Configuration
        );
    }
}

#[test]
fn retained_blobs_are_not_reread_from_source_and_unit_publication_never_overwrites() {
    let temporary = Temporary::new();
    let work = temporary.0.join("work");
    std::fs::create_dir(&work).unwrap();
    let handle = ready(CaptureLimits::default());
    let sealed = handle.try_seal().unwrap();
    sealed.write_private_unit(&work).unwrap();
    let metadata = std::fs::read(work.join("source-unit.json")).unwrap();
    assert_eq!(
        std::fs::read(work.join(&sealed.receipt.files[0].blob)).unwrap(),
        b"abc"
    );
    assert!(sealed.write_private_unit(&work).is_err());
    assert_eq!(
        std::fs::read(work.join("source-unit.json")).unwrap(),
        metadata
    );
    let other = temporary.0.join("other");
    std::fs::create_dir(&other).unwrap();
    std::fs::write(other.join(&sealed.receipt.files[0].blob), b"bad").unwrap();
    assert!(sealed.write_private_unit(&other).is_err());
    assert!(!other.join("source-unit.json").exists());
    let small = temporary.0.join("small");
    std::fs::create_dir(&small).unwrap();
    let sealed = ready(CaptureLimits {
        json_bytes: 1,
        ..CaptureLimits::default()
    })
    .try_seal()
    .unwrap();
    assert_eq!(
        sealed.write_private_unit(&small).err().unwrap().kind,
        ErrorKind::ResourceLimit
    );
    assert!(std::fs::read_dir(&small).unwrap().next().is_none());
}

#[test]
fn bounded_ir_pretty_writer_preserves_exact_bytes_and_checks_one_over_before_extend() {
    let value = serde_json::json!({"body":[1,2,"raw\r\n\u{feff}"],"name":"fixture::value"});
    let expected = serde_json::to_vec_pretty(&value).unwrap();
    assert_eq!(serialize_ir(&value, expected.len()).unwrap(), expected);
    assert_ne!(expected.last(), Some(&b'\n'));
    assert_eq!(
        serialize_ir(&value, expected.len() - 1).unwrap_err().kind,
        ErrorKind::ResourceLimit
    );
    let mut writer = JsonWriter {
        bytes: vec![],
        max: 3,
        exceeded: false,
    };
    writer.write_all(b"abc").unwrap();
    assert!(writer.write_all(b"d").is_err());
    assert_eq!(writer.bytes, b"abc");
    assert!(writer.exceeded);
}

#[test]
fn staged_ir_uses_remaining_total_and_failed_batches_write_no_exports() {
    let temporary = Temporary::new();
    let output = temporary.0.join("exports");
    let value = serde_json::json!({"value":7});
    let length = serde_json::to_vec_pretty(&value).unwrap().len();
    let handle = capture(CaptureLimits {
        exports: 2,
        ir_bytes: length,
        total_ir_bytes: length * 2 - 1,
        ..CaptureLimits::default()
    });
    observe(&handle, "package/lib.rs", ReadKind::Text, b"abc");
    handle.finish_features();
    let result = (|| -> Result<Vec<StagedExport>> {
        Ok(vec![
            stage_export(&handle, "fixture::a", &value)?,
            stage_export(&handle, "fixture::b", &value)?,
        ])
    })();
    assert_eq!(result.err().unwrap().kind, ErrorKind::ResourceLimit);
    assert!(!output.exists());
    assert_eq!(
        handle.try_seal().err().unwrap().kind,
        ErrorKind::ResourceLimit
    );
}

#[test]
fn exact_staged_ir_batch_writes_original_json_only_after_complete_admission() {
    let temporary = Temporary::new();
    let output = temporary.0.join("exports");
    let value = serde_json::json!({"value":7});
    let expected = serde_json::to_vec_pretty(&value).unwrap();
    let handle = capture(CaptureLimits {
        exports: 2,
        ir_bytes: expected.len(),
        total_ir_bytes: expected.len() * 2,
        ..CaptureLimits::default()
    });
    observe(&handle, "package/lib.rs", ReadKind::Text, b"abc");
    handle.finish_features();
    let staged = vec![
        stage_export(&handle, "fixture::b", &value).unwrap(),
        stage_export(&handle, "fixture::a", &value).unwrap(),
    ];
    assert!(!output.exists());
    write_staged_exports(&output, staged).unwrap();
    let sealed = handle.try_seal().unwrap();
    assert_eq!(sealed.receipt.exports.len(), 2);
    for row in sealed.receipt.exports {
        assert_eq!(row.bytes, expected.len() as u64);
        assert_eq!(row.sha256, collector::sha(&expected));
        assert_eq!(std::fs::read(output.join(row.ir)).unwrap(), expected);
    }
}

#[test]
fn export_count_rejection_precedes_serializer_invocation() {
    struct Counted(Arc<AtomicUsize>);
    impl serde::Serialize for Counted {
        fn serialize<S: serde::Serializer>(
            &self,
            serializer: S,
        ) -> std::result::Result<S::Ok, S::Error> {
            self.0.fetch_add(1, Ordering::SeqCst);
            serializer.serialize_u64(7)
        }
    }
    let handle = capture(CaptureLimits {
        exports: 1,
        ..CaptureLimits::default()
    });
    stage_export(&handle, "fixture::first", &7).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let error = stage_export(&handle, "fixture::second", &Counted(calls.clone()))
        .err()
        .unwrap();
    assert_eq!(error.kind, ErrorKind::ResourceLimit);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
