//! Native-valid toolchain relocations, parsed routing and publication controls.
#[path = "toolchain/harness.rs"]
mod harness;
mod support;

use harness::{Unit, extern_arg, relative, succeeds, text};
use std::{fs, process::Command};

#[test]
fn default_split_equals_relative_and_explicit_pinned_core_keep_working() {
    for mode in ["default", "split", "equals", "relative", "pinned-core"] {
        let unit = Unit::new(include_str!("toolchain/bytes.rs"));
        let pinned = unit.pinned();
        let args = match mode {
            "default" => vec![],
            "split" => vec!["--sysroot".into(), text(&pinned)],
            "equals" => vec![format!("--sysroot={}", pinned.display())],
            "relative" => vec![
                "--sysroot".into(),
                text(&relative(&unit.root().canonicalize().unwrap(), &pinned)),
            ],
            "pinned-core" => extern_arg("core", &unit.core("rmeta")),
            _ => unreachable!(),
        };
        unit.native(&args);
        unit.exports_one(unit.run(true, true, true, &args));
    }
}

#[cfg(unix)]
#[test]
fn canonical_symlink_alias_is_supported_but_relocated_sysroot_is_not() {
    use std::os::unix::fs::symlink;
    let alias = Unit::new(include_str!("toolchain/bytes.rs"));
    let link = alias.root().join("same-installed-toolchain");
    symlink(alias.pinned(), &link).unwrap();
    let args = vec!["--sysroot".into(), text(&link)];
    alias.native(&args);
    alias.exports_one(alias.run(true, true, true, &args));

    let moved = Unit::new(include_str!("toolchain/bytes.rs"));
    let relocated = moved.root().join("relocated-sysroot");
    fs::create_dir_all(&relocated).unwrap();
    // The complete library layout includes rustc's original codegen backend.
    // Only the sysroot identity differs; no library bytes are counterfeit.
    symlink(moved.pinned().join("lib"), relocated.join("lib")).unwrap();
    let args = vec![format!("--sysroot={}", relocated.display())];
    moved.native(&args);
    moved.rejects(moved.run(true, true, true, &args), "sysroot");
}

#[test]
fn copied_core_metadata_with_or_without_its_archive_is_not_supported() {
    for with_archive in [false, true] {
        let unit = Unit::new(include_str!("toolchain/bytes.rs"));
        let copied = unit.copied_core("rmeta");
        if with_archive {
            // The installed rlib is metadata-stub-only on this nightly. Supply
            // its real metadata, rather than counting native E0786 as a guard.
            unit.copied_core("rlib");
        }
        let args = extern_arg("core", &copied);
        unit.native(&args);
        unit.loaded(&copied);
        unit.rejects(unit.run(true, true, true, &args), "standard library");
    }
}

#[test]
fn renamed_extern_and_search_paths_deduplicate_to_pinned_core() {
    let alias = Unit::new(include_str!("toolchain/alias.rs"));
    let copied = alias.copied_core("rmeta");
    let args = extern_arg("replacement", &copied);
    alias.native(&args);
    alias.loaded(&alias.core("rmeta"));
    alias.not_loaded(&copied);
    alias.exports_one(alias.run(true, true, true, &args));

    for kind in ["dependency", "crate", "all"] {
        let searched = Unit::new(include_str!("toolchain/bytes.rs"));
        let copied = searched.copied_core("rmeta");
        let args = vec![
            "-L".into(),
            format!("{kind}={}", copied.parent().unwrap().display()),
        ];
        searched.native(&args);
        // This pinned rustc prefers its installed core for all three search
        // kinds. Keep that observed native behavior as a positive control.
        searched.loaded(&searched.core("rmeta"));
        searched.not_loaded(&copied);
        searched.exports_one(searched.run(true, true, true, &args));
    }
}

#[test]
fn transitive_identical_core_hash_resolves_to_pinned_metadata_for_target_units() {
    let unit = Unit::new(include_str!("toolchain/transitive.rs"));
    let copied = unit.copied_core("rmeta");
    let dependency = unit.root().join("dependency.rs");
    fs::write(&dependency, include_str!("toolchain/dependency.rs")).unwrap();
    let metadata = unit.root().join("libreplacement_dependency.rmeta");
    succeeds(
        Command::new(unit.fixture.compiler())
            .current_dir(unit.root())
            .arg(dependency)
            .args([
                "--crate-name=replacement_dependency",
                "--crate-type=lib",
                "--edition=2024",
                "--emit=metadata",
                "-o",
            ])
            .arg(&metadata)
            .args(extern_arg("core", &copied))
            .output()
            .unwrap(),
    );
    let mut args = extern_arg("replacement_dependency", &metadata);
    args.extend([
        "-L".into(),
        format!("dependency={}", copied.parent().unwrap().display()),
    ]);
    unit.native(&args);
    // Rustc resolves this same-hash transitive dependency back to pinned core.
    // It is a valid positive graph control, not evidence of a loaded replacement.
    unit.loaded(&unit.core("rmeta"));
    unit.not_loaded(&copied);
    succeeds(unit.run(true, false, true, &args));
    assert!(unit.fixture.exports().is_empty());
}

#[test]
fn reserved_standard_library_crate_names_and_custom_backend_fail_closed() {
    for name in ["core", "alloc", "std"] {
        let unit = Unit::new(include_str!("toolchain/bytes.rs"));
        let native = unit
            .command(false, true, name)
            .args(unit.args(false, true, name))
            .output()
            .unwrap();
        succeeds(native);
        let wrapped = unit
            .command(true, true, name)
            .args(unit.args(true, true, name))
            .output()
            .unwrap();
        unit.rejects(wrapped, "reserved crate");
    }
    let unit = Unit::new(include_str!("toolchain/bytes.rs"));
    let args = vec!["-Zcodegen-backend=llvm".into()];
    unit.native(&args);
    unit.rejects(unit.run(true, true, true, &args), "codegen backend");
}

#[test]
fn response_file_target_and_crate_identity_select_real_exports() {
    let unit = Unit::new(include_str!("toolchain/bytes.rs"));
    let args = unit.args(true, true, "wrapper_fixture");
    let response = unit.root().join("target.args");
    fs::write(&response, format!("{}\n", args.join("\n"))).unwrap();
    // Neither target nor crate-name occurs in the wrapper's raw argv.
    let output = unit
        .command(true, true, "wrapper_fixture")
        .arg(format!("@{}", response.display()))
        .output()
        .unwrap();
    unit.exports_one(output);
}

#[test]
fn response_file_sysroot_rejection_and_host_delegation_publish_no_ir() {
    let invalid = Unit::new(include_str!("toolchain/bytes.rs"));
    let mut args = invalid.args(true, true, "wrapper_fixture");
    args.push(format!(
        "--sysroot={}",
        invalid.root().join("missing-sysroot").display()
    ));
    let response = invalid.root().join("invalid.args");
    fs::write(&response, format!("{}\n", args.join("\n"))).unwrap();
    let output = invalid
        .command(true, true, "wrapper_fixture")
        .arg(format!("@{}", response.display()))
        .output()
        .unwrap();
    invalid.rejects(output, "sysroot");

    let host = Unit::new(include_str!("toolchain/host.rs"));
    let args = host.args(true, false, "wrapper_fixture");
    let response = host.root().join("host.args");
    fs::write(&response, format!("{}\n", args.join("\n"))).unwrap();
    let output = host
        .command(true, true, "wrapper_fixture")
        .arg(format!("@{}", response.display()))
        .output()
        .unwrap();
    succeeds(output);
    assert!(
        host.fixture.exports().is_empty(),
        "host unit published target IR"
    );
}

#[test]
fn response_expansion_is_once_so_a_nested_at_filename_remains_source_data() {
    let unit = Unit::new(include_str!("toolchain/bytes.rs"));
    fs::write(
        unit.root().join("@literal.rs"),
        include_str!("toolchain/bytes.rs"),
    )
    .unwrap();
    for wrapped in [false, true] {
        let mut args = unit.args(wrapped, true, "wrapper_fixture");
        let source = text(&unit.root().join("src/lib.rs"));
        let input = args.iter_mut().find(|arg| **arg == source).unwrap();
        *input = "@literal.rs".into();
        let response = unit.root().join(if wrapped {
            "wrapped.args"
        } else {
            "native.args"
        });
        fs::write(&response, format!("{}\n", args.join("\n"))).unwrap();
        let output = unit
            .command(wrapped, true, "wrapper_fixture")
            .arg(format!("@{}", response.display()))
            .output()
            .unwrap();
        if wrapped {
            unit.exports_one(output);
        } else {
            succeeds(output);
        }
    }
}

#[test]
fn print_modes_that_continue_compilation_cannot_bypass_selected_analysis() {
    for args in [
        vec!["--print=native-static-libs".into()],
        vec!["--print=link-args".into()],
        vec!["-Ctarget-cpu=help".into(), "-Ctarget-cpu=generic".into()],
    ] {
        let unit = Unit::new(include_str!("toolchain/host.rs"));
        unit.native(&args);
        let output = unit.run(true, true, true, &args);
        assert!(
            !output.status.success(),
            "{args:?} skipped selected analysis"
        );
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(errors.contains("unsupported integer operation"), "{errors}");
        assert!(!errors.contains("internal compiler error"), "{errors}");
        assert!(unit.fixture.exports().is_empty());
    }
}

#[test]
fn custom_target_json_with_the_host_stem_does_not_count_as_the_builtin_host() {
    let unit = Unit::new(include_str!("toolchain/bytes.rs"));
    let spec = Command::new(unit.fixture.compiler())
        .args([
            "-Zunstable-options",
            "--print=target-spec-json",
            "--target",
            &unit.host,
        ])
        .output()
        .unwrap();
    assert!(
        spec.status.success(),
        "{}",
        String::from_utf8_lossy(&spec.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&spec.stdout).unwrap();
    let path = unit.root().join(format!("{}.json", unit.host));
    fs::write(&path, spec.stdout).unwrap();
    let mut args = unit.args(true, false, "wrapper_fixture");
    args.extend(["--target".into(), text(&path), "-Zunstable-options".into()]);
    let output = unit
        .command(true, true, "wrapper_fixture")
        .args(args)
        .output()
        .unwrap();
    unit.rejects(output, "target");
}

#[test]
fn source_crate_name_cannot_hide_a_reserved_target_identity() {
    let unit = Unit::new(&format!(
        "#![crate_name=\"core\"]\n{}",
        include_str!("toolchain/bytes.rs")
    ));
    for wrapped in [false, true] {
        let mut args = unit.args(wrapped, true, "wrapper_fixture");
        drop(args.drain(..2)); // Let the parsed crate attribute choose the real name.
        let output = unit
            .command(wrapped, false, "wrapper_fixture")
            .args(args)
            .output()
            .unwrap();
        if wrapped {
            unit.rejects(output, "reserved crate");
        } else {
            succeeds(output);
        }
    }
}

#[test]
fn backend_aliases_and_early_print_options_reject_before_loading_a_plugin() {
    for spelling in ["codegen-backend", "codegen_backend"] {
        for early in ["-vV", "-Cpasses=list"] {
            let unit = Unit::new(include_str!("toolchain/bytes.rs"));
            let args = vec![
                format!(
                    "-Z{spelling}={}",
                    unit.root().join("never-load-this-backend").display()
                ),
                early.into(),
            ];
            let output = unit.run(true, true, true, &args);
            let errors = String::from_utf8_lossy(&output.stderr);
            assert!(
                !errors.contains("failed to load"),
                "backend was loaded before rejection: {errors}"
            );
            unit.rejects(output, "codegen backend");
        }
    }
}

#[test]
fn custom_target_default_backend_is_rejected_before_early_probe_loading() {
    for early in ["-vV", "-Cpasses=list"] {
        let unit = Unit::new(include_str!("toolchain/bytes.rs"));
        let output = Command::new(unit.fixture.compiler())
            .args([
                "-Zunstable-options",
                "--print=target-spec-json",
                "--target",
                &unit.host,
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut spec: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        spec.as_object_mut().unwrap().insert(
            "default-codegen-backend".into(),
            text(&unit.root().join("never-load-this-target-backend")).into(),
        );
        let path = unit.root().join(format!("{}.json", unit.host));
        fs::write(&path, serde_json::to_vec(&spec).unwrap()).unwrap();
        let mut args = unit.args(true, false, "wrapper_fixture");
        args.extend([
            "--target".into(),
            text(&path),
            "-Zunstable-options".into(),
            early.into(),
        ]);
        let output = unit
            .command(true, true, "wrapper_fixture")
            .args(args)
            .output()
            .unwrap();
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("failed to load"),
            "custom target loaded its backend before rejection"
        );
        unit.rejects(output, "target");
    }
}

#[cfg(unix)]
#[test]
fn native_valid_relocated_sysroot_is_rejected_before_early_backend_probes() {
    use std::os::unix::fs::symlink;
    for early in ["-vV", "-Cpasses=list"] {
        let unit = Unit::new(include_str!("toolchain/bytes.rs"));
        let relocated = unit.root().join("relocated-with-original-backend");
        fs::create_dir(&relocated).unwrap();
        symlink(unit.pinned().join("lib"), relocated.join("lib")).unwrap();
        let args = vec![format!("--sysroot={}", relocated.display()), early.into()];
        unit.native(&args);
        unit.rejects(unit.run(true, true, true, &args), "sysroot");
    }
}

#[test]
fn genuine_help_and_read_only_probe_modes_succeed_without_exporting_ir() {
    for flag in [
        "--help",
        "-Ahelp",
        "-Dhelp",
        "-Fhelp",
        "--force-warn=help",
        "-Zhelp",
        "-Ctarget-cpu=help",
        "-Ctarget-feature=help",
        "--print=sysroot",
    ] {
        let unit = Unit::new(include_str!("toolchain/host.rs"));
        let args = vec![flag.into()];
        unit.native(&args);
        succeeds(unit.run(true, true, true, &args));
        assert!(
            unit.fixture.exports().is_empty(),
            "{flag} published target IR"
        );
    }
}

#[test]
fn standalone_rejects_response_input_but_accepts_a_literal_at_filename() {
    let unit = Unit::new(include_str!("toolchain/bytes.rs"));
    let response = unit.root().join("never-compile.args");
    fs::write(
        &response,
        format!(
            "-vV\n--target={}\n-Zcodegen-backend={}\n",
            unit.host,
            unit.root().join("never-load-this-backend").display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_chialisp-oxide-driver"))
        .current_dir(unit.root())
        .env_remove("CHIALISP_OXIDE_CARGO_MODE")
        .arg(format!("@{}", response.display()))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(
        errors.contains("unsupported toolchain configuration"),
        "{errors}"
    );
    assert!(errors.contains("response"), "{errors}");
    assert!(!errors.contains("failed to load"), "{errors}");

    fs::write(
        unit.root().join("@literal.rs"),
        include_str!("toolchain/bytes.rs"),
    )
    .unwrap();
    let mut args = unit.args(false, true, "wrapper_fixture");
    let source = text(&unit.root().join("src/lib.rs"));
    *args.iter_mut().find(|arg| **arg == source).unwrap() = "./@literal.rs".into();
    succeeds(
        unit.command(false, true, "wrapper_fixture")
            .args(args)
            .output()
            .unwrap(),
    );
    let ir = unit.root().join("literal.ir.json");
    succeeds(
        Command::new(env!("CARGO_BIN_EXE_chialisp-oxide-driver"))
            .current_dir(unit.root())
            .env_remove("CHIALISP_OXIDE_CARGO_MODE")
            .args(["./@literal.rs", "--entry", "entry", "--output"])
            .arg(&ir)
            .output()
            .unwrap(),
    );
    let program: chialisp_oxide_ir::Program =
        serde_json::from_slice(&fs::read(ir).unwrap()).unwrap();
    program.validate().unwrap();
}

#[test]
fn metadata_only_target_dependencies_reject_actual_copied_core_sources() {
    let direct = Unit::new(include_str!("toolchain/bytes.rs"));
    let copied = direct.copied_core("rmeta");
    let args = extern_arg("core", &copied);
    direct.native(&args);
    direct.loaded(&copied);
    direct.rejects(direct.run(true, false, true, &args), "standard library");

    let graph = Unit::new(include_str!("toolchain/alias_without_injected_core.rs"));
    let copied = graph.copied_core("rmeta");
    let args = extern_arg("replacement", &copied);
    graph.native(&args);
    graph.loaded(&copied);
    // The renamed option is not a reserved extern. This case must reach the
    // loaded-graph guard in the metadata-only callback, before Continue.
    graph.rejects(graph.run(true, false, true, &args), "standard library");
}

#[test]
fn native_valid_reserved_extern_modifiers_and_spellings_keep_location_policy() {
    // These four modifiers are independently native-valid on the pinned
    // compiler with unstable-options. This is not an exhaustive flag claim.
    for modifier in ["", "priv", "noprelude", "nounused", "force"] {
        for equals in [false, true] {
            for copied in [false, true] {
                let unit = Unit::new(include_str!("toolchain/bytes.rs"));
                let library = if copied {
                    unit.copied_core("rmeta")
                } else {
                    unit.core("rmeta")
                };
                let name = if modifier.is_empty() {
                    "core".into()
                } else {
                    format!("{modifier}:core")
                };
                let value = format!("{name}={}", library.display());
                let mut args = vec!["-Zunstable-options".into()];
                if equals {
                    args.push(format!("--extern={value}"));
                } else {
                    args.extend(["--extern".into(), value]);
                }
                unit.native(&args);
                unit.loaded(&library);
                if copied {
                    unit.rejects(unit.run(true, true, true, &args), "standard library");
                } else {
                    unit.exports_one(unit.run(true, true, true, &args));
                }
            }
        }
    }
}
