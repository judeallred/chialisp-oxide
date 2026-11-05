use super::*;

#[test]
fn malformed_and_unsupported_encodings_keep_existing_typed_failures() {
    for source in [vec![], bytes("ff"), bytes("ff80"), bytes("8080")] {
        kind(
            inspect_reveal(&source, &Limits::default(), None),
            ErrorKind::MalformedSerialization,
        );
    }
    kind(
        inspect_reveal(&bytes("8101"), &Limits::default(), None),
        ErrorKind::NonCanonicalSerialization,
    );
    kind(
        inspect_reveal(&bytes("fe01"), &Limits::default(), None),
        ErrorKind::UnsupportedEncoding,
    );
    kind(
        inspect_reveal(&bytes("fdff32303236"), &Limits::default(), None),
        ErrorKind::UnsupportedEncoding,
    );
}

#[test]
fn argument_count_and_exact_whole_pipeline_work_have_separate_host_bounds() {
    let source = curried(&[1], &vec![vec![0x80]; 128]);
    let result = inspect(&source);
    assert_eq!(result.arguments().len(), 128);
    let excessive = curried(&[1], &vec![vec![0x80]; 129]);
    kind(
        inspect_reveal(&excessive, &Limits::default(), None),
        ErrorKind::ResourceLimit,
    );
    let limits = Limits {
        max_work: result.work(),
        ..Limits::default()
    };
    assert_eq!(
        inspect_reveal(&source, &limits, None).unwrap().work(),
        result.work()
    );
    let limits = Limits {
        max_work: result.work() - 1,
        ..limits
    };
    kind(
        inspect_reveal(&source, &limits, None),
        ErrorKind::ResourceLimit,
    );
    let full = inspect_artifact(&source, Encoding::Plain, &Limits::default()).unwrap();
    let limits = Limits {
        max_work: full.stats().work as usize
            + super::super::catalog::CATALOG_MATCH_WORK
            + full.stats().nodes as usize
            + full.stats().depth as usize
            - 1,
        ..Limits::default()
    };
    let error = inspect_reveal(&source, &limits, None).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::ResourceLimit);
    assert!(error.to_string().contains("inspection aggregate work"));
}

#[test]
fn deep_argument_spans_are_iterative_and_limits_precede_output_copies() {
    let mut deep = Vec::new();
    for _ in 0..1500 {
        deep.extend_from_slice(&[0xff, 1]);
    }
    deep.push(0x80);
    let source = curried(&[1], &[deep.clone()]);
    let result = inspect(&source);
    assert_eq!(result.arguments()[0].bytes(), deep);
    let limits = Limits {
        max_blob_bytes: source.len(),
        max_expanded_bytes: source.len(),
        ..Limits::default()
    };
    inspect_reveal(&source, &limits, None).unwrap();
    kind(
        inspect_reveal(
            &source,
            &Limits {
                max_blob_bytes: source.len() - 1,
                ..limits.clone()
            },
            None,
        ),
        ErrorKind::ResourceLimit,
    );
    kind(
        inspect_reveal(
            &source,
            &Limits {
                max_depth: 100,
                ..limits
            },
            None,
        ),
        ErrorKind::ResourceLimit,
    );
}

#[test]
fn hex_transport_is_bounded_complete_and_distinct_from_tree_admission() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let path = root.join("reveal.hex");
    for (text, expected) in [
        ("80\n", bytes("80")),
        ("0x80\r\n", bytes("80")),
        (" 0XFF0180 ", bytes("ff0180")),
    ] {
        std::fs::write(&path, text).unwrap();
        assert_eq!(read_hex_file(&path, &Limits::default()).unwrap(), expected);
    }
    for text in ["", "0x", "8", "gg", "80 80", "0x80z", "\u{feff}80"] {
        std::fs::write(&path, text).unwrap();
        kind(
            read_hex_file(&path, &Limits::default()),
            ErrorKind::InvalidManifest,
        );
    }
    std::fs::write(&path, "ff").unwrap();
    let malformed = read_hex_file(&path, &Limits::default()).unwrap();
    kind(
        inspect_reveal(&malformed, &Limits::default(), None),
        ErrorKind::MalformedSerialization,
    );
    let limits = Limits {
        max_blob_bytes: 1,
        ..Limits::default()
    };
    std::fs::write(&path, "0x80\r\n").unwrap();
    assert_eq!(read_hex_file(&path, &limits).unwrap(), [0x80]);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(7)
        .unwrap();
    kind(read_hex_file(&path, &limits), ErrorKind::ResourceLimit);
    std::fs::write(&path, "80").unwrap();
    kind(
        read_hex_file(
            &path,
            &Limits {
                max_work: 1,
                ..Limits::default()
            },
        ),
        ErrorKind::ResourceLimit,
    );
    kind(
        read_hex_file(&root, &Limits::default()),
        ErrorKind::InvalidPath,
    );
    #[cfg(unix)]
    {
        let alias = root.join("alias.hex");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        kind(
            read_hex_file(&alias, &Limits::default()),
            ErrorKind::InvalidPath,
        );
    }
}
