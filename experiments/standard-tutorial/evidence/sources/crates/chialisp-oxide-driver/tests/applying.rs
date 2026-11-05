//! Production-path Cargo admission of the reserved `apply` export: opcode 2 with
//! strict operand bindings, failure propagation, lazy branches and a same-name
//! control. Native execution uses the SDK's `vm` feature as a dev-dependency only.
mod support;
#[path = "support/vm.rs"]
mod vm;
use chialisp_oxide_backend_experiment::Outcome;
use chialisp_oxide_ir::Program;
use std::{
    path::Path,
    process::{Command, Output},
};
use support::Fixture;

fn success(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn native(fixture: &Fixture) {
    success(
        Command::new(Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/cargo"))
            .current_dir(&fixture.0)
            .env("RUSTC", fixture.compiler())
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER")
            .env_remove("CHIALISP_OXIDE_CARGO_MODE")
            .args(["test", "--lib", "--offline", "--target-dir"])
            .arg(fixture.0.join("native-build"))
            .output()
            .unwrap(),
    );
}
fn find<'a>(programs: &'a [Program], name: &str) -> &'a Program {
    programs
        .iter()
        .find(|p| p.name.ends_with(&format!("::{name}")))
        .unwrap()
}
fn accepted(hex: &str) -> Outcome {
    Outcome::Accepted {
        output_hex: hex.into(),
    }
}
/// The fixture's native tests need the SDK evaluator; extraction must not.
fn enable_native_vm(fixture: &Fixture, alias: &str) {
    let manifest = fixture.0.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    let dependency = text
        .lines()
        .find(|line| line.starts_with(&format!("{alias}=")))
        .unwrap()
        .replace("}}", ",features=[\"vm\"]}}")
        .replace("}", ",features=[\"vm\"]}");
    std::fs::write(
        &manifest,
        format!("{text}[dev-dependencies]\n{dependency}\n"),
    )
    .unwrap();
}

#[test]
fn actual_cargo_applies_untrusted_programs_with_strict_bindings() {
    let fixture = Fixture::with_sdk_alias("inner_api");
    let source = include_str!("applying/library.rs").replace("chialisp_oxide", "inner_api");
    fixture.source(&source);
    enable_native_vm(&fixture, "inner_api");
    native(&fixture);
    success(fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 6);
    for name in ["delegate", "nested", "unused", "lazy", "apply_then_reject"] {
        let program = find(&programs, name);
        assert!(
            program.uses_runtime(),
            "apply requires strict bindings: {name}"
        );
        assert!(program.uses_apply(), "apply node expected: {name}");
    }
    assert!(!find(&programs, "fake_apply").uses_apply());
    for optimize in [false, true] {
        let code =
            |name: &str| chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
        let run = |name: &str, solution: &str| {
            vm::execute(&code(name), solution, 10_000_000)
                .unwrap()
                .outcome
        };
        // (q . 7) ignores its environment; 1 returns the environment itself.
        assert_eq!(run("delegate", "((1 . 7) ())"), accepted("07"));
        assert_eq!(run("delegate", "(1 (7))"), accepted("ff0780"));
        assert_eq!(
            run("delegate", "((1 . (51 0x11 100)) (ignored))"),
            accepted("ff33ff11ff6480")
        );
        // (+ 2 5) adds the environment's first two elements.
        assert_eq!(run("delegate", "((16 2 5) (20 22))"), accepted("2a"));
        // nested: the produced program (q . 7) is then applied again.
        assert_eq!(run("nested", "((1 1 . 7) ())"), accepted("07"));
        // A raising program rejects even when its result is unused.
        assert_eq!(run("delegate", "((8) ())"), Outcome::Rejected);
        assert_eq!(run("unused", "((8) ())"), Outcome::Rejected);
        assert_eq!(run("unused", "((1 . 7) ())"), accepted("07"));
        // Lazy: the failing program sits in the unchosen arm.
        assert_eq!(run("lazy", "(() (8) ())"), accepted("09"));
        assert_eq!(run("lazy", "(1 (8) ())"), Outcome::Rejected);
        assert_eq!(run("lazy", "(1 (1 . 7) ())"), accepted("07"));
        // The application happens before the later shape assertion.
        assert_eq!(
            run("apply_then_reject", "((1 7 8) ())"),
            accepted("ff07ff0880")
        );
        assert_eq!(run("apply_then_reject", "((1 . 7) ())"), Outcome::Rejected);
        assert_eq!(run("apply_then_reject", "((8) ())"), Outcome::Rejected);
        // Strict ABI: both arguments are required, nothing more.
        assert_eq!(run("delegate", "((1 . 7))"), Outcome::Rejected);
        assert_eq!(run("delegate", "((1 . 7) () ())"), Outcome::Rejected);
        // The same-name helper keeps its real body.
        assert_eq!(run("fake_apply", "((8) ())"), accepted("08"));
    }
}

const REVIEWED_BODY: &str = "    panic!()\n";

/// A relocated, renamed crate exporting only the admitted tree shape and one
/// `apply` body. Identity and signature match the reviewed export; only the
/// canonical body fingerprint can distinguish the reviewed body from a change.
fn counterfeit(body: &str) -> Fixture {
    let fixture = Fixture::new();
    let sdk = fixture.0.join("relocated-reviewed-sdk");
    std::fs::create_dir_all(sdk.join("src")).unwrap();
    std::fs::write(
        sdk.join("Cargo.toml"),
        "[package]\nname=\"chialisp-oxide\"\nversion=\"0.7.0\"\nedition=\"2024\"\n",
    )
    .unwrap();
    std::fs::write(
        sdk.join("src/lib.rs"),
        format!(
            "#![no_std]\n#![allow(unused_variables)]\n#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub enum Tree<'a> {{\n    Atom(&'a [u8]),\n    Pair(&'a Tree<'a>, &'a Tree<'a>),\n}}\npub fn apply<'a>(program: &Tree<'a>, environment: &Tree<'a>) -> Tree<'static> {{\n{body}}}\n"
        ),
    )
    .unwrap();
    std::fs::write(fixture.0.join("Cargo.toml"),"[package]\nname=\"chialisp-oxide-driver-cargo-fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n[lib]\nname=\"wrapper_fixture\"\n[dependencies]\nrenamed={package=\"chialisp-oxide\",path=\"relocated-reviewed-sdk\"}\n").unwrap();
    fixture.source("#[doc=\"__oxide_puzzle_v1\"] pub fn entry<'a>(p: &'a renamed::Tree<'a>, e: &'a renamed::Tree<'a>) -> renamed::Tree<'static> { renamed::apply(p, e) }\n");
    fixture
}

#[test]
fn reserved_body_admission_is_relocation_safe_and_fails_closed_on_change() {
    let relocated = counterfeit(REVIEWED_BODY);
    success(relocated.cargo(true));
    let programs = relocated.exports();
    assert_eq!(programs.len(), 1);
    assert!(programs[0].uses_apply());
    for optimize in [false, true] {
        let code = chialisp_oxide_ir::compile(&programs[0], optimize).unwrap();
        assert_eq!(
            vm::execute(&code, "((1 . 7) ())", 10_000_000)
                .unwrap()
                .outcome,
            accepted("07")
        );
    }
    // An encodable but different body is a fingerprint mismatch.
    let looping = counterfeit("    loop {}\n");
    let output = looping.cargo(true);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("reviewed native apply primitive rejected")
            && stderr.contains("fingerprint mismatch"),
        "{stderr}"
    );
    // A body using constructs the encoder does not review fails closed too.
    let inspecting = counterfeit(
        "    if matches!(program, Tree::Atom(_)) {\n        return Tree::Atom(&[]);\n    }\n    panic!()\n",
    );
    let output = inspecting.cargo(true);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("reviewed native apply primitive rejected") && !stderr.contains("Built"),
        "{stderr}"
    );
}
