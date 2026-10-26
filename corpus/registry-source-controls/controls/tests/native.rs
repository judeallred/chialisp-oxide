//! Typed native source-control witnesses only; no VM, SDK, wallets or network.
#![cfg(feature = "native-vm")]
#[path = "support/wire.rs"]
mod wire;

use chialisp_oxide::{Cons, PublicKey, PuzzleHash, Tree};
use chialisp_oxide_puzzles::{registry_recipe as original, registry_set_alpha, registry_set_beta};
use chialisp_oxide_registry_controls::{
    alpha_drop_create, alpha_drop_send, beta_drop_create, beta_drop_send,
    registry_recipe as control,
};
use serde_json::{Value, json};
use wire::Wire;

fn bytes<const N: usize>(text: &str) -> [u8; N] {
    assert_eq!(text.len(), 2 * N);
    let mut value = [0; N];
    for (index, byte) in value.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * index..2 * index + 2], 16).unwrap();
    }
    value
}
fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn original_truth(alpha: [u8; 32], beta: [u8; 32]) -> original::RegistryTruth<'static> {
    Cons {
        first: Tree::Atom(&[]),
        rest: Cons {
            first: true,
            rest: Cons {
                first: original::EntryCommitment(alpha),
                rest: original::EntryCommitment(beta),
            },
        },
    }
}
fn control_truth(alpha: [u8; 32], beta: [u8; 32]) -> control::RegistryTruth<'static> {
    Cons {
        first: Tree::Atom(&[]),
        rest: Cons {
            first: true,
            rest: Cons {
                first: control::EntryCommitment(alpha),
                rest: control::EntryCommitment(beta),
            },
        },
    }
}

#[test]
fn local_recipe_is_exact_current_original_source() {
    assert_eq!(
        include_bytes!("../src/registry_recipe.rs").as_slice(),
        include_bytes!("../../../puzzles/registry_recipe.rs").as_slice(),
        "refresh the reviewed recipe copy explicitly after any original change"
    );
}

#[test]
fn actual_control_exports_and_linked_originals_match_independent_witnesses() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../fixtures/native-vectors.json")).unwrap();
    let cases = fixtures["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    let mut baseline_calls = 0;
    let mut variant_calls = 0;
    for case in cases {
        // These are finite typed Rust inputs, not a CLVM ABI decoder or ledger.
        assert!(case["initialized"].as_bool().unwrap());
        assert_eq!(case["ephemeral_atom_hex"], "");
        let owner = PublicKey(bytes(case["owner_hex"].as_str().unwrap()));
        let first = PuzzleHash(bytes(case["slot_first_hash_hex"].as_str().unwrap()));
        let alpha = bytes(case["alpha_hash_hex"].as_str().unwrap());
        let beta = bytes(case["beta_hash_hex"].as_str().unwrap());
        let old = bytes(case["old_value_hex"].as_str().unwrap());
        let new = bytes(case["new_value_hex"].as_str().unwrap());
        baseline_calls += 1;
        let baseline = match case["side"].as_str().unwrap() {
            "alpha" => {
                registry_set_alpha::puzzle(owner, first, original_truth(alpha, beta), old, new)
                    .wire()
            }
            "beta" => {
                registry_set_beta::puzzle(owner, first, original_truth(alpha, beta), old, new)
                    .wire()
            }
            other => panic!("unknown closed fixture side {other}"),
        };
        variant_calls += 1;
        let variant = match case["id"].as_str().unwrap() {
            "alpha-drop-send" => {
                alpha_drop_send::puzzle(owner, first, control_truth(alpha, beta), old, new).wire()
            }
            "alpha-drop-create" => {
                alpha_drop_create::puzzle(owner, first, control_truth(alpha, beta), old, new).wire()
            }
            "beta-drop-send" => {
                beta_drop_send::puzzle(owner, first, control_truth(alpha, beta), old, new).wire()
            }
            "beta-drop-create" => {
                beta_drop_create::puzzle(owner, first, control_truth(alpha, beta), old, new).wire()
            }
            other => panic!("unknown closed fixture variant {other}"),
        };
        assert_eq!(
            hex(&baseline),
            case["baseline_expected"]["output_hex"].as_str().unwrap()
        );
        assert_eq!(
            hex(&variant),
            case["variant_expected"]["output_hex"].as_str().unwrap()
        );
        assert_ne!(baseline, variant);
        println!(
            "REGISTRY_RUST_CONTROL_NATIVE {}",
            json!({
                "id":case["id"],
                "baseline":{"status":"accepted","output_hex":hex(&baseline)},
                "variant":{"status":"accepted","output_hex":hex(&variant)},
                "scope":"Actual typed native calls only; no CLVM ABI/frontend, VM or SDK credit."
            })
        );
    }
    assert_eq!((baseline_calls, variant_calls), (4, 4));
    println!(
        "REGISTRY_RUST_CONTROL_NATIVE_SUMMARY {}",
        json!({
            "baseline_policy_calls":baseline_calls,"variant_policy_calls":variant_calls,
            "abi_observations":0,"VM_executions":0,"SDK_submissions":0
        })
    );
}
