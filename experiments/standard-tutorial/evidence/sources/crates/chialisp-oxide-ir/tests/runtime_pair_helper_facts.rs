//! Branch-local facts in v5 table bodies; strict pairs and proper call frames.
#[path = "runtime_evaluation/support.rs"]
#[allow(dead_code)]
mod support;

use chialisp_oxide_ir::{Environment, Expr, Function, Program, Type, optimize};
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
fn atom_value(value: Expr) -> Expr {
    Expr::TreeAtomValue {
        value: Box::new(value),
    }
}
fn copy_spine(id: usize) -> Function {
    function(
        id,
        vec![Type::Tree],
        Type::Tree,
        branch(
            is_pair(arg(0)),
            binding(
                0,
                Type::Tree,
                first(arg(0)),
                binding(
                    2,
                    Type::Tree,
                    local(0),
                    binding(
                        1,
                        Type::Tree,
                        call(id, vec![rest(arg(0))]),
                        binding(
                            3,
                            Type::Tree,
                            local(1),
                            Expr::TreePair {
                                first: Box::new(local(2)),
                                rest: Box::new(local(3)),
                            },
                        ),
                    ),
                ),
            ),
            Expr::TreeAtom {
                value: Box::new(atom_value(arg(0))),
            },
        ),
    )
}
fn raw(result: Type, body: Expr, functions: Vec<Function>) -> Program {
    program(Environment::Raw, vec![Type::Tree], result, body, functions)
}

#[test]
fn recursive_helper_facts_keep_physical_aliases_and_arbitrary_v5_tails() {
    let source = raw(
        pair_type(Type::Tree, Type::Tree),
        cons(call(0, vec![arg(0)]), arg(0)),
        vec![copy_spine(0)],
    );
    for (_, code) in modes(&source) {
        for leaf in [
            atom(&[]),
            atom(&[0]),
            atom(&[0, 0]),
            atom(&[0, 128]),
            atom(&[255]),
        ] {
            for depth in [0, 1, 2, 8, 32] {
                let input = right_spine(depth, &leaf);
                accepted(run(&code, &input, FINITE_LIMIT), &pair(&input, &input));
            }
            let input = pair(&pair(&atom(&[0, 0]), &atom(&[255])), &leaf);
            accepted(run(&code, &input, FINITE_LIMIT), &pair(&input, &input));
        }
    }
}

#[test]
fn pair_fields_and_projections_keep_strict_calls_through_public_v5_transport() {
    let functions = vec![
        copy_spine(0),
        function(1, vec![], Type::Tree, call(1, vec![])),
    ];
    let helper_then_raise = sequence(call(0, vec![arg(0)]), reject(&Type::Tree));
    let bad_hash = Expr::Sha256 {
        parts: vec![atom_value(Expr::TreePair {
            first: Box::new(tree_atom("")),
            rest: Box::new(tree_atom("")),
        })],
    };
    for (body, expected) in [
        (
            pair_first(cons(helper_then_raise.clone(), call(1, vec![]))),
            FailureCode::Raise,
        ),
        (
            pair_first(cons(call(1, vec![]), helper_then_raise)),
            FailureCode::CostExceeded,
        ),
        (
            pair_rest(cons(reject(&Type::Tree), call(0, vec![arg(0)]))),
            FailureCode::Raise,
        ),
        (
            pair_first(cons(call(0, vec![arg(0)]), bad_hash)),
            FailureCode::Raise,
        ),
        (
            pair_first(cons(call(0, vec![arg(0)]), add(int(255), int(1)))),
            FailureCode::Raise,
        ),
    ] {
        let source = raw(Type::Tree, body, functions.clone());
        for candidate in [source.clone(), optimize(&source).unwrap()] {
            let wire = serde_json::to_vec(&candidate).unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&wire).unwrap()["ir_version"],
                5
            );
            let reread: Program = serde_json::from_slice(&wire).unwrap();
            for (_, code) in modes(&reread) {
                failed(
                    run(&code, &pair(&atom(&[0]), &atom(&[255])), LIMIT),
                    expected,
                );
            }
        }
        // CostExceeded is a retained nontermination observation, not rejection.
    }
}

#[test]
fn discarded_nontotal_helper_calls_and_unguarded_projections_still_fail_first() {
    let rejecting = function(
        0,
        vec![Type::Tree],
        Type::Atom,
        branch(
            is_pair(arg(0)),
            Expr::Reject {
                message: "pair".into(),
            },
            atom_value(arg(0)),
        ),
    );
    let unused = raw(
        pair_type(Type::Tree, Type::Tree),
        binding(0, Type::Atom, call(0, vec![arg(0)]), cons(arg(0), arg(0))),
        vec![rejecting],
    );
    let unguarded = raw(
        pair_type(Type::Tree, Type::Tree),
        cons(call(0, vec![arg(0)]), arg(0)),
        vec![function(
            0,
            vec![Type::Tree],
            Type::Tree,
            sequence(first(arg(0)), reject(&Type::Tree)),
        )],
    );
    for (_, code) in modes(&unused) {
        failed(
            run(&code, &pair(&atom(&[]), &atom(&[])), LIMIT),
            FailureCode::Raise,
        );
        for input in [atom(&[]), atom(&[0, 0]), atom(&[255])] {
            accepted(run(&code, &input, FINITE_LIMIT), &pair(&input, &input));
        }
    }
    for (optimize, code) in modes(&unguarded) {
        let projection_fault = if optimize {
            FailureCode::PathIntoAtom
        } else {
            FailureCode::InvalidOpArg
        };
        failed(run(&code, &atom(&[]), LIMIT), projection_fault);
        failed(
            run(&code, &pair(&atom(&[]), &atom(&[])), LIMIT),
            FailureCode::Raise,
        );
    }
}

#[test]
fn recursive_helpers_start_with_fresh_facts_for_the_new_argument() {
    let helper = function(
        0,
        vec![Type::Tree, Type::Bool],
        Type::Tree,
        branch(
            arg(1),
            branch(
                is_pair(arg(0)),
                binding(
                    0,
                    Type::Tree,
                    rest(arg(0)),
                    call(0, vec![local(0), Expr::Bool { value: false }]),
                ),
                reject(&Type::Tree),
            ),
            first(arg(0)),
        ),
    );
    let source = raw(
        pair_type(Type::Tree, Type::Tree),
        cons(call(0, vec![arg(0), Expr::Bool { value: true }]), arg(0)),
        vec![helper],
    );
    for (optimize, code) in modes(&source) {
        let projection_fault = if optimize {
            FailureCode::PathIntoAtom
        } else {
            FailureCode::InvalidOpArg
        };
        failed(run(&code, &atom(&[]), LIMIT), FailureCode::Raise);
        failed(
            run(&code, &pair(&atom(&[]), &atom(&[0, 0])), LIMIT),
            projection_fault,
        );
        let input = pair(&atom(&[255]), &pair(&atom(&[0, 0]), &atom(&[128])));
        accepted(
            run(&code, &input, FINITE_LIMIT),
            &pair(&atom(&[0, 0]), &input),
        );
    }
}

#[test]
fn malformed_unused_function_is_validated_before_helper_selection() {
    let mut source = raw(
        pair_type(Type::Tree, Type::Tree),
        cons(call(0, vec![arg(0)]), arg(0)),
        vec![copy_spine(0)],
    );
    source
        .functions
        .push(function(1, vec![], Type::Tree, call(99, vec![])));
    assert!(source.validate().is_err());
    assert!(optimize(&source).is_err());
    for optimize in [false, true] {
        assert!(chialisp_oxide_ir::compile(&source, optimize).is_err());
    }
}

#[test]
fn pair_returning_helpers_rebase_branch_facts_and_keep_unused_calls_strict() {
    let result = pair_type(Type::Tree, Type::Tree);
    for (effect, failure) in [
        (tree_atom("ff"), None),
        (reject(&Type::Tree), Some(FailureCode::Raise)),
        (call(1, vec![]), Some(FailureCode::CostExceeded)),
    ] {
        let helper = function(
            0,
            vec![Type::Tree],
            result.clone(),
            branch(
                is_pair(arg(0)),
                binding(
                    0,
                    Type::Tree,
                    first(arg(0)),
                    binding(
                        1,
                        Type::Tree,
                        local(0),
                        binding(
                            2,
                            Type::Tree,
                            rest(arg(0)),
                            binding(3, Type::Tree, call(1, vec![]), cons(local(1), local(2))),
                        ),
                    ),
                ),
                cons(arg(0), arg(0)),
            ),
        );
        let source = raw(
            result.clone(),
            call(0, vec![arg(0)]),
            vec![helper, function(1, vec![], Type::Tree, effect)],
        );
        for candidate in [source.clone(), optimize(&source).unwrap()] {
            for (_, code) in modes(&candidate) {
                // The other branch must not inherit a pair fact or run its call.
                for input in [atom(&[]), atom(&[0, 0]), atom(&[0, 128]), atom(&[255])] {
                    accepted(run(&code, &input, LIMIT), &pair(&input, &input));
                }
                for input in [
                    pair(&atom(&[0, 0]), &atom(&[128])),
                    pair(&pair(&atom(&[255]), &atom(&[])), &atom(&[0])),
                ] {
                    let execution = run(&code, &input, LIMIT);
                    if let Some(expected) = failure {
                        // Divergence is resource exhaustion, never rejection credit.
                        failed(execution, expected);
                    } else {
                        accepted(execution, &input);
                    }
                }
            }
        }
    }
}

#[test]
fn pair_returning_helper_keeps_unguarded_fault_before_divergent_tail() {
    let result = pair_type(Type::Tree, Type::Tree);
    let source = raw(
        result.clone(),
        call(0, vec![arg(0)]),
        vec![
            function(
                0,
                vec![Type::Tree],
                result,
                cons(first(arg(0)), call(1, vec![])),
            ),
            function(1, vec![], Type::Tree, call(1, vec![])),
        ],
    );
    for (optimized, code) in modes(&source) {
        failed(
            run(&code, &atom(&[]), LIMIT),
            if optimized {
                FailureCode::PathIntoAtom
            } else {
                FailureCode::InvalidOpArg
            },
        );
        failed(
            run(&code, &pair(&atom(&[]), &atom(&[0])), LIMIT),
            FailureCode::CostExceeded,
        );
    }
}
