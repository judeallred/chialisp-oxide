//! Executed regressions for the owned optimizer passes over pair-capable
//! programs. The off path remains the original lowering; exact outputs and
//! typed failures are the oracle. Explicit pairs fold and refine like borrowed
//! trees with total projections, strict children and no helper inlining. These
//! finite comparisons are not termination or compiler-preservation proofs.
#[path = "runtime_evaluation/support.rs"]
#[allow(dead_code)]
mod support;

use chialisp_oxide_ir::{Environment, Expr, Type};
use chialisp_oxide_vm::FailureCode;
use support::*;

fn pair_type(first: Type, rest: Type) -> Type {
    Type::Pair {
        first: Box::new(first),
        rest: Box::new(rest),
    }
}
fn cons(first: Expr, rest: Expr) -> Expr {
    Expr::Pair {
        first: Box::new(first),
        rest: Box::new(rest),
    }
}
fn pair_first(value: Expr) -> Expr {
    Expr::PairFirst {
        value: Box::new(value),
    }
}
fn pair_rest(value: Expr) -> Expr {
    Expr::PairRest {
        value: Box::new(value),
    }
}
fn tree_pair_type() -> Type {
    pair_type(Type::Tree, Type::Tree)
}
/// A rejection whose dead fallback is pair-typed, so both branches agree.
fn reject_pair() -> Expr {
    sequence(
        Expr::Reject {
            message: "explicit first failure".into(),
        },
        cons(tree_atom(""), tree_atom("")),
    )
}

/// The delegated-conditions shape: nested pair matches on a raw environment
/// whose projections re-test the same immutable value, returning an explicit
/// pair of the two projections.
fn nested_match() -> chialisp_oxide_ir::Program {
    let guarded = |value: Expr, body: Expr| branch(is_pair(value.clone()), body, reject_pair());
    let inner = binding(
        1,
        Type::Tree,
        first(local(0)),
        cons(local(1), rest(local(0))),
    );
    let body = guarded(
        arg(0),
        binding(
            0,
            Type::Tree,
            rest(arg(0)),
            guarded(
                local(0),
                branch(is_pair(arg(0)), inner.clone(), reject_pair()),
            ),
        ),
    );
    program(
        Environment::Raw,
        vec![Type::Tree],
        tree_pair_type(),
        body,
        vec![],
    )
}

#[test]
fn redundant_pair_tests_fold_in_a_pair_capable_entry_with_identical_outcomes() {
    let source = nested_match();
    let [(_, off), (_, on)] = modes(&source);
    assert!(
        on.len() < off.len(),
        "folding the repeated IsPair must shrink the optimized program"
    );
    let environment = list(&[atom(b"k"), atom(b"c"), atom(b"x")]);
    let expected = pair(&atom(b"c"), &list(&[atom(b"x")]));
    for code in [&off, &on] {
        accepted(run(code, &environment, 1_000_000), &expected);
        failed(run(code, &atom(b"k"), 1_000_000), FailureCode::Raise);
        failed(
            run(code, &pair(&atom(b"k"), &atom(b"z")), 1_000_000),
            FailureCode::Raise,
        );
    }
}

#[test]
fn explicit_pair_children_stay_strict_even_when_discarded() {
    // (first (Pair Reject x)) must still reject: the projection is total by
    // typing but both children evaluate before any projection.
    let discarded = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        pair_rest(cons(reject(&Type::Tree), arg(0))),
        vec![],
    );
    let unused = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        binding(
            0,
            tree_pair_type(),
            cons(arg(0), reject(&Type::Tree)),
            arg(0),
        ),
        vec![],
    );
    for source in [discarded, unused] {
        for (_, code) in modes(&source) {
            failed(run(&code, &atom(b"a"), 1_000_000), FailureCode::Raise);
        }
    }
    let projected = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        pair_first(pair_rest(cons(
            tree_atom("01"),
            cons(arg(0), tree_atom("02")),
        ))),
        vec![],
    );
    for (_, code) in modes(&projected) {
        accepted(run(&code, &atom(b"q"), 1_000_000), &atom(b"q"));
    }
}

#[test]
fn constant_children_fold_inside_pairs_and_pair_projections_shrink() {
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        pair_first(cons(
            branch(
                Expr::Bool { value: true },
                tree_atom("0a"),
                reject(&Type::Tree),
            ),
            arg(0),
        )),
        vec![],
    );
    let [(_, off), (_, on)] = modes(&source);
    assert!(on.len() < off.len());
    for code in [&off, &on] {
        accepted(run(code, &atom(b"z"), 1_000_000), &atom(&[0x0a]));
    }
}

#[test]
fn helpers_that_construct_pairs_are_not_inlined_but_still_agree() {
    let helper = function(
        0,
        vec![Type::Tree],
        tree_pair_type(),
        branch(
            is_pair(arg(0)),
            cons(first(arg(0)), rest(arg(0))),
            reject_pair(),
        ),
    );
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        pair_first(call(0, vec![arg(0)])),
        vec![helper],
    );
    let [(_, off), (_, on)] = modes(&source);
    let environment = pair(&atom(b"h"), &atom(b"t"));
    for code in [&off, &on] {
        accepted(run(code, &environment, 1_000_000), &atom(b"h"));
        failed(run(code, &atom(b"h"), 1_000_000), FailureCode::Raise);
    }
    assert!(on.len() <= off.len());
}

#[test]
fn facts_from_a_pair_branch_do_not_cross_a_diverging_call() {
    // A guard before a diverging helper must keep rejecting atoms before the
    // helper runs, in a pair-capable program as in a pair-free one.
    let spinner = spin(&Type::Tree);
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        tree_pair_type(),
        branch(
            is_pair(arg(0)),
            cons(sequence(call(0, vec![]), first(arg(0))), arg(0)),
            reject_pair(),
        ),
        vec![spinner],
    );
    for (_, code) in modes(&source) {
        failed(run(&code, &atom(b"a"), 50_000), FailureCode::Raise);
        failed(
            run(&code, &pair(&atom(b"a"), &atom(b"b")), 50_000),
            FailureCode::CostExceeded,
        );
    }
}
