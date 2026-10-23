//! WIP prototype; do not use for real funds.
//!
//! The author explicitly chooses the canonical standard module's whole-tree
//! environment. The dependency lock and generator infer no nominal Rust ABI.
//! Run prepare.py to generate this module from the exact admitted dependency.
use chialisp_oxide::Tree;

#[path = "generated/standard.rs"]
mod generated_standard;

/// Whole environment: `(SYNTHETIC_KEY ORIGINAL_KEY DELEGATED_PUZZLE SOLUTION)`.
/// Before funding, curry SYNTHETIC_KEY as one argument into this wrapper.
/// The resulting instance receives `(ORIGINAL_KEY DELEGATED_PUZZLE SOLUTION)`.
/// This example uses nil ORIGINAL_KEY and the standard delegated-signature path.
/// Returning the actual dependency's output preserves strict Apply behavior.
#[chialisp_oxide::puzzle(environment = "raw")]
pub fn standard_wallet<'a>(environment: &'a Tree<'a>) -> Tree<'static> {
    generated_standard::run(environment)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(tree: &Tree<'_>) -> Vec<u8> {
        match tree {
            Tree::Atom(bytes) => {
                assert!(bytes.len() < 64, "fixed native witness atom bound");
                if bytes.len() == 1 && bytes[0] < 128 {
                    return bytes.to_vec();
                }
                let mut result = vec![0x80 | u8::try_from(bytes.len()).unwrap()];
                result.extend_from_slice(bytes);
                result
            }
            Tree::Pair(first, rest) => {
                let mut result = vec![0xff];
                result.extend(encoded(first));
                result.extend(encoded(rest));
                result
            }
        }
    }

    #[test]
    fn native_actual_generated_standard_returns_exact_independent_condition() {
        // Public key from the pinned mainnet observation, with no claim of
        // owning its private key. SDK spends choose a different disposable key.
        let key = hex::decode("9415e2b4b30977380e167096714abfdd5a594f6326dc278aa895e6bd6901953b9d8d37126e5411027eda81be8b16b97f").unwrap();
        let nil = Tree::Atom(&[]);
        let identity_program = Tree::Atom(&[1]);
        let solution_tail = Tree::Pair(&nil, &nil);
        let delegated = Tree::Pair(&identity_program, &solution_tail);
        let solution = Tree::Pair(&nil, &delegated);
        let public_key = Tree::Atom(&key);
        let environment = Tree::Pair(&public_key, &solution);
        let actual = standard_wallet(&environment);

        // Independent wire bytes: AGG_SIG_ME and SHA256(0x01 || 0x01).
        let opcode = Tree::Atom(&[50]);
        let message_bytes =
            hex::decode("9dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2")
                .unwrap();
        let message = Tree::Atom(&message_bytes);
        let message_tail = Tree::Pair(&message, &nil);
        let key_message = Tree::Pair(&public_key, &message_tail);
        let signature = Tree::Pair(&opcode, &key_message);
        assert_eq!(actual, Tree::Pair(&signature, &nil));
        println!(
            "REGISTRY_STANDARD_NATIVE {}",
            serde_json::json!({"name":"retrieved_standard_identity_program","status":"accepted","output_hex":hex::encode(encoded(&actual))})
        );
    }
}
