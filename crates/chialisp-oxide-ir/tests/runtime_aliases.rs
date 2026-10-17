//! Execution tests for total aliases and static data, including the effects
//! that must remain. Old/new compiler artifact comparisons live in the corpus
//! cost experiment; off/on here is an independent behavioral control.
#[path = "runtime_evaluation/support.rs"]
#[allow(dead_code)] // Shared helpers also serve the wider runtime test suites.
mod support;

use chialisp_oxide_ir::{Environment, Expr, Type};
use chialisp_oxide_vm::FailureCode;
use support::*;

fn tuple_type(items: Vec<Type>) -> Type {
    Type::Tuple { items }
}

#[test]
fn resolved_alias_chains_rebase_after_calls_and_sibling_local_reuse() {
    let triple = tuple_type(vec![Type::Tree; 3]);
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        tuple_type(vec![triple, Type::Tree]),
        Expr::Tuple {
            items: vec![
                binding(
                    0,
                    Type::Tree,
                    arg(0),
                    binding(
                        1,
                        Type::Tree,
                        local(0),
                        Expr::Tuple {
                            items: vec![local(1), call(0, vec![arg(0)]), local(0)],
                        },
                    ),
                ),
                binding(0, Type::Tree, tree_atom("ff"), local(0)),
            ],
        },
        vec![rightmost(0, 0)],
    );
    let [(_, off), (_, on)] = modes(&source);
    for depth in [0, 1, 32] {
        let leaf = atom(&[0, 0, 1]);
        let environment = right_spine(depth, &leaf);
        let expected = list(&[
            list(&[environment.clone(), leaf, environment.clone()]),
            atom(&[255]),
        ]);
        let before = accepted(run(&off, &environment, FINITE_LIMIT), &expected);
        let after = accepted(run(&on, &environment, FINITE_LIMIT), &expected);
        assert!(after < before, "depth={depth}: off={before}, on={after}");
    }
}

#[test]
fn tuple_alias_types_and_bytes_survive_later_physical_frames() {
    let argument = tuple_type(vec![Type::Tree, Type::Bytes { len: 2 }]);
    let source = program(
        Environment::Strict,
        vec![argument.clone()],
        tuple_type(vec![Type::Tree, u8_type(), Type::Tree]),
        binding(
            0,
            argument,
            arg(0),
            binding(
                1,
                Type::Tree,
                call(0, vec![field(local(0), 0)]),
                Expr::Tuple {
                    items: vec![local(1), field(field(local(0), 1), 1), field(local(0), 0)],
                },
            ),
        ),
        vec![rightmost(0, 0)],
    );
    for (_, code) in modes(&source) {
        let leaf = atom(&[0, 1]);
        let tree = right_spine(8, &leaf);
        let environment = list(&[list(&[tree.clone(), atom(&[0, 128])])]);
        accepted(
            run(&code, &environment, FINITE_LIMIT),
            &list(&[leaf, uint(128), tree]),
        );
        for invalid in [atom(&[]), atom(&[128]), atom(&[0, 0, 128])] {
            let environment = list(&[list(&[atom(&[]), invalid])]);
            abi_rejected(run(&code, &environment, FINITE_LIMIT));
        }
    }
}

#[test]
fn a_discarded_checked_projection_still_fails_before_divergence() {
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        binding(
            0,
            Type::Tree,
            arg(0),
            sequence(first(local(0)), call(0, vec![])),
        ),
        vec![spin(&Type::Tree)],
    );
    for (_, code) in modes(&source) {
        // A direct first opcode and its equivalent folded environment path
        // have different primary-VM codes. Both must reject at the projection,
        // rather than entering the later nonterminating call.
        assert!(matches!(
            run(&code, &atom(&[]), LIMIT),
            Outcome::Failed(FailureCode::InvalidOpArg | FailureCode::PathIntoAtom)
        ));
        failed(
            run(&code, &pair(&atom(&[]), &atom(&[])), LIMIT),
            FailureCode::CostExceeded,
        );
    }
}

#[test]
fn ignored_constant_list_is_quoted_once_with_canonical_identity() {
    let source = program(
        Environment::Ignored,
        vec![],
        tuple_type(vec![u8_type(); 3]),
        Expr::Tuple {
            items: vec![int(1), int(2), int(3)],
        },
        vec![],
    );
    let [(_, off), (_, on)] = modes(&source);
    let reference = chialisp_oxide_reference::compile_expression("(q . (1 2 3))").unwrap();
    assert_eq!(on, reference);
    assert_eq!(on.len(), 9);
    let expected = list(&[uint(1), uint(2), uint(3)]);
    for environment in [atom(&[]), atom(&[0, 0]), pair(&atom(&[8]), &atom(&[255]))] {
        accepted(run(&off, &environment, LIMIT), &expected);
        assert_eq!(accepted(run(&on, &environment, LIMIT), &expected), 20);
    }
}

#[test]
fn static_pair_quoting_does_not_hide_rejection_or_nontermination() {
    for dynamic_first in [false, true] {
        for diverges in [false, true] {
            let dynamic = if diverges {
                call(0, vec![])
            } else {
                reject(&Type::Tree)
            };
            let literal = tree_atom("0000");
            let (first, rest) = if dynamic_first {
                (dynamic, literal)
            } else {
                (literal, dynamic)
            };
            let source = program(
                Environment::Raw,
                vec![Type::Tree],
                Type::Tree,
                Expr::TreePair {
                    first: Box::new(first),
                    rest: Box::new(rest),
                },
                if diverges {
                    vec![spin(&Type::Tree)]
                } else {
                    vec![]
                },
            );
            for (_, code) in modes(&source) {
                failed(
                    run(&code, &atom(&[]), LIMIT),
                    if diverges {
                        FailureCode::CostExceeded
                    } else {
                        FailureCode::Raise
                    },
                );
            }
        }
    }
}
