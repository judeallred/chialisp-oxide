//! Real Cargo/SDK ownership tests. Native cases are bounded over the whole
//! input tree before constructing boxes; nontermination runs only in CLVM.
mod support;
#[path = "support/vm.rs"]
mod vm;

use chialisp_oxide_backend_experiment::{FailureCode, Outcome};
use chialisp_oxide_ir::{Environment, Program, Type};
use std::{path::Path, process::Command};
use support::Fixture;

fn fixture(source: &str) -> Fixture {
    let fixture = Fixture::new();
    let manifest = fixture.0.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    let marker = "package=\"chialisp-oxide\",";
    assert_eq!(text.matches(marker).count(), 1);
    std::fs::write(
        manifest,
        text.replace(marker, "package=\"chialisp-oxide\",features=[\"alloc\"],"),
    )
    .unwrap();
    fixture.source(source);
    fixture
}

fn success(output: std::process::Output) {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn cargo(fixture: &Fixture, args: &[&str]) -> std::process::Output {
    Command::new(fixture.compiler().parent().unwrap().join("cargo"))
        .current_dir(&fixture.0)
        .env("RUSTC", fixture.compiler())
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CHIALISP_OXIDE_CARGO_MODE")
        .args(args)
        .args(["--offline", "--target-dir"])
        .arg(fixture.0.join("native-build"))
        .output()
        .unwrap()
}

fn find<'a>(programs: &'a [Program], name: &str) -> &'a Program {
    programs
        .iter()
        .find(|p| p.name.ends_with(&format!("::{name}")))
        .unwrap()
}

fn expected(value: &str) -> Outcome {
    vm::execute(&[1], value, 10_000_000).unwrap().outcome
}

fn native(oracle: &Path, name: &str, solution: &str) -> Outcome {
    let Outcome::Accepted { output_hex } = expected(solution) else {
        unreachable!()
    };
    let output = Command::new(oracle)
        .args([name, &output_hex])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "native oracle host error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let text = text.trim();
    match text.strip_prefix("accepted ") {
        Some(hex) => Outcome::Accepted {
            output_hex: hex.into(),
        },
        None => {
            assert_eq!(text, "rejected");
            Outcome::Rejected
        }
    }
}

#[test]
fn actual_cargo_owned_prefix_moves_and_shared_borrows_match_native_and_chialisp() {
    let fixture = fixture(include_str!("owned_trees/library.rs"));
    std::fs::write(
        fixture.0.join("src/native.rs"),
        include_str!("owned_trees/native.rs"),
    )
    .unwrap();
    std::fs::create_dir(fixture.0.join("src/bin")).unwrap();
    std::fs::write(
        fixture.0.join("src/bin/oracle.rs"),
        include_str!("owned_trees/oracle.rs"),
    )
    .unwrap();
    success(cargo(&fixture, &["test", "--lib"]));
    success(cargo(&fixture, &["build", "--bin", "oracle"]));
    let oracle = fixture.0.join("native-build/debug/oracle");
    success(fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 20);
    for program in &programs {
        program.validate().unwrap();
    }
    assert!(find(&programs, "prefix").uses_runtime());
    assert!(find(&programs, "runtime_snapshot").uses_runtime());
    assert_eq!(find(&programs, "prefix").environment, Environment::Strict);
    assert!(
        find(&programs, "prefix")
            .parameters
            .iter()
            .all(|p| p.ty == Type::Tree)
    );
    assert_eq!(find(&programs, "construct").environment, Environment::Raw);
    assert!(!find(&programs, "construct").uses_runtime());

    // Independently compiled source reference. This is a semantic test module,
    // not a canonical-source or module-identity claim; malformed ABI is separate.
    let reference = chialisp_oxide_reference::compile(
        "(mod (L P) (defun prefix (L P) (if L (c (f L) (prefix (r L) P)) P)) (prefix L P))",
    )
    .unwrap();
    let heads = [
        "()",
        "0x00",
        "0x0001",
        "0xff",
        "(8)",
        "(2 1)",
        "(0x00 . 0x0001)",
    ];
    let tails = [
        "()",
        "0x00",
        "0x0001",
        "0xff",
        "(1 . 2)",
        "(2 1)",
        "((1) . 0x0000)",
    ];
    let mut cases = vec![];
    for tail in tails {
        cases.push((format!("(() {tail})"), expected(tail)));
        for head in heads {
            cases.push((
                format!("(({head}) {tail})"),
                expected(&format!("({head} . {tail})")),
            ));
        }
        cases.push((
            format!("((0x00 (1 . 2) 0x0001) {tail})"),
            expected(&format!("(0x00 (1 . 2) 0x0001 . {tail})")),
        ));
    }
    for malformed_list in ["0x00", "0x0000", "0x0001", "1", "(1 . 0x00)", "(1 2 . 3)"] {
        cases.push((format!("({malformed_list} (2 1))"), Outcome::Rejected));
    }
    for (solution, wanted) in &cases {
        assert_eq!(
            &native(&oracle, "prefix", solution),
            wanted,
            "native {solution}"
        );
    }
    for optimize in [false, true] {
        let code = chialisp_oxide_ir::compile(find(&programs, "prefix"), optimize).unwrap();
        for (solution, wanted) in &cases {
            let actual = vm::execute(&code, solution, 100_000_000).unwrap();
            assert_eq!(
                &actual.outcome, wanted,
                "prefix optimize={optimize}: {solution}"
            );
            if actual.outcome == Outcome::Rejected {
                assert_eq!(actual.failure.unwrap().code(), FailureCode::Raise);
            }
            assert_eq!(
                &vm::execute(&reference, solution, 100_000_000)
                    .unwrap()
                    .outcome,
                wanted,
                "Chialisp {solution}"
            );
        }
        for malformed in ["()", "(())", "(() () 1)", "(() () . 0x00)", "(() . 1)", "1"] {
            assert_eq!(
                vm::execute(&code, malformed, 1_000_000).unwrap().outcome,
                Outcome::Rejected,
                "strict owned ABI {malformed}"
            );
        }
        let raw = ["()", "0x00", "0x0001", "(1 . 2)", "((2 1) . 0x0000)"];
        for name in ["head", "tail", "construct", "boxed_identity"] {
            let code = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            for input in raw {
                assert_eq!(
                    vm::execute(&code, input, 10_000_000).unwrap().outcome,
                    native(&oracle, name, input),
                    "{name}: {input}"
                );
            }
        }
        for name in [
            "ignored_owned",
            "misleading_name",
            "borrow_then_move",
            "shared_box_then_move",
            "structural",
        ] {
            let code = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            for (input, wanted) in [("0x0001", "0x0001"), ("(0x00 . 0xff)", "(0xff . 0x00)")] {
                let wanted = match name {
                    "ignored_owned" => "7".into(),
                    "misleading_name" => "2".into(),
                    "borrow_then_move" | "shared_box_then_move" => format!(
                        "({} {input})",
                        if input.starts_with('(') { "1" } else { "()" }
                    ),
                    _ => wanted.into(),
                };
                assert_eq!(
                    vm::execute(&code, input, 10_000_000).unwrap().outcome,
                    expected(&wanted),
                    "{name}: {input}"
                );
            }
        }
        for name in ["snapshot", "runtime_snapshot"] {
            let snapshot = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            for input in ["()", "0x00", "0x0000", "0x0001", "0xff"] {
                assert_eq!(
                    vm::execute(&snapshot, input, 1_000_000).unwrap().outcome,
                    expected(input),
                    "{name} after move/reassignment: {input}"
                );
            }
            let failed = vm::execute(&snapshot, "(1 . 2)", 1_000_000).unwrap();
            assert_eq!(failed.outcome, Outcome::Rejected);
            assert_eq!(failed.failure.unwrap().code(), FailureCode::Raise);
        }
        for (name, input, failure) in [
            ("unused_constructor", "()", true),
            ("unused_constructor", "0x00", false),
            ("unused_overflow", "(255)", true),
            ("unused_overflow", "(254)", false),
            ("lazy", "(1 255)", true),
            ("lazy", "(() 255)", false),
        ] {
            let code = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            let actual = vm::execute(&code, input, 1_000_000).unwrap();
            assert_eq!(
                actual.outcome,
                if failure {
                    Outcome::Rejected
                } else {
                    expected("7")
                },
                "{name}: {input}"
            );
            if failure {
                assert_eq!(actual.failure.unwrap().code(), FailureCode::Raise);
            }
        }
        for name in [
            "unused_spin",
            "spin_before_reject",
            "reject_before_spin",
            "unused_spin_then_reject",
            "unused_reject_then_spin",
        ] {
            let code = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            let actual =
                chialisp_oxide_backend_experiment::execute(&code, "0x0001", 100_000).unwrap();
            let rejects_first = matches!(name, "reject_before_spin" | "unused_reject_then_spin");
            let code = if rejects_first {
                FailureCode::Raise
            } else {
                FailureCode::CostExceeded
            };
            assert_eq!(
                actual.outcome,
                if rejects_first {
                    Outcome::Rejected
                } else {
                    Outcome::Inconclusive
                }
            );
            assert_eq!(actual.failure.unwrap().code(), code);
        }
    }

    // These inputs are rejected by the host BEFORE constructing owned boxes.
    // Deep first children exercise a different path from a list-tail limit.
    let mut deep_first = "80".to_string();
    let mut deep_rest = "80".to_string();
    for _ in 0..130 {
        deep_first = format!("ff{deep_first}80");
        deep_rest = format!("ff80{deep_rest}");
    }
    let mut wide = "80".to_string();
    for _ in 0..12 {
        wide = format!("ff{wide}{wide}");
    }
    for input in [deep_first, deep_rest, wide] {
        let output = Command::new(&oracle)
            .args(["boxed_identity", &input])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("host-inconclusive"));
        assert!(
            output.stdout.is_empty(),
            "host bound cannot report policy rejection"
        );
    }
}

#[test]
fn unsupported_owned_box_uses_fail_after_native_rust_check_without_partial_exports() {
    let cases = [
        "use chialisp_oxide::{OwnedTree,Tree}; #[chialisp_oxide::puzzle]pub fn bad(v:Box<Tree<'_>>)->u8 {let _=v;7}",
        "use chialisp_oxide::{OwnedTree,Tree}; #[chialisp_oxide::puzzle]pub fn bad()->u8 {let _=Box::new(Tree::Atom(&[]));7}",
        "use chialisp_oxide::OwnedTree; #[chialisp_oxide::puzzle]pub fn bad(v:Box<Box<OwnedTree<'_>>>)->u8 {let _=v;7}",
        "use chialisp_oxide::OwnedTree; #[chialisp_oxide::puzzle]pub fn bad(v:OwnedTree<'_>)->u8 {std::mem::forget(v);7}",
        "use chialisp_oxide::OwnedTree; #[chialisp_oxide::puzzle]pub fn bad(v:OwnedTree<'_>)->u8 {let _=Box::into_raw(Box::new(v));7}",
        "use chialisp_oxide::OwnedTree; #[chialisp_oxide::puzzle]pub fn bad<'a>(v:OwnedTree<'a>)->&'a OwnedTree<'a> {Box::leak(Box::new(v))}",
        "use chialisp_oxide::OwnedTree; #[chialisp_oxide::puzzle]pub fn bad(a:&OwnedTree<'_>,b:&OwnedTree<'_>)->bool {std::ptr::eq(a,b)}",
        "use chialisp_oxide::OwnedTree; #[chialisp_oxide::puzzle]pub fn bad(v:&mut OwnedTree<'_>)->u8 {let _=v;7}",
        "pub enum OwnedTree<'a>{Atom(&'a[u8]),Pair(Box<Self>,Box<Self>)} impl Drop for OwnedTree<'_>{fn drop(&mut self){panic!()}} #[chialisp_oxide::puzzle]pub fn bad(v:OwnedTree<'_>)->u8 {let _=v;7}",
        "pub enum OwnedTree<'a>{Atom(&'a[u8]),Pair(Box<Self>,Box<Self>),Borrowed(&'a Self)} #[chialisp_oxide::puzzle]pub fn bad(v:OwnedTree<'_>)->u8 {let _=v;7}",
        "#[repr(C)]pub enum OwnedTree<'a>{Atom(&'a[u8]),Pair(Box<Self>,Box<Self>)} #[chialisp_oxide::puzzle]pub fn bad(v:OwnedTree<'_>)->u8 {let _=v;7}",
        "use chialisp_oxide::OwnedTree; pub struct Box<T>(T); impl<T> Box<T>{fn new(v:T)->Self{Self(v)}} #[chialisp_oxide::puzzle]pub fn bad(v:OwnedTree<'_>)->OwnedTree<'_>{Box::new(v).0}",
    ];
    for source in cases {
        let fixture = fixture(&format!(
            "{source}\n#[chialisp_oxide::puzzle]pub fn good()->u8{{7}}"
        ));
        success(fixture.cargo(false));
        let output = fixture.cargo(true);
        assert!(
            !output.status.success(),
            "unsupported owned operation compiled: {source}"
        );
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(
            errors.contains("unsupported")
                || errors.contains("require")
                || errors.contains("supports only")
                || errors.contains("only local or chialisp_oxide"),
            "unhelpful diagnostic: {errors}"
        );
        assert!(!errors.contains("internal compiler error"), "{errors}");
        assert!(fixture.exports().is_empty(), "partial exports: {source}");
    }
    let fixture = fixture(
        "#![feature(allocator_api)] use chialisp_oxide::OwnedTree; #[chialisp_oxide::puzzle]pub fn bad(v:Box<OwnedTree<'_>,std::alloc::System>)->u8{let _=v;7}",
    );
    success(fixture.cargo(false));
    assert!(!fixture.cargo(true).status.success());
    assert!(fixture.exports().is_empty());
}

#[test]
fn actual_rust_ownership_errors_precede_owned_ir_extraction() {
    for (source, code) in [
        (
            "use chialisp_oxide::OwnedTree; fn consume(v:OwnedTree<'_>)->OwnedTree<'_>{v} #[chialisp_oxide::puzzle]pub fn bad(v:OwnedTree<'_>)->OwnedTree<'_>{let _=consume(v);v}",
            "E0382",
        ),
        (
            "use chialisp_oxide::OwnedTree; fn consume(_:OwnedTree<'_>){} #[chialisp_oxide::puzzle]pub fn bad<'a>(v:OwnedTree<'a>)->&'a OwnedTree<'a>{let borrow=&v;consume(v);borrow}",
            "E0505",
        ),
        (
            "use chialisp_oxide::OwnedTree; #[chialisp_oxide::puzzle]pub fn bad<'a>()->&'a OwnedTree<'a>{let local=OwnedTree::Atom(&[]);&local}",
            "E0515",
        ),
    ] {
        let fixture = fixture(source);
        for wrapped in [false, true] {
            let output = fixture.cargo(wrapped);
            assert!(!output.status.success());
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            assert!(
                diagnostic.contains(code),
                "expected native {code}: {diagnostic}"
            );
            assert!(!diagnostic.contains("internal compiler error"));
        }
        assert!(fixture.exports().is_empty());
    }
}

#[test]
fn a_loaded_dependency_global_allocator_is_not_silently_erased() {
    let fixture = fixture(
        "extern crate allocator_choice; use chialisp_oxide::OwnedTree; #[chialisp_oxide::puzzle(environment=\"raw\")]pub fn entry(value:OwnedTree<'_>)->OwnedTree<'_>{value}",
    );
    let dependency = fixture.0.join("allocator-choice");
    std::fs::create_dir_all(dependency.join("src")).unwrap();
    std::fs::write(
        dependency.join("Cargo.toml"),
        "[package]\nname=\"allocator-choice\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    )
    .unwrap();
    std::fs::write(
        dependency.join("src/lib.rs"),
        "#[global_allocator] static ALLOC:std::alloc::System=std::alloc::System;\n",
    )
    .unwrap();
    let manifest = fixture.0.join("Cargo.toml");
    let mut text = std::fs::read_to_string(&manifest).unwrap();
    text.push_str("allocator_choice={package=\"allocator-choice\",path=\"allocator-choice\"}\n");
    std::fs::write(manifest, text).unwrap();
    success(fixture.cargo(false));
    let output = fixture.cargo(true);
    assert!(
        !output.status.success(),
        "owned export accepted a loaded allocator override"
    );
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(errors.contains("global allocator"), "{errors}");
    assert!(!errors.contains("internal compiler error"));
    assert!(fixture.exports().is_empty());

    // The restriction belongs to the new owned-Box abstraction. An unrelated
    // host allocator must not disable existing pointer-free scalar/borrowed IR.
    fixture.source("extern crate allocator_choice; use chialisp_oxide::Tree; #[chialisp_oxide::puzzle]pub fn scalar(value:u8)->u8{value} #[chialisp_oxide::puzzle(environment=\"raw\")]pub fn borrowed<'a>(value:&'a Tree<'a>)->Tree<'a>{*value}");
    success(fixture.cargo(false));
    success(fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 2);
    for optimize in [false, true] {
        for (name, input, wanted) in [("scalar", "(7)", "7"), ("borrowed", "0x0001", "0x0001")] {
            let code = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            assert_eq!(
                vm::execute(&code, input, 1_000_000).unwrap().outcome,
                expected(wanted)
            );
        }
    }
}
