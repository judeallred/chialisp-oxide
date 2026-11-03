#![cfg(feature = "vm")]
use chialisp_oxide::{Tree, raw_clvm as raw};

#[test]
fn exact_byte_operations_preserve_nil_padding_and_high_bits() {
    let input = Tree::Atom(&[0, 0x80, 0xff]);
    assert_eq!(raw::byte_length(&input), Tree::Atom(&[3]));
    assert_eq!(raw::byte_length(&Tree::Atom(&[])), Tree::Atom(&[]));
    assert_eq!(raw::byte_concat(&input, &Tree::Atom(&[])), input);
    assert_eq!(
        raw::byte_concat(&input, &Tree::Atom(&[0])),
        Tree::Atom(&[0, 0x80, 0xff, 0])
    );
    assert_eq!(
        raw::byte_substr(&input, &Tree::Atom(&[1]), &Tree::Atom(&[3])),
        Tree::Atom(&[0x80, 0xff])
    );
    assert_eq!(
        raw::byte_substr(&input, &Tree::Atom(&[3]), &Tree::Atom(&[3])),
        Tree::Atom(&[])
    );
}

#[test]
fn raw_integer_floor_division_differs_from_rust_and_preserves_large_atoms() {
    let negative_seven = Tree::Atom(&[0xf9]);
    let three = Tree::Atom(&[3]);
    assert_eq!(raw::div(&negative_seven, &three), Tree::Atom(&[0xfd]));
    assert_eq!(raw::modulo(&negative_seven, &three), Tree::Atom(&[2]));
    assert_eq!(
        raw::divmod(&negative_seven, &three),
        Tree::Pair(&Tree::Atom(&[0xfd]), &Tree::Atom(&[2]))
    );
    let large = Tree::Atom(&[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(raw::div(&large, &Tree::Atom(&[1])), large);
    assert_eq!(raw::div(&Tree::Atom(&[0, 7]), &three), Tree::Atom(&[2]));
}

#[test]
fn raw_truth_is_nil_only_and_boolean_combinators_are_not_short_circuiting() {
    let nil = Tree::Atom(&[]);
    let one = Tree::Atom(&[1]);
    let zero_byte = Tree::Atom(&[0]);
    let pair = Tree::Pair(&nil, &nil);
    assert_eq!(raw::not(&nil), one);
    assert_eq!(raw::not(&zero_byte), nil);
    assert_eq!(raw::not(&pair), nil);
    assert_eq!(raw::any2(&nil, &zero_byte), one);
    assert_eq!(raw::all2(&one, &pair), one);
    assert_eq!(raw::all2(&one, &nil), nil);
}

#[test]
fn malformed_operands_reject_as_program_failures() {
    let nil = Tree::Atom(&[]);
    let pair = Tree::Pair(&nil, &nil);
    for result in [
        std::panic::catch_unwind(|| raw::byte_length(&pair)),
        std::panic::catch_unwind(|| raw::byte_concat(&nil, &pair)),
        std::panic::catch_unwind(|| {
            raw::byte_substr(&Tree::Atom(&[1]), &Tree::Atom(&[2]), &Tree::Atom(&[1]))
        }),
        std::panic::catch_unwind(|| raw::div(&Tree::Atom(&[1]), &nil)),
    ] {
        let payload = result.expect_err("operator must reject");
        assert!(
            !payload.is::<chialisp_oxide::ResourceExhausted>(),
            "host resource failure is inconclusive"
        );
    }
}

#[test]
fn raw_unbounded_math_and_byte_order_are_distinct() {
    let large_bytes = [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let large = Tree::Atom(&large_bytes);
    let one = Tree::Atom(&[1]);
    let minus_one = Tree::Atom(&[255]);
    let mut plus_one = large_bytes;
    plus_one[16] = 1;
    assert_eq!(raw::add(&large, &one), Tree::Atom(&plus_one));
    assert_eq!(raw::sub(&large, &minus_one), Tree::Atom(&plus_one));
    assert_eq!(
        raw::mul(&large, &Tree::Atom(&[2])),
        Tree::Atom(&[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
    );
    assert_eq!(raw::bitand(&large, &minus_one), large);
    assert_eq!(raw::bitor(&large, &one), Tree::Atom(&plus_one));
    assert_eq!(raw::bitxor(&large, &one), Tree::Atom(&plus_one));
    assert_eq!(raw::lognot(&minus_one), Tree::Atom(&[]));
    assert_eq!(raw::arithmetic_shift(&minus_one, &minus_one), minus_one);
    assert_eq!(
        raw::logical_shift(&minus_one, &minus_one),
        Tree::Atom(&[127])
    );
    assert_eq!(raw::atom_equal(&Tree::Atom(&[0, 1]), &one), Tree::Atom(&[]));
    assert_eq!(
        raw::integer_greater(&Tree::Atom(&[0, 1]), &one),
        Tree::Atom(&[])
    );
    assert_eq!(
        raw::atom_greater(&Tree::Atom(&[128]), &Tree::Atom(&[127])),
        one
    );
}

#[test]
fn raw_coin_id_hashes_exact_validated_components() {
    use sha2::{Digest, Sha256};
    let hash = Tree::Atom(&[0; 32]);
    let zero = Tree::Atom(&[]);
    let expected: [u8; 32] = Sha256::digest([0; 64]).into();
    assert_eq!(raw::coin_id(&hash, &hash, &zero), Tree::Atom(&expected));
    for amount in [
        &[0][..],
        &[0, 1][..],
        &[255][..],
        &[1, 0, 0, 0, 0, 0, 0, 0, 0][..],
    ] {
        let failure = std::panic::catch_unwind(|| raw::coin_id(&hash, &hash, &Tree::Atom(amount)))
            .expect_err("invalid coin amount must reject");
        assert!(!failure.is::<chialisp_oxide::ResourceExhausted>());
    }
}

#[test]
fn raw_strict_selection_uses_nil_truth() {
    let one = Tree::Atom(&[1]);
    let two = Tree::Atom(&[2]);
    assert_eq!(raw::strict_if3(&Tree::Atom(&[]), &one, &two), two);
    assert_eq!(raw::strict_if3(&Tree::Atom(&[0]), &one, &two), one);
    let failure = std::panic::catch_unwind(|| {
        raw::strict_if3(&Tree::Atom(&[]), &raw::div(&one, &Tree::Atom(&[])), &two)
    })
    .expect_err("discarded branch value is evaluated before the call");
    assert!(!failure.is::<chialisp_oxide::ResourceExhausted>());
}

// These are direct public-facade native calls, bypassing all corpus codecs.
// Inputs use leaked immutable references solely to avoid recursive destruction
// or recursive test-data construction obscuring the native oracle boundary.
fn native_resource_nil() -> &'static Tree<'static> {
    static NIL: Tree<'static> = Tree::Atom(&[]);
    &NIL
}
fn native_resource_list(value: &'static Tree<'static>, count: usize) -> &'static Tree<'static> {
    let mut tail = native_resource_nil();
    for _ in 0..count {
        tail = Box::leak(Box::new(Tree::Pair(value, tail)));
    }
    tail
}
fn native_resource_dag(depth: usize) -> &'static Tree<'static> {
    let mut value = native_resource_nil();
    for _ in 0..depth {
        value = Box::leak(Box::new(Tree::Pair(value, value)));
    }
    value
}
fn expect_native_list_resource(call: impl FnOnce() -> Tree<'static> + std::panic::UnwindSafe) {
    let failure = std::panic::catch_unwind(call).expect_err("native list resource bound");
    assert!(
        failure.is::<chialisp_oxide::ResourceExhausted>(),
        "resource failure became policy panic"
    );
}
#[test]
fn native_operand_list_preflights_depth_spine_bytes_and_expanded_shared_values() {
    // A direct call previously recursed without a complete-spine limit. Every
    // first value is nil, so each old local cons/apply preflight was tiny.
    let deep = native_resource_list(native_resource_nil(), 10_000);
    expect_native_list_resource(|| raw::add_list(deep));
    let too_many = native_resource_list(native_resource_nil(), 129);
    expect_native_list_resource(|| raw::any_list(too_many));
    let large = Box::leak(vec![0u8; 1_048_577].into_boxed_slice());
    let large = Box::leak(Box::new(Tree::Atom(large)));
    let one_large = native_resource_list(large, 1);
    expect_native_list_resource(|| raw::concat_list(one_large));
    // Seventeen distinct objects expand beyond the full-tree node budget.
    let dag = native_resource_list(native_resource_dag(16), 1);
    expect_native_list_resource(|| raw::any_list(dag));
    // Nodes/bytes/depth individually fit, but cumulative repeated suffix
    // materialization would exceed its independently checked copy budgets.
    let projected_nodes = native_resource_list(native_resource_dag(6), 128);
    expect_native_list_resource(|| raw::any_list(projected_nodes));
    let atom = Box::leak(vec![0u8; 4096].into_boxed_slice());
    let atom = Box::leak(Box::new(Tree::Atom(atom)));
    let projected_bytes = native_resource_list(atom, 64);
    expect_native_list_resource(|| raw::concat_list(projected_bytes));
    // A small boundary input still executes through the real native VM route.
    let bounded = native_resource_list(native_resource_nil(), 128);
    assert_eq!(raw::add_list(bounded), Tree::Atom(&[]));
}
#[test]
fn native_list_resource_preflight_checks_full_siblings_before_policy_failure() {
    let nil = native_resource_nil();
    let bad_tail = Tree::Atom(&[1]);
    let small_bad = Tree::Pair(nil, &bad_tail);
    let policy = std::panic::catch_unwind(|| raw::all_list(&small_bad))
        .expect_err("false first value cannot short-circuit improper tail");
    assert!(!policy.is::<chialisp_oxide::ResourceExhausted>());
    // The oversized malformed tail is visited before the original semantic
    // proper-list check. It must not become evidence of policy rejection.
    let large = vec![0u8; 1_048_577];
    let large_tail = Tree::Atom(&large);
    let oversized_bad = Tree::Pair(nil, &large_tail);
    expect_native_list_resource(|| raw::all_list(&oversized_bad));
    // A pair-valued, policy-invalid numeric sibling does not excuse scanning
    // the other sibling's complete expanded tree before entering the policy.
    let invalid_number = Tree::Pair(nil, nil);
    let huge_value = native_resource_dag(16);
    let remainder = Tree::Pair(huge_value, nil);
    let siblings = Tree::Pair(&invalid_number, &remainder);
    expect_native_list_resource(|| raw::add_list(&siblings));
}
