//! Proper-list raw operand transport, independent fixed-arity Chialisp programs,
//! and explicit softfork guards. Native calls use the same VM oracle.
mod support;
#[path = "support/vm.rs"]
mod vm;
use chialisp_oxide_backend_experiment::{Execution, Outcome};
use clvmr_sdk::{Allocator, NodePtr, allocator::SExp, serde::node_from_bytes};
use serde_json::Value;
use std::{path::Path, process::Command};
use support::Fixture;

const HELPERS: &[(&str, &str)] = &[
    ("add_list", "+"),
    ("sub_list", "-"),
    ("mul_list", "*"),
    ("bitand_list", "logand"),
    ("bitor_list", "logior"),
    ("bitxor_list", "logxor"),
    ("concat_list", "concat"),
    ("sha256_list", "sha256"),
    ("any_list", "any"),
    ("all_list", "all"),
    ("point_add_list", "point_add"),
    ("g1_subtract_list", "g1_subtract"),
    ("g2_add_list", "g2_add"),
    ("g2_subtract_list", "g2_subtract"),
    ("pairing_identity_list", "bls_pairing_identity"),
    ("bls_verify_list", "bls_verify"),
];
const STRUCTURAL: &[(&str, &str, &[&str])] = &[
    (
        "cons",
        "(mod (A B) (c A B))",
        &["(() ())", "(0x00 (1 . 2))", "((8) 0x0001)"],
    ),
    (
        "first",
        "(mod (V) (f V))",
        &["((1 . 2))", "((() 0x00))", "(())", "(0x00)"],
    ),
    (
        "rest",
        "(mod (V) (r V))",
        &["((1 . 2))", "((() 0x00))", "(())", "(0x00)"],
    ),
    (
        "listp",
        "(mod (V) (l V))",
        &["((1 . 2))", "((8))", "(())", "(0x00)"],
    ),
    (
        "raise",
        "(mod (V) (x V))",
        &["((1 . 2))", "((8))", "(())", "(0x00)"],
    ),
];
fn softfork_cases() -> Vec<Value> {
    let data: Value =
        serde_json::from_str(include_str!("raw_clvm_softfork/softfork-vectors.json")).unwrap();
    data["cases"].as_array().unwrap().clone()
}
struct Case {
    helper: &'static str,
    arguments: String,
}
fn cases() -> Vec<Case> {
    let mut result = Vec::new();
    for (helper, _) in &HELPERS[..10] {
        let values = match *helper {
            "concat_list" | "sha256_list" => vec!["()", "(0x00)", "(0x0080ff () 0x00)", "((8))"],
            "any_list" | "all_list" => vec!["()", "(())", "(0x00)", "(() 0x00 (8))"],
            _ => vec![
                "()",
                "(-7)",
                "(0x0100000000000000000000000000000000 1 -3)",
                "(0x0001 2 3)",
                "((8))",
            ],
        };
        result.extend(values.into_iter().map(|arguments| Case {
            helper,
            arguments: arguments.into(),
        }));
    }
    let crypto: Value = serde_json::from_str(include_str!("raw_clvm_crypto/vectors.json")).unwrap();
    for (list, original) in [
        ("point_add_list", "point_add"),
        ("g1_subtract_list", "g1_subtract"),
        ("g2_add_list", "g2_add"),
        ("g2_subtract_list", "g2_subtract"),
        ("pairing_identity_list", "bls_pairing_identity_two"),
        ("bls_verify_list", "bls_verify"),
    ] {
        result.push(Case {
            helper: list,
            arguments: "()".into(),
        });
        for row in crypto["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["helper"].as_str() == Some(original))
        {
            result.push(Case {
                helper: list,
                arguments: row["arguments"].as_str().unwrap().into(),
            });
        }
    }
    let infinity1 = format!("0xc0{}", "00".repeat(47));
    let infinity2 = format!("0xc0{}", "00".repeat(95));
    for helper in ["point_add_list", "g1_subtract_list"] {
        for arguments in [
            format!("({infinity1})"),
            format!("({infinity1} {infinity1} {infinity1})"),
        ] {
            result.push(Case { helper, arguments });
        }
    }
    for helper in ["g2_add_list", "g2_subtract_list"] {
        for arguments in [
            format!("({infinity2})"),
            format!("({infinity2} {infinity2} {infinity2})"),
        ] {
            result.push(Case { helper, arguments });
        }
    }
    result.push(Case {
        helper: "pairing_identity_list",
        arguments: format!("({infinity1} {infinity2})"),
    });
    result.push(Case {
        helper: "pairing_identity_list",
        arguments: format!("({infinity1})"),
    });
    result.push(Case {
        helper: "bls_verify_list",
        arguments: format!("({infinity2})"),
    });
    result
}
fn literal(source: &str) -> Vec<u8> {
    let code = chialisp_oxide_reference::compile(&format!("(mod () (q . {source}))")).unwrap();
    let Outcome::Accepted { output_hex } = vm::execute(&code, "()", 100_000_000).unwrap().outcome
    else {
        panic!("literal must encode")
    };
    decode_hex(&output_hex)
}
fn decode_hex(encoded: &str) -> Vec<u8> {
    (0..encoded.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&encoded[i..i + 2], 16).unwrap())
        .collect()
}
fn rust_tree(a: &Allocator, node: NodePtr) -> String {
    match a.sexp(node) {
        SExp::Atom => format!("Tree::Atom(&{:?})", a.atom(node).as_ref()),
        SExp::Pair(f, r) => format!("Tree::Pair(&{},&{})", rust_tree(a, f), rust_tree(a, r)),
    }
}
fn rust_literal(bytes: &[u8]) -> String {
    let mut a = Allocator::new();
    let n = node_from_bytes(&mut a, bytes).unwrap();
    rust_tree(&a, n)
}
fn arity(arguments: &str) -> usize {
    let mut a = Allocator::new();
    let mut n = node_from_bytes(&mut a, &literal(arguments)).unwrap();
    let mut count = 0;
    while let SExp::Pair(_, r) = a.sexp(n) {
        count += 1;
        n = r;
    }
    assert!(a.atom(n).as_ref().is_empty());
    count
}
fn reference(case: &Case) -> Vec<u8> {
    // Only the declared empty BLS-verify case uses this separate runtime
    // reference. Its transport ABI remains `(VALUES)`, but its program ABI is
    // `(59 . VALUES)` applied to nil. VALUES is known empty in this case, so
    // there are no argument expressions to quote or execute. A direct
    // `(bls_verify)` is rejected by the Chialisp optimizer during compilation;
    // that compiler error is not VM-rejection evidence. This is an explicit
    // per-case reference recipe, never a fallback after a compilation failure.
    if case.helper == "bls_verify_list" && arity(&case.arguments) == 0 {
        assert_eq!(case.arguments, "()");
        return chialisp_oxide_reference::compile("(mod (VALUES) (a (c (q . 59) VALUES) ()))")
            .unwrap();
    }
    let operator = HELPERS.iter().find(|h| h.0 == case.helper).unwrap().1;
    let arguments = (0..arity(&case.arguments))
        .map(|i| {
            let mut path = "VALUES".to_string();
            for _ in 0..i {
                path = format!("(r {path})");
            }
            format!("(f {path})")
        })
        .collect::<Vec<_>>()
        .join(" ");
    chialisp_oxide_reference::compile(&format!("(mod (VALUES) ({operator} {arguments}))"))
        .unwrap_or_else(|error| {
            panic!(
                "reference compilation failed for {} {}: {error:#}",
                case.helper, case.arguments
            )
        })
}
fn rust_operands(source: &str) -> Vec<String> {
    let mut a = Allocator::new();
    let mut node = node_from_bytes(&mut a, &literal(source)).unwrap();
    let mut result = Vec::new();
    while let SExp::Pair(first, rest) = a.sexp(node) {
        result.push(rust_tree(&a, first));
        node = rest;
    }
    assert!(a.atom(node).as_ref().is_empty());
    result
}
fn native_source(cases: &[Case]) -> String {
    let mut source = String::from("use chialisp_oxide::{Tree,raw_clvm as raw};\n");
    for (helper, _) in HELPERS {
        source.push_str(&format!("#[chialisp_oxide::puzzle]pub fn {helper}<'a>(values:&'a Tree<'a>)->Tree<'static>{{raw::{helper}(values)}}\n"));
    }
    source.push_str("#[chialisp_oxide::puzzle]pub fn cons<'a>(a:&'a Tree<'a>,b:&'a Tree<'a>)->Tree<'static>{raw::cons(a,b)}\n");
    for helper in ["first", "rest", "listp", "raise"] {
        source.push_str(&format!("#[chialisp_oxide::puzzle]pub fn {helper}<'a>(v:&'a Tree<'a>)->Tree<'static>{{raw::{helper}(v)}}\n"));
    }
    source.push_str(r#"
#[chialisp_oxide::puzzle]pub fn guard<'a>(cost:&'a Tree<'a>,extension:&'a Tree<'a>,program:&'a Tree<'a>,environment:&'a Tree<'a>)->Tree<'static>{raw::softfork(cost,extension,program,environment)}
#[chialisp_oxide::puzzle]pub fn unused<'a>(v:&'a Tree<'a>)->u8{let _=raw::add_list(v);7}
#[chialisp_oxide::puzzle]pub fn lazy<'a>(flag:bool,v:&'a Tree<'a>)->Tree<'static>{if flag{raw::add_list(v)}else{Tree::Atom(&[7])}}
#[cfg(test)]mod native{use super::*;#[test]fn all_list_cases(){
"#);
    for case in cases {
        let solution = format!("({})", case.arguments);
        let expected = vm::execute(&reference(case), &solution, 100_000_000).unwrap();
        let call = format!(
            "{}(&{})",
            case.helper,
            rust_literal(&literal(&case.arguments))
        );
        match expected.outcome {
            Outcome::Accepted{output_hex}=>source.push_str(&format!("assert_eq!({call},{});\n",rust_literal(&decode_hex(&output_hex)))),
            Outcome::Rejected=>source.push_str(&format!("let e=std::panic::catch_unwind(||{call}).expect_err(\"raw operator must reject\");assert!(!e.is::<chialisp_oxide::ResourceExhausted>());\n")),
            Outcome::Inconclusive=>panic!("resource controls must stay separate"),
        }
    }
    for (helper, reference_source, solutions) in STRUCTURAL {
        let reference = chialisp_oxide_reference::compile(reference_source).unwrap();
        for solution in *solutions {
            let arguments = rust_operands(solution);
            let call = format!(
                "{helper}({})",
                arguments
                    .iter()
                    .map(|s| format!("&{s}"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            let expected = vm::execute(&reference, solution, 100_000_000).unwrap();
            match expected.outcome {
                Outcome::Accepted { output_hex } => source.push_str(&format!("assert_eq!({call},{});\n", rust_literal(&decode_hex(&output_hex)))),
                Outcome::Rejected => source.push_str(&format!("let e=std::panic::catch_unwind(||{call}).expect_err(\"raw shape must reject\");assert!(!e.is::<chialisp_oxide::ResourceExhausted>());\n")),
                Outcome::Inconclusive => panic!("resource control is separate"),
            }
        }
    }
    source.push_str("let e=std::panic::catch_unwind(||add_list(&Tree::Pair(&Tree::Atom(&[1]),&Tree::Atom(&[2])))).expect_err(\"proper list required\");assert!(!e.is::<chialisp_oxide::ResourceExhausted>());\n");
    for case in softfork_cases() {
        let args = rust_operands(case["arguments"].as_str().unwrap());
        assert_eq!(args.len(), 4);
        let call = format!(
            "guard({})",
            args.iter()
                .map(|v| format!("&{v}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        if let Some(expected) = case.get("expected_output") {
            source.push_str(&format!(
                "assert_eq!({call},{});\n",
                rust_literal(&literal(expected.as_str().unwrap()))
            ));
        } else if case["expected_failure"].as_str() == Some("CostExceeded") {
            source.push_str(&format!("let e=std::panic::catch_unwind(||{call}).expect_err(\"cost control must exhaust\");assert!(e.is::<chialisp_oxide::ResourceExhausted>());\n"));
        } else {
            source.push_str(&format!("let e=std::panic::catch_unwind(||{call}).expect_err(\"guard must reject\");assert!(!e.is::<chialisp_oxide::ResourceExhausted>());\n"));
        }
    }
    source.push_str("let e=std::panic::catch_unwind(||all_list(&Tree::Pair(&Tree::Atom(&[]),&Tree::Atom(&[1])))).expect_err(\"false does not skip malformed tail\");assert!(!e.is::<chialisp_oxide::ResourceExhausted>());\n");
    // Separate host-resource observation, not another native/VM equivalence
    // case: the Rust source is allowed to execute beyond this host-only bound.
    source.push_str(
        r#"
static HOST_BOUND_NIL: Tree<'static> = Tree::Atom(&[]);
let mut host_bound_input: &'static Tree<'static> = &HOST_BOUND_NIL;
for _ in 0..129 {
    host_bound_input = Box::leak(Box::new(Tree::Pair(&HOST_BOUND_NIL, host_bound_input)));
}
let host_bound_failure = std::panic::catch_unwind(|| add_list(host_bound_input))
    .expect_err("native-only 128-item resource bound");
assert!(host_bound_failure.is::<chialisp_oxide::ResourceExhausted>());
"#,
    );
    source.push_str("}}\n");
    source
}
fn fixture_with_vm(source: &str) -> Fixture {
    let fixture = Fixture::new();
    fixture.source(source);
    let manifest = fixture.0.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    let dependency = text
        .lines()
        .find(|l| l.starts_with("chialisp-oxide="))
        .unwrap()
        .replace('}', ",features=[\"vm\"]}");
    std::fs::write(
        &manifest,
        format!("{text}[dev-dependencies]\n{dependency}\n"),
    )
    .unwrap();
    fixture
}
fn run_native(fixture: &Fixture) {
    let o = Command::new(Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/cargo"))
        .current_dir(&fixture.0)
        .env("RUSTC", fixture.compiler())
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CHIALISP_OXIDE_CARGO_MODE")
        .args(["test", "--lib", "--offline", "--target-dir"])
        .arg(fixture.0.join("native-build"))
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
}
fn same(actual: &Execution, expected: &Execution, context: &str) {
    assert_eq!(actual.outcome, expected.outcome, "{context}");
    if actual.failure.is_some() {
        assert_eq!(
            actual.failure.map(|f| f.code()),
            expected.failure.map(|f| f.code()),
            "{context}"
        );
    }
}
#[test]
fn proper_operand_lists_match_independent_nary_programs() {
    let cases = cases();
    let fixture = fixture_with_vm(&native_source(&cases));
    run_native(&fixture);
    let o = fixture.cargo(true);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 24);
    let find = |name: &str| {
        programs
            .iter()
            .find(|p| p.name.ends_with(&format!("::{name}")))
            .unwrap()
    };
    for optimize in [false, true] {
        for case in &cases {
            let rust = chialisp_oxide_ir::compile(find(case.helper), optimize).unwrap();
            let solution = format!("({})", case.arguments);
            let actual = vm::execute(&rust, &solution, 100_000_000).unwrap();
            let expected = vm::execute(&reference(case), &solution, 100_000_000).unwrap();
            same(
                &actual,
                &expected,
                &format!(
                    "nary helper={} operands={} optimized={optimize}",
                    case.helper, case.arguments
                ),
            );
        }
        for (helper, source, solutions) in STRUCTURAL {
            let rust = chialisp_oxide_ir::compile(find(helper), optimize).unwrap();
            let reference = chialisp_oxide_reference::compile(source).unwrap();
            for solution in *solutions {
                let actual = vm::execute(&rust, solution, 100_000_000).unwrap();
                let canonical = vm::execute(&reference, solution, 100_000_000).unwrap();
                let context =
                    format!("structural export={helper} solution={solution} optimized={optimize}");
                if matches!(*helper, "first" | "rest") {
                    // The direct Chialisp source `(f V)` / `(r V)` optimizes
                    // to an environment path. On atoms that path fails with
                    // PathIntoAtom, whereas explicit opcodes 5/6 fail with
                    // InvalidOpArg. Preserve both measured typed observations.
                    // This independent runtime reference has transport ABI
                    // `(PROGRAM VALUE)`, applying supplied fixed `(5 1)` or
                    // `(6 1)` data to VALUE, so the compiler cannot rewrite the
                    // actual projection opcode into an environment path.
                    let opcode = if *helper == "first" { 5 } else { 6 };
                    let explicit_reference = chialisp_oxide_reference::compile(
                        "(mod (PROGRAM VALUE) (a PROGRAM VALUE))",
                    )
                    .unwrap();
                    let value = &solution[1..solution.len() - 1];
                    let explicit_solution = format!("(({opcode} 1) {value})");
                    let explicit =
                        vm::execute(&explicit_reference, &explicit_solution, 100_000_000).unwrap();
                    same(
                        &actual,
                        &explicit,
                        &format!("{context}; explicit opcode reference"),
                    );
                    assert_eq!(actual.outcome, canonical.outcome, "{context}");
                    if actual.outcome == Outcome::Rejected {
                        assert_eq!(
                            format!("{:?}", actual.failure.unwrap().code()),
                            "InvalidOpArg",
                            "{context}; explicit operator"
                        );
                        assert_eq!(
                            format!("{:?}", canonical.failure.unwrap().code()),
                            "PathIntoAtom",
                            "{context}; optimized Chialisp path"
                        );
                    }
                } else {
                    same(&actual, &canonical, &context);
                }
            }
        }
        for (helper, _) in HELPERS {
            let rust = chialisp_oxide_ir::compile(find(helper), optimize).unwrap();
            for malformed in ["(1)", "((1 . 2))", "((1 2 . 3))"] {
                assert_eq!(
                    vm::execute(&rust, malformed, 100_000_000).unwrap().outcome,
                    Outcome::Rejected,
                    "{helper}: {malformed}"
                );
            }
        }
        // A false head cannot short-circuit transport validation of its tail.
        assert_eq!(
            max_run(find("all_list"), optimize, "((() . 1))").outcome,
            Outcome::Rejected
        );
        let run = |name: &str, input: &str| max_run(find(name), optimize, input);
        assert_eq!(run("unused", "(((8)))").outcome, Outcome::Rejected);
        assert_eq!(
            run("lazy", "(() ((8)))").outcome,
            Outcome::Accepted {
                output_hex: "07".into()
            }
        );
        assert_eq!(run("lazy", "(1 ((8)))").outcome, Outcome::Rejected);
        let guard = chialisp_oxide_ir::compile(find("guard"), optimize).unwrap();
        let guard_reference =
            chialisp_oxide_reference::compile("(mod (C E P A) (softfork C E P A))").unwrap();
        for case in softfork_cases() {
            let solution = case["arguments"].as_str().unwrap();
            let actual =
                chialisp_oxide_backend_experiment::execute(&guard, solution, 100_000_000).unwrap();
            let expected =
                chialisp_oxide_backend_experiment::execute(&guard_reference, solution, 100_000_000)
                    .unwrap();
            same(
                &actual,
                &expected,
                &format!(
                    "softfork case={} arguments={} optimized={optimize}",
                    case["name"], case["arguments"]
                ),
            );
            if let Some(output) = case.get("expected_output") {
                let hex = literal(output.as_str().unwrap())
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();
                assert_eq!(
                    actual.outcome,
                    Outcome::Accepted { output_hex: hex },
                    "{}",
                    case["name"]
                );
            } else {
                let expected_code = case["expected_failure"].as_str().unwrap();
                assert_eq!(
                    format!("{:?}", actual.failure.unwrap().code()),
                    expected_code,
                    "{}",
                    case["name"]
                );
                if expected_code == "CostExceeded" {
                    assert_eq!(actual.outcome, Outcome::Inconclusive);
                } else {
                    assert_eq!(actual.outcome, Outcome::Rejected);
                }
            }
        }
        // Resource observations add no policy-rejection credit.
        let rust = chialisp_oxide_ir::compile(find("add_list"), optimize).unwrap();
        // The facade's native-only 128-item resource cap is not an on-chain
        // policy check. Execute 129 nil operands in both Rust modes and an
        // independently compiled recursive Chialisp sum, outside native/VM
        // equivalence cases (the direct native observation is ResourceExhausted).
        let beyond_host_limit = format!("(({}))", vec!["()"; 129].join(" "));
        let independent_sum = chialisp_oxide_reference::compile(
            "(mod (VALUES) (defun sum (L) (if L (+ (f L) (sum (r L))) 0)) (sum VALUES))",
        )
        .unwrap();
        let beyond_host = vm::execute(&rust, &beyond_host_limit, 100_000_000).unwrap();
        let independent = vm::execute(&independent_sum, &beyond_host_limit, 100_000_000).unwrap();
        assert_eq!(
            beyond_host.outcome,
            Outcome::Accepted {
                output_hex: "80".into()
            }
        );
        same(
            &beyond_host,
            &independent,
            "native-only list limit is absent from compiled policy",
        );
        let resource = chialisp_oxide_backend_experiment::execute(&rust, "((1 2 3))", 1).unwrap();
        assert_eq!(resource.outcome, Outcome::Inconclusive);
        assert_eq!(
            format!("{:?}", resource.failure.unwrap().code()),
            "CostExceeded"
        );
    }
}
fn max_run(program: &chialisp_oxide_ir::Program, optimize: bool, input: &str) -> Execution {
    vm::execute(
        &chialisp_oxide_ir::compile(program, optimize).unwrap(),
        input,
        100_000_000,
    )
    .unwrap()
}

/// These are direct constructed/raw VM controls, not additional Rust exports,
/// native calls or compiler-preservation observations.
#[test]
fn pinned_direct_softfork_arity_cost_and_argument_order_controls() {
    let data: Value =
        serde_json::from_str(include_str!("raw_clvm_softfork/softfork-vectors.json")).unwrap();
    for case in data["raw_program_cases"].as_array().unwrap() {
        let program = literal(case["program"].as_str().unwrap());
        let execution = chialisp_oxide_backend_experiment::execute(
            &program,
            case["environment"].as_str().unwrap(),
            case["max_cost"].as_u64().unwrap(),
        )
        .unwrap();
        if let Some(output) = case.get("expected_output") {
            let hex = literal(output.as_str().unwrap())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            assert_eq!(
                execution.outcome,
                Outcome::Accepted { output_hex: hex },
                "{}",
                case["name"]
            );
            assert_eq!(
                execution.cost,
                case.get("expected_cost").and_then(Value::as_u64),
                "{}",
                case["name"]
            );
        } else {
            assert_eq!(
                format!("{:?}", execution.failure.unwrap().code()),
                case["expected_failure"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            assert_eq!(
                execution.outcome,
                if case["expected_failure_kind"].as_str() == Some("Resource") {
                    Outcome::Inconclusive
                } else {
                    Outcome::Rejected
                }
            );
        }
    }
}
