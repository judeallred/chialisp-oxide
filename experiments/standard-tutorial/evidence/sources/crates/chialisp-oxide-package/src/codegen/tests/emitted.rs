use super::*;
use clvmr::{Allocator, NodePtr, serde::node_to_bytes};
use std::collections::BTreeMap;

/// Independent finite reader of the actual emitted data statements. It does
/// not use the production index/recipe, serialize copied input spans, or claim
/// to parse Rust generally. Actual rustc/CLVM checks are a separate gate.
fn emitted_tree(source: &str) -> Vec<u8> {
    let mut allocator = Allocator::new();
    let mut values: BTreeMap<String, NodePtr> = BTreeMap::new();
    let mut result = None;
    for line in source.lines() {
        if let Some(line) = line.strip_prefix("const ") {
            let (name, bytes) = line.split_once(": Tree<'static> = Tree::Atom(&[").unwrap();
            let bytes = bytes.strip_suffix("]);").unwrap();
            let payload: Vec<_> = if bytes.is_empty() {
                Vec::new()
            } else {
                bytes
                    .split(", ")
                    .map(|byte| u8::from_str_radix(byte.strip_prefix("0x").unwrap(), 16).unwrap())
                    .collect()
            };
            assert!(
                values
                    .insert(name.into(), allocator.new_atom(&payload).unwrap())
                    .is_none()
            );
        } else if let Some(line) = line.strip_prefix("    let ") {
            let (name, operands) = line.split_once(" = Tree::Pair(").unwrap();
            let (first, rest) = operands
                .strip_suffix(");")
                .unwrap()
                .split_once(", ")
                .unwrap();
            let first = values[first.trim_start_matches('&')];
            let rest = values[rest.trim_start_matches('&')];
            assert!(
                values
                    .insert(name.into(), allocator.new_pair(first, rest).unwrap())
                    .is_none()
            );
        } else if let Some(line) = line.strip_prefix("    apply(") {
            let name = line
                .strip_suffix(", environment)")
                .unwrap()
                .trim_start_matches('&');
            assert!(result.replace(values[name]).is_none());
        }
    }
    node_to_bytes(&allocator, result.unwrap()).unwrap()
}

#[test]
fn actual_emitted_numeric_constants_and_pair_statements_reconstruct_independent_bytes() {
    let mut cases = vec![
        vec![0x80],
        vec![0],
        vec![0x82, 0, 0x80],
        vec![0xff, 0x83, 0xff, 0xfe, 0, 0x81, 0xff],
        hex::decode("ff02ffff01ff12ff02ff0580ffff04ffff0102ff018080").unwrap(),
        hex::decode(
            include_str!(
                "../../../tests/fixtures/mainnet-standard/pinned-standard-module-hex.body"
            )
            .trim(),
        )
        .unwrap(),
    ];
    for length in [63, 64, 255, 256] {
        cases.push(atom(&vec![0xff; length]));
    }
    let mut spine = vec![0x80];
    for _ in 0..31 {
        spine = pair(&[0x80], &spine);
    }
    cases.push(spine);
    for bytes in cases {
        let generated = generated(&bytes, &GenerationLimits::default()).unwrap();
        let encoded = emitted_tree(generated.source());
        assert_eq!(encoded, bytes);
        assert_eq!(digest(&encoded), generated.record().artifact.blob_sha256);
        let admitted = inspect_artifact(&encoded, Encoding::Plain, &Limits::default()).unwrap();
        assert_eq!(admitted.identity(), &generated.record().artifact);
    }
}

#[test]
fn changed_actual_source_atom_or_pair_is_distinguished_by_independent_reader() {
    let bytes = [0xff, 1, 2];
    let generated = generated(&bytes, &GenerationLimits::default()).unwrap();
    let changed_atom = generated
        .source()
        .replacen("Tree::Atom(&[0x01])", "Tree::Atom(&[0x03])", 1);
    assert_ne!(changed_atom, generated.source());
    assert_eq!(emitted_tree(&changed_atom), [0xff, 3, 2]);
    let changed_pair = generated.source().replacen(
        "Tree::Pair(&ATOM_0, &ATOM_1)",
        "Tree::Pair(&ATOM_1, &ATOM_0)",
        1,
    );
    assert_ne!(changed_pair, generated.source());
    assert_eq!(emitted_tree(&changed_pair), [0xff, 2, 1]);
    assert_ne!(
        digest(changed_atom.as_bytes()),
        generated.record().source_sha256
    );
    assert_ne!(
        digest(changed_pair.as_bytes()),
        generated.record().source_sha256
    );
}
