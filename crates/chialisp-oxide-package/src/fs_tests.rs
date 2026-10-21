use super::*;

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    (directory, root)
}

#[test]
fn bounded_regular_reads_include_exact_and_zero_boundaries() {
    let (_temporary, root) = fixture();
    let path = root.join("input");
    std::fs::write(&path, b"abc").unwrap();
    assert_eq!(read_regular_file(&path, 3).unwrap(), b"abc");
    assert_eq!(
        read_regular_file(&path, 2).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    assert_eq!(
        read_regular_file(&root, 100).unwrap_err().kind(),
        ErrorKind::InvalidPath
    );
    std::fs::write(&path, []).unwrap();
    assert!(read_regular_file(&path, 0).unwrap().is_empty());
    assert_eq!(
        read_regular_file(&root.join("missing"), 3)
            .unwrap_err()
            .kind(),
        ErrorKind::Io
    );
}

#[test]
fn directory_creation_does_not_accept_file_components() {
    let (_temporary, root) = fixture();
    create_directories(&root.join("a/b/c")).unwrap();
    require_directory(&root.join("a/b/c")).unwrap();
    std::fs::write(root.join("file"), b"x").unwrap();
    assert_eq!(
        create_directories(&root.join("file/child"))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidPath
    );
    assert_eq!(
        absolute_path(Path::new("")).unwrap_err().kind(),
        ErrorKind::InvalidPath
    );
}

#[cfg(unix)]
#[test]
fn in_root_and_escaping_symlink_components_are_rejected() {
    use std::os::unix::fs::symlink;
    let (_temporary, root) = fixture();
    std::fs::create_dir(root.join("real")).unwrap();
    std::fs::write(root.join("real/input"), b"abc").unwrap();
    symlink("real", root.join("linked")).unwrap();
    symlink("real/input", root.join("input-link")).unwrap();
    symlink("missing", root.join("dangling")).unwrap();
    for path in [
        root.join("linked/input"),
        root.join("input-link"),
        root.join("dangling"),
    ] {
        assert_eq!(
            read_regular_file(&path, 3).unwrap_err().kind(),
            ErrorKind::InvalidPath
        );
    }
    assert_eq!(
        create_directories(&root.join("linked/new"))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidPath
    );
    // Lexical parent traversal cannot erase a symlink before it is inspected.
    assert_eq!(
        check_no_symlink_components(&root.join("linked/../real/input"))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidPath
    );
    let (outside_temp, outside) = fixture();
    std::fs::write(outside.join("input"), b"abc").unwrap();
    symlink(outside.join("input"), root.join("escape")).unwrap();
    assert_eq!(
        read_regular_file(&root.join("escape"), 3)
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidPath
    );
    drop(outside_temp);
}

#[cfg(unix)]
#[test]
fn sockets_devices_and_fifos_are_rejected_before_open() {
    use std::os::unix::net::UnixListener;
    let (_temporary, root) = fixture();
    let socket = root.join("socket");
    let _listener = UnixListener::bind(&socket).unwrap();
    assert_eq!(
        read_regular_file(&socket, 10).unwrap_err().kind(),
        ErrorKind::InvalidPath
    );
    assert_eq!(
        read_regular_file(Path::new("/dev/null"), 10)
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidPath
    );
    let fifo = root.join("fifo");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(status.success(), "create actual FIFO fixture");
    // A blocking open here would hang: there is intentionally no FIFO writer.
    assert_eq!(
        read_regular_file(&fifo, 10).unwrap_err().kind(),
        ErrorKind::InvalidPath
    );
}
