//! Actual rustc count-bound control; this is not a CLVM policy rejection.
mod support;
use std::process::Command;
use support::Fixture;

#[test]
fn captured_export_count_is_checked_before_any_mir_lowering_or_ir_output() {
    let fixture = Fixture::new();
    let division_source =
        "#[doc=\"__oxide_puzzle_v1\"]pub fn first_division(value:u64)->u64 {value/3}\n";
    let mut source = division_source.to_owned();
    for index in 1..1025 {
        source.push_str(&format!(
            "#[doc=\"__oxide_puzzle_v1\"]pub fn export_{index}(value:u64)->u64 {{value}}\n"
        ));
    }
    fixture.source(&source);
    let root = fixture.0.canonicalize().unwrap();
    let library = root.join("src/lib.rs");
    let native_output = root.join("native-output");
    std::fs::create_dir(&native_output).unwrap();
    let native = Command::new(fixture.compiler())
        .args([
            "--crate-name",
            "wrapper_fixture",
            "--crate-type",
            "lib",
            "--edition",
            "2024",
            "--emit=metadata",
            "--out-dir",
        ])
        .arg(&native_output)
        .arg(&library)
        .output()
        .unwrap();
    assert!(
        native.status.success(),
        "native positive control: {}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert!(native_output.join("libwrapper_fixture.rmeta").is_file());

    let exports = root.join("exports");
    let metadata = root.join("wrapped-output");
    std::fs::create_dir(&exports).unwrap();
    std::fs::create_dir(&metadata).unwrap();
    let run_checked = || {
        Command::new(env!("CARGO_BIN_EXE_chialisp-oxide-driver"))
            .arg("--cargo-wrapper")
            .arg(fixture.compiler())
            .args([
                "--crate-name",
                "wrapper_fixture",
                "--crate-type",
                "lib",
                "--edition",
                "2024",
                "--emit=metadata",
                "--target",
            ])
            .arg(fixture.host())
            .arg("--out-dir")
            .arg(&metadata)
            .arg(&library)
            .env("CARGO_PRIMARY_PACKAGE", "1")
            .env("CARGO_MANIFEST_DIR", &root)
            .env("CHIALISP_OXIDE_PRIMARY_PACKAGES", "wrapper_fixture")
            .env("CHIALISP_OXIDE_OUTPUT_DIR", &exports)
            .env("CHIALISP_OXIDE_SOURCE_RECEIPT", "1")
            .env("CHIALISP_OXIDE_SOURCE_PACKAGE_ROOT", &root)
            .env("CHIALISP_OXIDE_SOURCE_LIBRARY", &library)
            .env_remove("OUT_DIR")
            .output()
            .unwrap()
    };
    let checked = run_checked();
    assert!(!checked.status.success());
    assert!(checked.stdout.is_empty());
    let errors = String::from_utf8_lossy(&checked.stderr);
    assert!(
        errors.contains(
            "ResourceLimit: declared source receipt export-count limit exceeded before lowering"
        ),
        "{errors}"
    );
    assert!(
        !errors.contains("unsupported integer operation"),
        "lowering preceded count preflight: {errors}"
    );
    assert!(std::fs::read_dir(&exports).unwrap().next().is_none());

    // Prove that this same first body really reaches unsupported division when
    // the export count permits lowering; its rejection is not assumed from text.
    fixture.source(division_source);
    let under_limit = run_checked();
    assert!(!under_limit.status.success());
    assert!(under_limit.stdout.is_empty());
    let errors = String::from_utf8_lossy(&under_limit.stderr);
    assert!(errors.contains("unsupported integer operation"), "{errors}");
    assert!(!errors.contains("export-count limit"), "{errors}");
    assert!(std::fs::read_dir(&exports).unwrap().next().is_none());
}
