//! WIP evidence fixture, not ready for use or a stable wallet interface.
//!
//! Actual CLI-generated local modules fix a completely resolved registry pin.
//! Every raw export receives `(program_witness . complete_environment)`.
//! This changes funding ABI and puzzle hash. Tree commitment under ordinary
//! SHA assumptions is not a commitment to original wire, source or authorship.
//! Existing `puzzles/inner-puzzle` already implements hash-checked Apply.
use chialisp_oxide::Tree;

#[path = "generated/multiply.rs"]
mod multiply;
#[path = "generated/quote257.rs"]
mod quote257;
#[path = "generated/quote8192.rs"]
mod quote8192;

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn multiply_witness<'a>(environment: &'a Tree<'a>) -> Tree<'static> {
    match environment {
        Tree::Pair(program, complete_environment) => multiply::run(program, complete_environment),
        Tree::Atom(_) => panic!(),
    }
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn strict_unused_multiply_witness<'a>(environment: &'a Tree<'a>) -> Tree<'static> {
    let _ = multiply_witness(environment);
    Tree::Atom(&[])
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn quote257_witness<'a>(environment: &'a Tree<'a>) -> Tree<'static> {
    match environment {
        Tree::Pair(program, complete_environment) => quote257::run(program, complete_environment),
        Tree::Atom(_) => panic!(),
    }
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn strict_unused_quote257_witness<'a>(environment: &'a Tree<'a>) -> Tree<'static> {
    let _ = quote257_witness(environment);
    Tree::Atom(&[])
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn quote8192_witness<'a>(environment: &'a Tree<'a>) -> Tree<'static> {
    match environment {
        Tree::Pair(program, complete_environment) => quote8192::run(program, complete_environment),
        Tree::Atom(_) => panic!(),
    }
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn strict_unused_quote8192_witness<'a>(environment: &'a Tree<'a>) -> Tree<'static> {
    let _ = quote8192_witness(environment);
    Tree::Atom(&[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use chialisp_oxide::{ResourceExhausted, apply};

    const NIL: Tree<'static> = Tree::Atom(&[]);
    const MULTIPLY: Tree<'static> = Tree::Atom(&[0x12]);
    const FIRST: Tree<'static> = Tree::Atom(&[2]);
    const SECOND: Tree<'static> = Tree::Atom(&[5]);
    const SECOND_TAIL: Tree<'static> = Tree::Pair(&SECOND, &NIL);
    const FIRST_TAIL: Tree<'static> = Tree::Pair(&FIRST, &SECOND_TAIL);
    const MULTIPLY_PROGRAM: Tree<'static> = Tree::Pair(&MULTIPLY, &FIRST_TAIL);

    #[test]
    fn matching_programs_return_complete_exact_outputs() {
        let value = Tree::Atom(&[21]);
        let solution = Tree::Pair(&value, &NIL);
        let environment = Tree::Pair(&FIRST, &solution);
        let witness = Tree::Pair(&MULTIPLY_PROGRAM, &environment);
        assert_eq!(multiply_witness(&witness), Tree::Atom(&[42]));
        assert_eq!(strict_unused_multiply_witness(&witness), NIL);
        let quote = Tree::Atom(&[1]);
        for size in [257, 8192] {
            // Runtime native test data, never a generated on-chain literal.
            let payload = vec![0xa5; size];
            let atom = Tree::Atom(&payload);
            let program = Tree::Pair(&quote, &atom);
            let witness = Tree::Pair(&program, &NIL);
            let (output, discarded) = if size == 257 {
                (quote257_witness(&witness), strict_unused_quote257_witness(&witness))
            } else {
                (quote8192_witness(&witness), strict_unused_quote8192_witness(&witness))
            };
            assert_eq!(output, atom);
            assert_eq!(discarded, NIL);
        }
    }

    #[test]
    fn wrong_program_rejects_even_when_direct_application_succeeds() {
        assert_eq!(apply(&NIL, &NIL), NIL);
        let witness = Tree::Pair(&NIL, &NIL);
        for call in [multiply_witness, strict_unused_multiply_witness,
            quote257_witness, strict_unused_quote257_witness,
            quote8192_witness, strict_unused_quote8192_witness] {
            let panic = std::panic::catch_unwind(|| call(&witness))
                .expect_err("a different logical program must fail the fixed hash guard");
            assert!(panic.downcast_ref::<ResourceExhausted>().is_none());
            assert!(panic_message(panic.as_ref()).is_some_and(|message| {
                message.starts_with("assertion failed: program_hash == EXPECTED_PROGRAM_TREE_HASH")
            }));
        }
    }

    #[test]
    fn discarded_matching_multiply_still_applies_and_fails() {
        // The module has its first argument but no second argument. The
        // native adapter category is observed; no Debug-to-typed-code parsing.
        let malformed = Tree::Pair(&FIRST, &NIL);
        let witness = Tree::Pair(&MULTIPLY_PROGRAM, &malformed);
        let panic = std::panic::catch_unwind(|| strict_unused_multiply_witness(&witness))
            .expect_err("discarding a call must retain matching-program failure");
        assert!(panic.downcast_ref::<ResourceExhausted>().is_none());
        assert!(panic_message(panic.as_ref()).is_some_and(|message| {
            message.starts_with("applied CLVM program rejected: ")
        }));
    }

    #[test]
    fn raw_outer_atoms_are_not_an_argument_list() {
        assert!(std::panic::catch_unwind(|| multiply_witness(&NIL)).is_err());
        assert!(std::panic::catch_unwind(|| quote257_witness(&NIL)).is_err());
        assert!(std::panic::catch_unwind(|| quote8192_witness(&NIL)).is_err());
    }

    fn panic_message(payload: &(dyn std::any::Any + Send)) -> Option<&str> {
        payload.downcast_ref::<String>().map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
    }
}
