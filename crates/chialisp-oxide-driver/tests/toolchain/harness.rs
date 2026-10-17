//! Direct native/wrapper invocations and relocation prerequisites.
use crate::support::Fixture;
use std::{
    ffi::OsStr,
    fs,
    path::{Component, Path, PathBuf},
    process::{Command, Output},
};

pub(crate) struct Unit {
    pub(crate) fixture: Fixture,
    pub(crate) host: String,
}

impl Unit {
    pub(crate) fn new(source: &str) -> Self {
        let fixture = Fixture::new();
        fixture.source(source);
        let host = fixture.host();
        Self { fixture, host }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.fixture.0
    }

    pub(crate) fn pinned(&self) -> PathBuf {
        Path::new(env!("CHIALISP_OXIDE_SYSROOT"))
            .canonicalize()
            .unwrap()
    }

    pub(crate) fn target_lib(&self) -> PathBuf {
        self.pinned()
            .join("lib/rustlib")
            .join(&self.host)
            .join("lib")
    }

    pub(crate) fn core(&self, extension: &str) -> PathBuf {
        let paths: Vec<_> = fs::read_dir(self.target_lib())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("libcore-")
                    && path.extension() == Some(OsStr::new(extension))
            })
            .collect();
        assert_eq!(paths.len(), 1, "ambiguous installed core {extension}");
        paths[0].clone()
    }

    pub(crate) fn args(&self, wrapped: bool, target: bool, crate_name: &str) -> Vec<String> {
        let out = self.root().join(if wrapped { "wrapped" } else { "native" });
        fs::create_dir_all(&out).unwrap();
        let mut args = vec![
            "--crate-name".into(),
            crate_name.into(),
            "--crate-type=lib".into(),
            "--edition=2024".into(),
            "--emit=metadata,dep-info".into(),
            "-Zbinary-dep-depinfo".into(),
            "--out-dir".into(),
            text(&out),
            text(&self.root().join("src/lib.rs")),
        ];
        if target {
            args.extend(["--target".into(), self.host.clone()]);
        }
        args
    }

    pub(crate) fn command(&self, wrapped: bool, selected: bool, crate_name: &str) -> Command {
        let mut command = if wrapped {
            let mut command = Command::new(env!("CARGO_BIN_EXE_chialisp-oxide-driver"));
            command.arg("--cargo-wrapper").arg(self.fixture.compiler());
            command
        } else {
            Command::new(self.fixture.compiler())
        };
        command
            .current_dir(self.root())
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER")
            .env_remove("CHIALISP_OXIDE_CARGO_MODE")
            .env("CHIALISP_OXIDE_PRIMARY_PACKAGES", crate_name)
            .env("CHIALISP_OXIDE_OUTPUT_DIR", self.root().join("exports"));
        if selected {
            command.env("CARGO_PRIMARY_PACKAGE", "1");
        } else {
            command.env_remove("CARGO_PRIMARY_PACKAGE");
        }
        command
    }

    pub(crate) fn run(
        &self,
        wrapped: bool,
        selected: bool,
        target: bool,
        extras: &[String],
    ) -> Output {
        self.command(wrapped, selected, "wrapper_fixture")
            .args(self.args(wrapped, target, "wrapper_fixture"))
            .args(extras)
            .output()
            .unwrap()
    }

    pub(crate) fn native(&self, extras: &[String]) {
        succeeds(self.run(false, true, true, extras));
    }

    pub(crate) fn loaded(&self, path: &Path) {
        let dep = fs::read_to_string(self.root().join("native/wrapper_fixture.d")).unwrap();
        let dep = dep.replace("\\ ", " ");
        let canonical = path.canonicalize().unwrap();
        assert!(
            dep.contains(&text(path)) || dep.contains(&text(&canonical)),
            "native rustc did not actually load the relocated library {}:\n{dep}",
            path.display()
        );
    }

    pub(crate) fn not_loaded(&self, path: &Path) {
        let dep = fs::read_to_string(self.root().join("native/wrapper_fixture.d")).unwrap();
        assert!(
            !dep.replace("\\ ", " ").contains(&text(path)),
            "native rustc unexpectedly loaded the test replacement: {dep}"
        );
    }

    pub(crate) fn copied_core(&self, extension: &str) -> PathBuf {
        let source = self.core(extension);
        let directory = self.root().join("replacement-libraries");
        fs::create_dir_all(&directory).unwrap();
        let destination = directory.join(source.file_name().unwrap());
        fs::copy(&source, &destination).unwrap();
        assert_eq!(fs::read(source).unwrap(), fs::read(&destination).unwrap());
        destination
    }

    pub(crate) fn rejects(&self, output: Output, reason: &str) {
        assert!(
            !output.status.success(),
            "unsupported configuration succeeded"
        );
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        assert!(
            diagnostics.contains("unsupported toolchain configuration")
                && diagnostics.contains(reason),
            "{diagnostics}"
        );
        assert!(
            !diagnostics.contains("internal compiler error"),
            "{diagnostics}"
        );
        assert!(
            self.fixture.exports().is_empty(),
            "failed unit published IR"
        );
    }

    pub(crate) fn exports_one(&self, output: Output) {
        succeeds(output);
        let programs = self.fixture.exports();
        assert_eq!(programs.len(), 1);
        assert_eq!(programs[0].name, "wrapper_fixture::entry");
        programs[0].validate().unwrap();
    }
}

pub(crate) fn text(path: &Path) -> String {
    path.to_str().expect("test paths must be UTF-8").into()
}
pub(crate) fn succeeds(output: Output) {
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
pub(crate) fn extern_arg(name: &str, path: &Path) -> Vec<String> {
    vec!["--extern".into(), format!("{name}={}", path.display())]
}
pub(crate) fn relative(from: &Path, to: &Path) -> PathBuf {
    let from: Vec<_> = from.components().collect();
    let to: Vec<_> = to.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    assert!(common > 0, "test paths must share a filesystem root");
    let mut result = PathBuf::new();
    for component in &from[common..] {
        assert!(matches!(component, Component::Normal(_)));
        result.push("..");
    }
    for component in &to[common..] {
        result.push(component.as_os_str());
    }
    result
}
