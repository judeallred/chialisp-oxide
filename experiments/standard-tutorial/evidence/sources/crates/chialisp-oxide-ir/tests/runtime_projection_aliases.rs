//! Guarded projections may forward immutable reads, never their strict roots.
//! Independent wire trees exercise rebasing and typed failure order in both modes.
#[path = "runtime_evaluation/support.rs"]
#[allow(dead_code)]
mod support;

use chialisp_oxide_ir::{Environment, Expr, Type, compile};
use chialisp_oxide_vm::FailureCode;
use support::*;

fn not(value: Expr) -> Expr {
    Expr::Not {
        value: Box::new(value),
    }
}
fn tuple(items: Vec<Expr>) -> Expr {
    Expr::Tuple { items }
}
fn trees(count: usize) -> Type {
    Type::Tuple {
        items: vec![Type::Tree; count],
    }
}
fn pair_value(first: Expr, rest: Expr) -> Expr {
    Expr::TreePair {
        first: Box::new(first),
        rest: Box::new(rest),
    }
}
fn guarded_nested() -> chialisp_oxide_ir::Program {
    program(
        Environment::Raw,
        vec![Type::Tree],
        trees(6),
        branch(
            is_pair(arg(0)),
            binding(
                0,
                Type::Tree,
                first(arg(0)),
                binding(
                    1,
                    Type::Tree,
                    rest(arg(0)),
                    branch(
                        not(is_pair(local(1))),
                        Expr::Reject {
                            message: "second pair required".into(),
                        },
                        binding(
                            2,
                            Type::Tree,
                            first(local(1)),
                            binding(
                                3,
                                Type::Tree,
                                rest(local(1)),
                                tuple(vec![
                                    local(0),
                                    call(0, vec![arg(0)]),
                                    local(2),
                                    local(3),
                                    local(1),
                                    arg(0),
                                ]),
                            ),
                        ),
                    ),
                ),
            ),
            Expr::Reject {
                message: "first pair required".into(),
            },
        ),
        vec![rightmost(0, 0)],
    )
}

#[test]
fn guarded_nested_aliases_rebase_after_a_physical_call_frame() {
    let source = guarded_nested();
    for (_, code) in modes(&source) {
        for first_atom in [atom(&[]), atom(&[0]), atom(&[0, 0, 128])] {
            for second_atom in [atom(&[255]), atom(&[0, 0])] {
                let tail = atom(&[128]);
                let second = pair(&second_atom, &tail);
                let environment = pair(&first_atom, &second);
                accepted(
                    run(&code, &environment, FINITE_LIMIT),
                    &list(&[
                        first_atom.clone(),
                        tail.clone(),
                        second_atom,
                        tail,
                        second,
                        environment.clone(),
                    ]),
                );
            }
        }
        for malformed in [atom(&[]), atom(&[0]), pair(&atom(&[1]), &atom(&[0, 0]))] {
            failed(run(&code, &malformed, LIMIT), FailureCode::Raise);
        }
    }
}

#[test]
fn guarded_aliases_have_no_projection_continuation_cost() {
    // A focused cost contract for the new optimization. The preceding compiler
    // emits 103 bytes here; forwarding two already-safe reads should eliminate
    // their frames without changing the guard or the input domain.
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        trees(3),
        branch(
            is_pair(arg(0)),
            binding(
                0,
                Type::Tree,
                first(arg(0)),
                binding(
                    1,
                    Type::Tree,
                    rest(arg(0)),
                    tuple(vec![local(0), local(1), local(0)]),
                ),
            ),
            Expr::Reject {
                message: "pair required".into(),
            },
        ),
        vec![],
    );
    let on = compile(&source, true).unwrap();
    let environment = pair(&atom(&[0, 0]), &atom(&[255]));
    let cost = accepted(
        run(&on, &environment, LIMIT),
        &list(&[atom(&[0, 0]), atom(&[255]), atom(&[0, 0])]),
    );
    assert!(
        on.len() < 103 && cost < 1721,
        "serialized bytes={}, VM cost={cost}",
        on.len()
    );
}

#[test]
fn physical_root_snapshots_survive_aliases_frames_and_sibling_local_reuse() {
    // The recursive source returns the original tree after traversing it. Its
    // call is strict and captured once; only later guarded reads can be aliases.
    let walk = function(
        0,
        vec![Type::Tree],
        Type::Tree,
        branch(
            is_pair(arg(0)),
            sequence(call(0, vec![rest(arg(0))]), arg(0)),
            arg(0),
        ),
    );
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        trees(3),
        tuple(vec![
            binding(
                0,
                Type::Tree,
                call(0, vec![arg(0)]),
                binding(
                    1,
                    Type::Tree,
                    local(0),
                    branch(
                        not(not(is_pair(local(1)))),
                        binding(
                            2,
                            Type::Tree,
                            first(local(1)),
                            binding(
                                3,
                                Type::Tree,
                                local(2),
                                sequence(call(0, vec![arg(0)]), local(3)),
                            ),
                        ),
                        Expr::Reject {
                            message: "pair required".into(),
                        },
                    ),
                ),
            ),
            binding(0, Type::Tree, tree_atom("ffff"), local(0)),
            binding(
                0,
                Type::Tree,
                call(0, vec![arg(0)]),
                branch(
                    is_pair(local(0)),
                    binding(1, Type::Tree, rest(local(0)), local(1)),
                    Expr::Reject {
                        message: "pair required".into(),
                    },
                ),
            ),
        ]),
        vec![walk],
    );
    for (_, code) in modes(&source) {
        for depth in [1, 2, 24] {
            let leaf = atom(&[0, 128]);
            let environment = right_spine(depth, &leaf);
            accepted(
                run(&code, &environment, FINITE_LIMIT),
                &list(&[atom(&[0]), atom(&[255, 255]), right_spine(depth - 1, &leaf)]),
            );
        }
    }
}

#[test]
fn branch_and_callee_facts_do_not_escape_to_unrelated_roots() {
    let invalid = sequence(
        binding(0, Type::Tree, first(arg(0)), tree_atom("")),
        call(0, vec![]),
    );
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        branch(is_pair(arg(0)), tree_atom("01"), invalid),
        vec![spin(&Type::Tree)],
    );
    for (_, code) in modes(&source) {
        assert!(matches!(
            run(&code, &atom(&[]), LIMIT),
            Outcome::Failed(FailureCode::InvalidOpArg | FailureCode::PathIntoAtom)
        ));
        accepted(
            run(&code, &pair(&atom(&[]), &atom(&[])), LIMIT),
            &atom(&[1]),
        );
    }
    let callee = function(
        1,
        vec![Type::Tree],
        Type::Tree,
        sequence(
            binding(0, Type::Tree, first(arg(0)), tree_atom("")),
            call(0, vec![]),
        ),
    );
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        branch(
            is_pair(arg(0)),
            call(1, vec![tree_atom("")]),
            tree_atom("01"),
        ),
        vec![spin(&Type::Tree), callee],
    );
    for (_, code) in modes(&source) {
        assert!(matches!(
            run(&code, &pair(&atom(&[]), &atom(&[])), LIMIT),
            Outcome::Failed(FailureCode::InvalidOpArg | FailureCode::PathIntoAtom)
        ));
    }
}

#[test]
fn a_guarded_alias_does_not_make_an_unchecked_child_total() {
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        branch(
            is_pair(arg(0)),
            binding(
                0,
                Type::Tree,
                first(arg(0)),
                binding(1, Type::Tree, first(local(0)), call(0, vec![])),
            ),
            Expr::Reject {
                message: "pair required".into(),
            },
        ),
        vec![spin(&Type::Tree)],
    );
    for (_, code) in modes(&source) {
        assert!(matches!(
            run(&code, &pair(&atom(&[]), &atom(&[])), LIMIT),
            Outcome::Failed(FailureCode::InvalidOpArg | FailureCode::PathIntoAtom)
        ));
        failed(
            run(
                &code,
                &pair(&pair(&atom(&[]), &atom(&[])), &atom(&[])),
                LIMIT,
            ),
            FailureCode::CostExceeded,
        );
    }
}

#[test]
fn constructor_payloads_and_guard_operands_keep_source_failure_order() {
    for (first_payload, rest_payload, expected) in [
        (reject(&Type::Tree), call(0, vec![]), FailureCode::Raise),
        (
            call(0, vec![]),
            reject(&Type::Tree),
            FailureCode::CostExceeded,
        ),
    ] {
        let source = program(
            Environment::Raw,
            vec![Type::Tree],
            Type::Tree,
            binding(
                0,
                Type::Tree,
                pair_value(first_payload, rest_payload),
                branch(
                    is_pair(local(0)),
                    binding(1, Type::Tree, first(local(0)), tree_atom("")),
                    tree_atom(""),
                ),
            ),
            vec![spin(&Type::Tree)],
        );
        for (_, code) in modes(&source) {
            failed(run(&code, &atom(&[]), LIMIT), expected);
        }
    }
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        branch(
            is_pair(sequence(call(0, vec![]), arg(0))),
            binding(0, Type::Tree, first(arg(0)), tree_atom("")),
            tree_atom(""),
        ),
        vec![spin(&Type::Tree)],
    );
    for (_, code) in modes(&source) {
        failed(
            run(&code, &pair(&atom(&[]), &atom(&[])), LIMIT),
            FailureCode::CostExceeded,
        );
    }
}

#[test]
fn a_future_pair_check_cannot_move_before_a_failing_projection() {
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        binding(
            0,
            Type::Tree,
            first(arg(0)),
            branch(is_pair(arg(0)), call(0, vec![]), tree_atom("")),
        ),
        vec![spin(&Type::Tree)],
    );
    for (_, code) in modes(&source) {
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
fn a_captured_effectful_root_is_not_recomputed_by_projected_aliases() {
    let walk = function(
        0,
        vec![Type::Tree],
        Type::Tree,
        branch(
            is_pair(arg(0)),
            sequence(call(0, vec![rest(arg(0))]), arg(0)),
            arg(0),
        ),
    );
    let body = binding(
        0,
        Type::Tree,
        call(0, vec![arg(0)]),
        branch(
            is_pair(local(0)),
            binding(
                1,
                Type::Tree,
                first(local(0)),
                tuple(vec![local(1), local(1), local(0)]),
            ),
            Expr::Reject {
                message: "pair required".into(),
            },
        ),
    );
    let once = program(
        Environment::Raw,
        vec![Type::Tree],
        trees(3),
        body.clone(),
        vec![walk.clone()],
    );
    let twice = program(
        Environment::Raw,
        vec![Type::Tree],
        trees(3),
        sequence(call(0, vec![arg(0)]), body),
        vec![walk],
    );
    let environment = right_spine(64, &atom(&[0, 128]));
    let expected = list(&[atom(&[0]), atom(&[0]), environment.clone()]);
    for mode in [false, true] {
        let once_code = compile(&once, mode).unwrap();
        let twice_code = compile(&twice, mode).unwrap();
        let once_cost = accepted(run(&once_code, &environment, FINITE_LIMIT), &expected);
        let twice_cost = accepted(run(&twice_code, &environment, FINITE_LIMIT), &expected);
        assert!(once_cost < twice_cost);
        let budget = once_cost + (twice_cost - once_cost) / 2;
        accepted(run(&once_code, &environment, budget), &expected);
        failed(
            run(&twice_code, &environment, budget),
            FailureCode::CostExceeded,
        );
    }
}

#[test]
fn guarded_reads_do_not_erase_unused_hashes_or_checked_overflow() {
    let hashed = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        branch(
            is_pair(arg(0)),
            binding(
                0,
                Type::Tree,
                first(arg(0)),
                binding(
                    1,
                    Type::Bytes { len: 32 },
                    Expr::Sha256 {
                        parts: vec![Expr::TreeAtomValue {
                            value: Box::new(local(0)),
                        }],
                    },
                    tree_atom(""),
                ),
            ),
            Expr::Reject {
                message: "pair required".into(),
            },
        ),
        vec![],
    );
    for (_, code) in modes(&hashed) {
        failed(
            run(
                &code,
                &pair(&pair(&atom(&[]), &atom(&[])), &atom(&[])),
                LIMIT,
            ),
            FailureCode::Raise,
        );
        let short = accepted(
            run(&code, &pair(&atom(&[]), &atom(&[])), FINITE_LIMIT),
            &atom(&[]),
        );
        // Build a long atom independently; the deliberately unused SHA must
        // still consume its payload and increase actual VM work.
        let mut long = vec![0xc0, 0x40];
        long.extend([7; 64]);
        let long_cost = accepted(
            run(&code, &pair(&long, &atom(&[])), FINITE_LIMIT),
            &atom(&[]),
        );
        assert!(long_cost > short);
    }
    let overflow = program(
        Environment::Strict,
        vec![Type::Tree, u8_type()],
        Type::Tree,
        branch(
            is_pair(arg(0)),
            binding(
                0,
                Type::Tree,
                first(arg(0)),
                binding(1, u8_type(), add(arg(1), int(1)), local(0)),
            ),
            Expr::Reject {
                message: "pair required".into(),
            },
        ),
        vec![],
    );
    for (_, code) in modes(&overflow) {
        let tree = pair(&atom(&[0, 0]), &atom(&[]));
        accepted(
            run(&code, &list(&[tree.clone(), uint(254)]), LIMIT),
            &atom(&[0, 0]),
        );
        failed(
            run(&code, &list(&[tree, uint(255)]), LIMIT),
            FailureCode::Raise,
        );
    }
}

#[test]
fn same_local_ids_in_siblings_do_not_transfer_a_pair_fact() {
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        trees(2),
        tuple(vec![
            binding(
                0,
                Type::Tree,
                call(1, vec![arg(0)]),
                branch(
                    is_pair(local(0)),
                    binding(1, Type::Tree, first(local(0)), local(1)),
                    tree_atom(""),
                ),
            ),
            binding(
                0,
                Type::Tree,
                tree_atom(""),
                binding(1, Type::Tree, first(local(0)), call(0, vec![])),
            ),
        ]),
        vec![
            spin(&Type::Tree),
            function(
                1,
                vec![Type::Tree],
                Type::Tree,
                branch(is_pair(arg(0)), arg(0), call(1, vec![arg(0)])),
            ),
        ],
    );
    for (_, code) in modes(&source) {
        assert!(matches!(
            run(&code, &pair(&atom(&[]), &atom(&[])), LIMIT),
            Outcome::Failed(FailureCode::InvalidOpArg | FailureCode::PathIntoAtom)
        ));
    }
}
