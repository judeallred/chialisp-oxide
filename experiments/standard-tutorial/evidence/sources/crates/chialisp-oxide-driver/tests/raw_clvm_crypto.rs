//! Pinned upstream crypto vectors, native VM oracle, actual helper MIR and
//! independent Chialisp compilation. These share cryptographic implementations.
mod support;
#[path = "support/vm.rs"]
mod vm;
use chialisp_oxide_backend_experiment::Outcome;
use clvmr_sdk::{Allocator, NodePtr, allocator::SExp, serde::node_from_bytes};
use serde_json::Value;
use std::{collections::BTreeSet, path::Path, process::Command};
use support::Fixture;

const VECTORS: &str = include_str!("raw_clvm_crypto/vectors.json");
const HELPERS: &[(&str, &str, usize)] = &[
    ("pubkey_for_exp", "pubkey_for_exp", 1),
    ("point_add", "point_add", 2),
    ("g1_subtract", "g1_subtract", 2),
    ("g1_multiply", "g1_multiply", 2),
    ("g1_negate", "g1_negate", 1),
    ("g2_add", "g2_add", 2),
    ("g2_subtract", "g2_subtract", 2),
    ("g2_multiply", "g2_multiply", 2),
    ("g2_negate", "g2_negate", 1),
    ("g1_map", "g1_map", 1),
    ("g1_map_with_dst", "g1_map", 2),
    ("g2_map", "g2_map", 1),
    ("g2_map_with_dst", "g2_map", 2),
    ("bls_pairing_identity", "bls_pairing_identity", 2),
    ("bls_pairing_identity_two", "bls_pairing_identity", 4),
    ("bls_verify", "bls_verify", 3),
    ("secp256k1_verify", "secp256k1_verify", 3),
    ("secp256r1_verify", "secp256r1_verify", 3),
];

fn literal(source: &str) -> Vec<u8> {
    // Compilation/VM here only transport an independently declared literal.
    let code = chialisp_oxide_reference::compile(&format!("(mod () (q . {source}))")).unwrap();
    let execution = vm::execute(&code, "()", 100_000_000).unwrap();
    let Outcome::Accepted { output_hex } = execution.outcome else {
        panic!("literal must encode")
    };
    (0..output_hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&output_hex[i..i + 2], 16).unwrap())
        .collect()
}
fn rust_tree(a: &Allocator, node: NodePtr) -> String {
    match a.sexp(node) {
        SExp::Atom => format!("Tree::Atom(&{:?})", a.atom(node).as_ref()),
        SExp::Pair(first, rest) => format!(
            "Tree::Pair(&{}, &{})",
            rust_tree(a, first),
            rust_tree(a, rest)
        ),
    }
}
fn rust_literal(source: &str) -> String {
    let mut a = Allocator::new();
    let node = node_from_bytes(&mut a, &literal(source)).unwrap();
    rust_tree(&a, node)
}
fn rust_arguments(source: &str) -> Vec<String> {
    let mut a = Allocator::new();
    let mut node = node_from_bytes(&mut a, &literal(source)).unwrap();
    let mut result = Vec::new();
    while let SExp::Pair(first, rest) = a.sexp(node) {
        result.push(rust_tree(&a, first));
        node = rest;
    }
    assert!(
        a.atom(node).as_ref().is_empty(),
        "fixture arguments must be a proper list"
    );
    result
}
fn native_source(cases: &[Value]) -> String {
    let mut source = String::from("use chialisp_oxide::{Tree,raw_clvm as raw};\n");
    for (helper, _, arity) in HELPERS {
        let names = ["a", "b", "c", "d"][..*arity].to_vec();
        source.push_str(&format!("#[chialisp_oxide::puzzle] pub fn {helper}<'a>({})->Tree<'static>{{raw::{helper}({})}}\n",names.iter().map(|n|format!("{n}:&'a Tree<'a>")).collect::<Vec<_>>().join(","),names.join(",")));
    }
    source.push_str("#[cfg(test)]mod native{use super::*;#[test]fn pinned_vm_oracle_vectors(){\n");
    for case in cases {
        let helper = case["helper"].as_str().unwrap();
        let arguments = rust_arguments(case["arguments"].as_str().unwrap());
        let call = format!(
            "{helper}({})",
            arguments
                .iter()
                .map(|s| format!("&{s}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        if let Some(expected) = case.get("expected_output") {
            source.push_str(&format!(
                "assert_eq!({call},{}, {:?});\n",
                rust_literal(expected.as_str().unwrap()),
                case["name"].as_str().unwrap()
            ));
        } else {
            source.push_str(&format!("let failure=std::panic::catch_unwind(||{call}).expect_err({:?});assert!(!failure.is::<chialisp_oxide::ResourceExhausted>(),\"host limits are inconclusive\");\n",case["name"].as_str().unwrap()));
        }
    }
    source.push_str("}}\n");
    source
}

#[test]
fn fixed_crypto_helpers_match_pinned_vectors_and_independent_chialisp() {
    let data: Value = serde_json::from_str(VECTORS).unwrap();
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(
        cases
            .iter()
            .map(|c| c["helper"].as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        HELPERS.iter().map(|h| h.0).collect()
    );
    let fixture = Fixture::new();
    fixture.source(&native_source(cases));
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
    let output = Command::new(Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/cargo"))
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
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let output = fixture.cargo(true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let programs = fixture.exports();
    assert_eq!(programs.len(), HELPERS.len());
    for (helper, operator, arity) in HELPERS {
        let program = programs
            .iter()
            .find(|p| p.name.ends_with(&format!("::{helper}")))
            .unwrap();
        assert!(program.uses_apply());
        let names = ["A", "B", "C", "D"][..*arity].join(" ");
        let reference =
            chialisp_oxide_reference::compile(&format!("(mod ({names}) ({operator} {names}))"))
                .unwrap();
        for optimize in [false, true] {
            let rust = chialisp_oxide_ir::compile(program, optimize).unwrap();
            for case in cases
                .iter()
                .filter(|c| c["helper"].as_str() == Some(helper))
            {
                let solution = case["arguments"].as_str().unwrap();
                let actual = vm::execute(&rust, solution, 100_000_000).unwrap();
                let expected = vm::execute(&reference, solution, 100_000_000).unwrap();
                assert_eq!(
                    actual.outcome, expected.outcome,
                    "{}; optimized={optimize}",
                    case["name"]
                );
                if let Some(output) = case.get("expected_output") {
                    let wanted = literal(output.as_str().unwrap());
                    let hex = wanted
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
                    assert_eq!(actual.outcome, Outcome::Rejected, "{}", case["name"]);
                    let code = actual.failure.unwrap().code();
                    assert_eq!(
                        format!("{code:?}"),
                        case["expected_failure"].as_str().unwrap(),
                        "{}",
                        case["name"]
                    );
                    assert_eq!(Some(code), expected.failure.map(|f| f.code()));
                }
            }
        }
    }
}

/// Establish that the retained G1 counterexample is on-curve but outside the
/// prime-order subgroup, separately from the actual VM rejection observations.
#[test]
fn retained_g1_subgroup_counterexample_passes_unchecked_but_not_checked_parser() {
    let data: Value = serde_json::from_str(VECTORS).unwrap();
    let encoded = data["subgroup_control"]["compressed_g1"].as_str().unwrap();
    let bytes: Vec<u8> = (0..encoded.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&encoded[i..i + 2], 16).unwrap())
        .collect();
    let bytes: [u8; 48] = bytes.try_into().unwrap();
    let unchecked = chia_bls::PublicKey::from_bytes_unchecked(&bytes)
        .expect("counterexample must decode as an on-curve point");
    assert!(
        !unchecked.is_valid(),
        "counterexample must fail subgroup membership"
    );
    assert!(chia_bls::PublicKey::from_bytes(&bytes).is_err());
}
