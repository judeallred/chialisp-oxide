//! Actual Cargo-source observations; fixed raw primitives use the native VM.
//! No Rust closure, cryptographic, ledger or compiler-preservation claim.
use chialisp_oxide::{OwnedTree, Tree};
use chialisp_oxide_puzzles::{modern_lambda_map, modern_let_product, modern_named_map};

fn atom(bytes: &'static [u8]) -> Tree<'static> {
    Tree::Atom(bytes)
}

fn pair(first: Tree<'static>, rest: Tree<'static>) -> Tree<'static> {
    Tree::Pair(Box::leak(Box::new(first)), Box::leak(Box::new(rest)))
}

fn list(values: &[Tree<'static>]) -> Tree<'static> {
    values
        .iter()
        .rev()
        .fold(atom(&[]), |tail, first| pair(*first, tail))
}

fn exact_owned(actual: &OwnedTree<'_>, expected: &Tree<'_>) {
    match (actual, expected) {
        (OwnedTree::Atom(a), Tree::Atom(b)) => assert_eq!(a, b),
        (OwnedTree::Pair(a, b), Tree::Pair(c, d)) => {
            exact_owned(a, c);
            exact_owned(b, d);
        }
        _ => panic!("different exact tree shapes"),
    }
}

fn assert_policy_failure<T>(result: std::thread::Result<T>) {
    let failure = result
        .err()
        .expect("source must reject this malformed input");
    assert!(
        !failure.is::<chialisp_oxide::ResourceExhausted>(),
        "host resource failure is inconclusive"
    );
    assert!(failure.is::<String>() || failure.is::<&str>());
}

#[test]
fn let_bindings_preserve_signed_padded_and_unbounded_arithmetic() {
    for (input, expected) in [
        (&[5][..], &[24][..]),
        (&[0, 5], &[24]),
        (&[0xfb], &[24]),
        (&[], &[0xff]),
        (&[0], &[0xff]),
        (&[1], &[]),
        (&[0xff, 0xff], &[]),
        (&[0x80], &[0x3f, 0xff]),
    ] {
        let environment = Tree::Pair(&Tree::Atom(input), &Tree::Atom(&[]));
        assert_eq!(
            modern_let_product::puzzle(&environment),
            Tree::Atom(expected)
        );
    }
    let beyond = atom(&[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let environment = pair(beyond, atom(&[]));
    let mut expected = vec![0xff; 33];
    expected[0] = 0;
    assert_eq!(
        modern_let_product::puzzle(&environment),
        Tree::Atom(&expected)
    );
}

#[test]
fn named_and_lambda_sources_return_exact_page_and_adversarial_lists() {
    let values = list(&[atom(&[]), atom(&[0, 1]), atom(&[0xff]), atom(&[0x7f])]);
    let expected = list(&[atom(&[1]), atom(&[2]), atom(&[]), atom(&[0, 0x80])]);
    let environment = pair(values, atom(&[]));
    exact_owned(&modern_named_map::puzzle(&environment), &expected);
    exact_owned(&modern_lambda_map::puzzle(&environment), &expected);
    let page = pair(list(&[atom(&[1]), atom(&[2]), atom(&[3])]), atom(&[]));
    let expected = list(&[atom(&[2]), atom(&[3]), atom(&[4])]);
    exact_owned(&modern_named_map::puzzle(&page), &expected);
    exact_owned(&modern_lambda_map::puzzle(&page), &expected);
}

#[test]
fn mapping_zero_is_nil_only_while_nonempty_atom_tails_reject() {
    let empty = pair(atom(&[]), atom(&[]));
    exact_owned(&modern_named_map::puzzle(&empty), &atom(&[]));
    exact_owned(&modern_lambda_map::puzzle(&empty), &atom(&[]));
    for values in [
        atom(&[0]),
        atom(&[0, 0]),
        atom(&[7]),
        pair(atom(&[1]), atom(&[0])),
        pair(atom(&[1]), atom(&[9])),
    ] {
        let environment = pair(values, atom(&[]));
        assert_policy_failure(std::panic::catch_unwind(|| {
            modern_named_map::puzzle(&environment)
        }));
        assert_policy_failure(std::panic::catch_unwind(|| {
            modern_lambda_map::puzzle(&environment)
        }));
    }
}

#[test]
fn extra_arguments_and_improper_environment_tails_are_ignored() {
    for tail in [
        atom(&[9]),
        list(&[atom(&[8]), pair(atom(&[7]), atom(&[6]))]),
    ] {
        let environment = pair(atom(&[5]), tail);
        assert_eq!(modern_let_product::puzzle(&environment), atom(&[24]));
        let environment = pair(list(&[atom(&[1])]), tail);
        let expected = list(&[atom(&[2])]);
        exact_owned(&modern_named_map::puzzle(&environment), &expected);
        exact_owned(&modern_lambda_map::puzzle(&environment), &expected);
    }
}

#[test]
fn missing_and_pair_arithmetic_arguments_fail_in_actual_sources() {
    for environment in [atom(&[]), atom(&[9])] {
        assert_policy_failure(std::panic::catch_unwind(|| {
            modern_let_product::puzzle(&environment)
        }));
        assert_policy_failure(std::panic::catch_unwind(|| {
            modern_named_map::puzzle(&environment)
        }));
        assert_policy_failure(std::panic::catch_unwind(|| {
            modern_lambda_map::puzzle(&environment)
        }));
    }
    let environment = pair(pair(atom(&[1]), atom(&[2])), atom(&[]));
    assert_policy_failure(std::panic::catch_unwind(|| {
        modern_let_product::puzzle(&environment)
    }));
    let environment = pair(list(&[pair(atom(&[1]), atom(&[2]))]), atom(&[]));
    assert_policy_failure(std::panic::catch_unwind(|| {
        modern_named_map::puzzle(&environment)
    }));
    assert_policy_failure(std::panic::catch_unwind(|| {
        modern_lambda_map::puzzle(&environment)
    }));
}
