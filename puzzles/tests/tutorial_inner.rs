//! Tests the actual three Cargo exports. Hash literals were computed separately
//! from the SDK using SHA-256(01 || atom) / SHA-256(02 || left || right).
//! These are native/tree observations, not signature or consensus verification.
use chialisp_oxide::Tree;
use chialisp_oxide_puzzles::{
    inner_any_conditions, inner_require_signature, morph_any_with_signature,
};

fn atom(bytes: &'static [u8]) -> Tree<'static> {
    Tree::Atom(bytes)
}

fn pair(first: Tree<'static>, rest: Tree<'static>) -> Tree<'static> {
    Tree::Pair(Box::leak(Box::new(first)), Box::leak(Box::new(rest)))
}

fn digest(hex: &str) -> [u8; 32] {
    assert_eq!(hex.len(), 64);
    std::array::from_fn(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
}

fn hash_cases() -> Vec<(Tree<'static>, &'static str)> {
    vec![
        (
            atom(&[]),
            "4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a",
        ),
        (
            atom(&[0]),
            "47dc540c94ceb704a23875c11273e16bb0b8a87aed84de911f2133568115f254",
        ),
        (
            atom(&[1]),
            "9dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2",
        ),
        (
            atom(&[0, 1]),
            "85f90dfea1d8027e1463e5ca971a250110a20df0119d204a74220bc63516d15b",
        ),
        (
            pair(atom(&[1]), atom(&[2])),
            "48f6eb3dcb192667016ff10dac09fb21b9388f18d91a863a270f4a91477e8528",
        ),
        (
            pair(atom(&[1]), atom(&[])),
            "ba4484b961b7a2369d948d06c55b64bdbfaffb326bc13b490ab1215dd33d8d46",
        ),
        (
            atom(&[0xff, 0, 0x80]),
            "11b7b29c6aa43cc1fcc3361ac17bf9195eb017349e555ee50a68e3c55652e69d",
        ),
    ]
}

fn assert_signature(signature: (u64, &Tree<'_>, [u8; 32]), key: &Tree<'_>, hash: &str) {
    assert_eq!(signature.0, 50);
    assert_eq!(signature.1, key);
    assert_eq!(signature.2, digest(hash));
}

fn assert_policy_panic<T>(result: std::thread::Result<T>) {
    let payload = result.err().expect("malformed environment must fail");
    assert!(
        !payload.is::<chialisp_oxide::ResourceExhausted>(),
        "host exhaustion cannot confirm rejection"
    );
    assert!(payload.is::<String>() || payload.is::<&str>());
}

#[test]
fn arbitrary_conditions_preserve_bytes_pairs_and_ignored_environment_tails() {
    for (conditions, _) in hash_cases() {
        for tail in [atom(&[]), atom(&[0xff]), pair(atom(&[8]), atom(&[9]))] {
            let environment = pair(conditions, tail);
            assert_eq!(*inner_any_conditions::puzzle(&environment), conditions);
        }
    }
}

#[test]
fn arbitrary_conditions_need_one_field_but_not_a_proper_argument_list() {
    for environment in [atom(&[]), atom(&[0]), atom(&[7])] {
        assert_policy_panic(std::panic::catch_unwind(|| {
            inner_any_conditions::puzzle(&environment)
        }));
    }
    let environment = pair(atom(&[0, 1]), atom(b"ignored"));
    assert_eq!(*inner_any_conditions::puzzle(&environment), atom(&[0, 1]));
}

#[test]
fn condition_signature_hashes_exact_arbitrary_trees_and_preserves_key_and_tail() {
    for (conditions, expected) in hash_cases() {
        for key in [atom(&[]), atom(&[0, 1]), pair(atom(&[1]), atom(&[2]))] {
            let environment = pair(key, pair(conditions, atom(b"ignored tail")));
            let result = morph_any_with_signature::puzzle(&environment);
            assert_signature(result.first, &key, expected);
            assert_eq!(*result.rest, conditions);
        }
    }
}

#[test]
fn signature_wrapper_hashes_the_complete_dotted_solution_before_identity_apply() {
    for (solution, expected) in hash_cases() {
        let key = pair(atom(b"arbitrary key"), atom(&[0]));
        let environment = pair(key, pair(atom(&[1]), solution));
        let result = inner_require_signature::puzzle(&environment);
        assert_signature(result.first, &key, expected);
        assert_eq!(result.rest, solution);
    }
}

#[test]
fn inner_output_does_not_replace_the_signed_solution_or_gain_a_list_wrapper() {
    let key = atom(&[0xff]);
    let solution = atom(&[0, 1]);
    for output in [atom(&[]), atom(&[0xff]), pair(atom(&[3]), atom(&[4]))] {
        let quoted = pair(atom(&[1]), output);
        let environment = pair(key, pair(quoted, solution));
        let result = inner_require_signature::puzzle(&environment);
        assert_signature(
            result.first,
            &key,
            "85f90dfea1d8027e1463e5ca971a250110a20df0119d204a74220bc63516d15b",
        );
        assert_eq!(result.rest, output);
    }
}

#[test]
fn changing_only_the_inner_solution_changes_its_signature_message() {
    let key = atom(&[1]);
    let quoted_nil = pair(atom(&[1]), atom(&[]));
    let first_env = pair(key, pair(quoted_nil, atom(&[])));
    let second_env = pair(key, pair(quoted_nil, atom(&[0])));
    let first = inner_require_signature::puzzle(&first_env);
    let second = inner_require_signature::puzzle(&second_env);
    assert_eq!(first.rest, atom(&[]));
    assert_eq!(second.rest, atom(&[]));
    assert_signature(
        first.first,
        &key,
        "4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a",
    );
    assert_signature(
        second.first,
        &key,
        "47dc540c94ceb704a23875c11273e16bb0b8a87aed84de911f2133568115f254",
    );
}

#[test]
fn signature_wrappers_reject_missing_fields_without_restricting_extra_tails() {
    for environment in [
        atom(&[]),
        atom(&[7]),
        pair(atom(&[1]), atom(&[])),
        pair(atom(&[1]), atom(&[2])),
    ] {
        assert_policy_panic(std::panic::catch_unwind(|| {
            morph_any_with_signature::puzzle(&environment)
        }));
        assert_policy_panic(std::panic::catch_unwind(|| {
            inner_require_signature::puzzle(&environment)
        }));
    }
}

#[test]
fn failing_inner_program_is_a_native_program_failure_not_resource_credit() {
    let raise = pair(atom(&[8]), atom(&[]));
    let environment = pair(atom(&[1]), pair(raise, atom(&[])));
    let payload = std::panic::catch_unwind(|| inner_require_signature::puzzle(&environment))
        .err()
        .expect("inner raise must fail");
    assert!(!payload.is::<chialisp_oxide::ResourceExhausted>());
    let message = payload
        .downcast_ref::<String>()
        .expect("native apply diagnostic");
    assert!(
        message.starts_with("applied CLVM program rejected:"),
        "{message}"
    );
}
