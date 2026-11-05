//! Native execution of the actual Cargo export, not a copied morphing policy.
//! Exact trees below are independently specified wire expectations. Native VM
//! panics establish this lane's failure only, not a typed consensus rejection.
use chialisp_oxide::{OwnedTree, Tree};
use chialisp_oxide_puzzles::morph_coin_doubler;

fn atom(bytes: &'static [u8]) -> Tree<'static> {
    Tree::Atom(bytes)
}

fn pair(first: Tree<'static>, rest: Tree<'static>) -> Tree<'static> {
    // Small immutable fixture graphs live for this test process.
    Tree::Pair(Box::leak(Box::new(first)), Box::leak(Box::new(rest)))
}

fn list(values: &[Tree<'static>]) -> Tree<'static> {
    values
        .iter()
        .rev()
        .fold(atom(&[]), |rest, value| pair(*value, rest))
}

fn assert_exact(actual: &OwnedTree<'_>, expected: &Tree<'_>) {
    match (actual, expected) {
        (OwnedTree::Atom(actual), Tree::Atom(expected)) => assert_eq!(actual, expected),
        (OwnedTree::Pair(first, rest), Tree::Pair(expected_first, expected_rest)) => {
            assert_exact(first, expected_first);
            assert_exact(rest, expected_rest);
        }
        _ => panic!("different Atom/Pair shapes"),
    }
}

fn assert_morphs(conditions: Tree<'static>, expected: Tree<'static>) {
    // CLVM program 1 returns the complete inner environment unchanged. Thus
    // this checks the actual `(INNER_PUZZLE . inner_solution)` export ABI too.
    let environment = pair(atom(&[1]), conditions);
    assert_exact(&morph_coin_doubler::puzzle(&environment), &expected);
}

fn assert_native_program_failure(environment: Tree<'static>) {
    let payload = std::panic::catch_unwind(|| morph_coin_doubler::puzzle(&environment))
        .err()
        .expect("malformed program input must fail");
    assert!(
        !payload.is::<chialisp_oxide::ResourceExhausted>(),
        "native resource failure is inconclusive"
    );
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .expect("expected native VM program-failure diagnostic");
    assert!(
        message.starts_with("applied CLVM program rejected:"),
        "{message}"
    );
}

#[test]
fn matching_payment_doubles_amount_and_strips_only_its_memos() {
    let hash = atom(&[0x42; 32]);
    let memo = list(&[atom(b"memo"), pair(atom(&[1]), atom(&[2]))]);
    let payment = list(&[atom(&[51]), hash, atom(&[7]), memo]);
    let other = pair(atom(&[50]), pair(memo, atom(b"improper tail")));
    assert_morphs(
        list(&[payment, other]),
        list(&[list(&[atom(&[51]), hash, atom(&[14])]), other]),
    );
}

#[test]
fn only_exact_opcode_bytes_match_and_other_conditions_keep_all_structure() {
    let padded = pair(atom(&[0, 51]), atom(b"not even a field list"));
    let zero = pair(atom(&[0]), atom(&[]));
    let nil = pair(atom(&[]), pair(atom(&[9]), atom(&[8])));
    let negative = pair(atom(&[0xff]), atom(&[1]));
    let conditions = list(&[padded, zero, nil, negative]);
    assert_morphs(conditions, conditions);
}

#[test]
fn listp_stops_on_any_atom_tail_and_discards_it() {
    for tail in [atom(&[]), atom(&[0]), atom(b"end")] {
        assert_morphs(tail, atom(&[]));
        let other = pair(atom(&[80]), atom(b"unchanged condition"));
        assert_morphs(pair(other, tail), list(&[other]));
    }
}

#[test]
fn numeric_atoms_are_unbounded_signed_and_need_not_be_canonical() {
    let cases: &[(&'static [u8], &'static [u8])] = &[
        (&[], &[]),
        (&[0], &[]),
        (&[0, 1], &[2]),
        (&[0xff], &[0xfe]),
        (&[0xff, 0xff], &[0xfe]),
        (&[0x80], &[0xff, 0]),
        (&[0, 0x80], &[1, 0]),
        // 2^128 -> 2^129: beyond every ordinary Rust integer type.
        (
            &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            &[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        ),
    ];
    for &(input, doubled) in cases {
        let hash = atom(b"arbitrary hash length");
        assert_morphs(
            list(&[list(&[atom(&[51]), hash, atom(input)])]),
            list(&[list(&[atom(&[51]), hash, atom(doubled)])]),
        );
    }
}

#[test]
fn puzzle_hash_is_arbitrary_tree_and_tail_after_amount_is_ignored() {
    let hash = pair(atom(b"left"), atom(b"right"));
    let condition = pair(atom(&[51]), pair(hash, pair(atom(&[3]), atom(b"tail"))));
    assert_morphs(
        list(&[condition]),
        list(&[list(&[atom(&[51]), hash, atom(&[6])])]),
    );
}

#[test]
fn quoted_inner_program_ignores_an_arbitrary_complete_solution() {
    let condition = list(&[atom(&[51]), atom(&[]), atom(&[2])]);
    let quoted = pair(atom(&[1]), list(&[condition]));
    for solution in [atom(b"atom solution"), pair(atom(&[7]), atom(&[8]))] {
        let environment = pair(quoted, solution);
        let expected = list(&[list(&[atom(&[51]), atom(&[]), atom(&[4])])]);
        assert_exact(&morph_coin_doubler::puzzle(&environment), &expected);
    }
}

#[test]
fn malformed_condition_fields_and_nonatom_opcodes_or_amounts_fail() {
    let malformed = [
        atom(&[]),
        atom(&[51]),
        list(&[atom(&[51])]),
        list(&[atom(&[51]), atom(b"hash")]),
        list(&[atom(&[51]), atom(b"hash"), pair(atom(&[1]), atom(&[]))]),
        pair(pair(atom(&[51]), atom(&[])), atom(&[])),
    ];
    for condition in malformed {
        let environment = pair(atom(&[1]), list(&[condition]));
        assert_native_program_failure(environment);
    }
}

#[test]
fn malformed_outer_environment_and_failing_inner_program_fail() {
    for environment in [atom(&[]), atom(&[1]), pair(list(&[atom(&[8])]), atom(&[]))] {
        assert_native_program_failure(environment);
    }
}
