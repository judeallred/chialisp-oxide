//! WIP evidence fixture, materialized by check-registry-codegen.py.
//!
//! Generated modules are produced from an explicit admitted local dependency
//! lock. They accept opaque Trees; framing and key roles belong to this author.
use chialisp_oxide::Tree;

#[path = "generated/atom255.rs"]
mod generated_atom255;
#[path = "generated/atom256.rs"]
mod generated_atom256;
#[path = "generated/multiply.rs"]
mod generated_multiply;
#[path = "generated/standard.rs"]
mod generated_standard;
mod key;

const MULTIPLIER: [u8; 1] = [2];

/// Apply the retrieved multiplier to `(2 . complete_raw_solution)`.
#[chialisp_oxide::puzzle(environment = "raw")]
pub fn multiply<'a>(solution: &'a Tree<'a>) -> Tree<'static> {
    let multiplier = Tree::Atom(&MULTIPLIER);
    let environment = Tree::Pair(&multiplier, solution);
    generated_multiply::run(&environment)
}

/// Apply the retrieved standard module to `(fixture_key . raw_solution)`.
/// This forwards malformed shapes and improper tails to the actual module.
#[chialisp_oxide::puzzle(environment = "raw")]
pub fn standard<'a>(solution: &'a Tree<'a>) -> Tree<'static> {
    let synthetic_key = Tree::Atom(&key::FIXTURE_SYNTHETIC_KEY);
    let environment = Tree::Pair(&synthetic_key, solution);
    generated_standard::run(&environment)
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn atom255<'a>(environment: &'a Tree<'a>) -> Tree<'static> {
    generated_atom255::run(environment)
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn atom256<'a>(environment: &'a Tree<'a>) -> Tree<'static> {
    generated_atom256::run(environment)
}

/// Discarding a strict application must not discard its failure behavior.
#[chialisp_oxide::puzzle(environment = "raw")]
pub fn strict_unused_apply<'a>(solution: &'a Tree<'a>) -> Tree<'static> {
    let multiplier = Tree::Atom(&MULTIPLIER);
    let environment = Tree::Pair(&multiplier, solution);
    let _result = generated_multiply::run(&environment);
    Tree::Atom(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_multiplier_uses_the_generated_program() {
        let nil = Tree::Atom(&[]);
        let value = Tree::Atom(&[21]);
        let solution = Tree::Pair(&value, &nil);
        assert_eq!(multiply(&solution), Tree::Atom(&[42]));
        assert_eq!(strict_unused_apply(&solution), nil);
    }

    #[test]
    fn native_standard_identity_program_returns_exact_signature_condition() {
        let nil = Tree::Atom(&[]);
        let identity_program = Tree::Atom(&[1]);
        let solution_tail = Tree::Pair(&nil, &nil);
        let program_and_solution = Tree::Pair(&identity_program, &solution_tail);
        let solution = Tree::Pair(&nil, &program_and_solution);
        let actual = standard(&solution);

        // Independent wire expectation, not a condition constructor. The
        // message is SHA256(0x01 || 0x01), the atom1 program's tree hash.
        let opcode = Tree::Atom(&[50]);
        let public_key = Tree::Atom(&key::FIXTURE_SYNTHETIC_KEY);
        let message = Tree::Atom(&[
            0x9d, 0xcf, 0x97, 0xa1, 0x84, 0xf3, 0x26, 0x23, 0xd1, 0x1a, 0x73, 0x12, 0x4c, 0xeb,
            0x99, 0xa5, 0x70, 0x9b, 0x08, 0x37, 0x21, 0xe8, 0x78, 0xa1, 0x6d, 0x78, 0xf5, 0x96,
            0x71, 0x8b, 0xa7, 0xb2,
        ]);
        let message_tail = Tree::Pair(&message, &nil);
        let key_and_message = Tree::Pair(&public_key, &message_tail);
        let condition = Tree::Pair(&opcode, &key_and_message);
        assert_eq!(actual, Tree::Pair(&condition, &nil));
    }

    #[test]
    fn native_atom_boundaries_preserve_every_payload_byte() {
        let environment = Tree::Atom(&[0xff, 0x80, 0]);
        assert_eq!(atom255(&environment), Tree::Atom(&[0xa5; 255]));
        assert_eq!(atom256(&environment), Tree::Atom(&[0xa5; 256]));
    }
}
