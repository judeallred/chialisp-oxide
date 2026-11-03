//! Genuine Cargo plugin publication and offline admission of cast-bearing exports.
use chialisp_oxide_backend_experiment::{Outcome, execute};
use chialisp_oxide_package::{Limits, bundle::admit_bundle};
use std::{path::Path, process::Command};

fn success(command: &mut Command) {
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn normal_cargo_build_retains_v7_sources_and_exact_cast_outputs_in_both_modes() {
    let temporary = tempfile::tempdir().unwrap();
    let scratch = temporary.path().canonicalize().unwrap();
    let consumer = scratch.join("consumer");
    std::fs::create_dir_all(consumer.join("src")).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    std::fs::write(
        consumer.join("Cargo.toml"),
        format!(
            "[package]\nname=\"cast-consumer\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n[dependencies]\nchialisp-oxide={{path={}}}\n",
            serde_json::to_string(&root.join("crates/chialisp-oxide")).unwrap()
        ),
    )
    .unwrap();
    let source = r#"
#[chialisp_oxide::puzzle]
pub fn narrow(value: i16) -> u8 { value as u8 }
#[chialisp_oxide::puzzle]
pub fn widen(value: i8) -> u128 { value as u128 }
#[chialisp_oxide::puzzle]
pub fn combined(value: i16) -> chialisp_oxide::Cons<u8, Option<u8>> {
    chialisp_oxide::Cons { first: value as u8, rest: Some(value as u8) }
}
#[cfg(test)] mod tests {
    #[test] fn native_outputs() {
        assert_eq!(super::narrow(-1), 255);
        assert_eq!(super::narrow(256), 0);
        assert_eq!(super::widen(-1), u128::MAX);
        let output = super::combined(256);
        assert_eq!(output.first, 0);
        assert_eq!(output.rest, Some(0));
    }
}
"#;
    std::fs::write(consumer.join("src/lib.rs"), source).unwrap();
    let pin = include_str!("../../../rust-toolchain.toml")
        .lines()
        .find_map(|line| line.strip_prefix("channel = "))
        .unwrap()
        .trim_matches('"');
    success(
        Command::new("rustup")
            .args([
                "run",
                pin,
                "cargo",
                "test",
                "--lib",
                "--offline",
                "--manifest-path",
            ])
            .arg(consumer.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", scratch.join("native"))
            .env_remove("RUSTC")
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER"),
    );
    let executable = Path::new(env!("CARGO_BIN_EXE_cargo-chialisp-oxide"));
    let driver = executable.with_file_name("chialisp-oxide-driver");
    assert!(
        driver.is_file(),
        "build the workspace before this integration test"
    );
    let search_path = std::env::join_paths(
        std::iter::once(executable.parent().unwrap().to_path_buf())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    for optimized in [false, true] {
        let output = scratch.join(if optimized { "on" } else { "off" });
        let mut build = Command::new("rustup");
        build
            .args([
                "run",
                pin,
                "cargo",
                "chialisp-oxide",
                "build",
                "--manifest-path",
            ])
            .arg(consumer.join("Cargo.toml"))
            .args(["--locked", "--offline", "--output-dir"])
            .arg(&output)
            .env("PATH", &search_path)
            .env("CARGO_TARGET_DIR", scratch.join("compiled"))
            .env("CHIALISP_OXIDE_DRIVER", &driver)
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER");
        if !optimized {
            build.arg("--no-optimize");
        }
        success(&mut build);
        let bundle = cargo_chialisp_oxide::load_bundle(&output).unwrap();
        assert_eq!(bundle.manifest["schema_version"], 2);
        let admitted = admit_bundle(&bundle.manifest_path, &Limits::default()).unwrap();
        assert_eq!(admitted.entries().len(), 3);
        assert!(admitted.source_receipt().is_some());
        for entry in admitted.entries().values() {
            assert!(entry.program().uses_casts());
            assert_eq!(
                serde_json::to_value(entry.program()).unwrap()["ir_version"],
                7
            );
        }
        for (name, input, expected) in [
            ("narrow", "(-1)", "8200ff"),
            ("narrow", "(128)", "820080"),
            ("narrow", "(256)", "80"),
            ("widen", "(-1)", "9100ffffffffffffffffffffffffffffffff"),
            ("combined", "(256)", "ff80ff8080"),
        ] {
            let entry = &admitted.entries()[&format!("cast_consumer::{name}")];
            assert_eq!(
                execute(entry.artifact().bytes(), input, 10_000_000)
                    .unwrap()
                    .outcome,
                Outcome::Accepted {
                    output_hex: expected.into()
                },
                "{name} {input}; optimized={optimized}"
            );
        }
        let combined = admitted.entries()["cast_consumer::combined"].program();
        assert!(combined.uses_pairs() && combined.uses_optional());
        // The actual selected-library snapshot is retained, not an inferred
        // build graph or a claim that source hashes authenticate execution.
        assert!(
            admitted
                .sources()
                .iter()
                .any(|file| file.content_hex == hex::encode(source))
        );
    }
}
