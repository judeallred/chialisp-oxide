use super::*;

fn equal(left: &OwnedTree<'_>, right: &OwnedTree<'_>) -> bool {
    match (left, right) {
        (OwnedTree::Atom(a), OwnedTree::Atom(b)) => a == b,
        (OwnedTree::Pair(a, b), OwnedTree::Pair(c, d)) => equal(a, c) && equal(b, d),
        _ => false,
    }
}

#[test]
fn finite_native_owned_values_preserve_bytes_tails_moves_and_failure() {
    let tail_value = OwnedTree::Pair(
        Box::new(OwnedTree::Atom(&[0, 1])),
        Box::new(OwnedTree::Atom(&[0, 0])),
    );
    let list = OwnedTree::Pair(
        Box::new(OwnedTree::Atom(&[0])),
        Box::new(OwnedTree::Atom(&[])),
    );
    let prefixed = prefix(list, tail_value);
    let expected = OwnedTree::Pair(
        Box::new(OwnedTree::Atom(&[0])),
        Box::new(OwnedTree::Pair(
            Box::new(OwnedTree::Atom(&[0, 1])),
            Box::new(OwnedTree::Atom(&[0, 0])),
        )),
    );
    assert!(equal(&prefixed, &expected));
    assert!(equal(&tail(prefixed), &tail(expected)));
    assert!(equal(
        &head(construct(OwnedTree::Atom(&[0, 1]))),
        &OwnedTree::Atom(&[1])
    ));
    assert!(equal(
        &boxed_identity(OwnedTree::Atom(&[0, 0])),
        &OwnedTree::Atom(&[0, 0])
    ));
    let (pair, moved) = borrow_then_move(OwnedTree::Atom(&[0, 1]));
    assert!(!pair);
    assert!(equal(&moved, &OwnedTree::Atom(&[0, 1])));
    assert!(equal(
        &snapshot(OwnedTree::Atom(&[0, 1])),
        &OwnedTree::Atom(&[0, 1])
    ));
    assert!(equal(
        &runtime_snapshot(OwnedTree::Atom(&[0, 1])),
        &OwnedTree::Atom(&[0, 1])
    ));
    let (pair, moved) = shared_box_then_move(OwnedTree::Atom(&[0, 0]));
    assert!(!pair);
    assert!(equal(&moved, &OwnedTree::Atom(&[0, 0])));
    assert_eq!(ignored_owned(OwnedTree::Atom(&[0])), 7);
    assert_eq!(unused_constructor(OwnedTree::Atom(&[0])), 7);
    assert_eq!(unused_overflow(254), 7);
    assert!(equal(&lazy(false, 255), &OwnedTree::Atom(&[7])));
    assert!(equal(&lazy(true, 254), &OwnedTree::Atom(&[7])));
    assert!(
        std::panic::catch_unwind(|| prefix(OwnedTree::Atom(&[0]), OwnedTree::Atom(&[]))).is_err()
    );
    assert!(std::panic::catch_unwind(|| unused_constructor(OwnedTree::Atom(&[]))).is_err());
    assert!(std::panic::catch_unwind(|| unused_overflow(255)).is_err());
    assert!(std::panic::catch_unwind(|| lazy(true, 255)).is_err());
    // The spin and argument-order controls are intentionally never run natively.
}
