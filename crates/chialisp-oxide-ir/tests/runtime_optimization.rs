//! Adversarial executions for optimized runtime lowering. The off path remains
//! the original CPS baseline. Exact outputs and typed failures are the oracle;
//! measured costs only distinguish explicitly once- and twice-evaluated calls.
//! These finite tests do not establish termination or compiler preservation.
#[path = "runtime_evaluation/support.rs"]
mod support;

use chialisp_oxide_ir::{Environment, Expr, Type};
use chialisp_oxide_vm::FailureCode;
use support::*;

fn tuple(items: Vec<Expr>) -> Expr {
    Expr::Tuple { items }
}

fn tuple_type(items: Vec<Type>) -> Type {
    Type::Tuple { items }
}

fn atom_value(value: Expr) -> Expr {
    Expr::TreeAtomValue {
        value: Box::new(value),
    }
}

#[test]
fn leaf_descriptors_rebase_after_fallible_arguments_and_reduce_forwarding_cost() {
    let mut value_types = vec![Type::Tree; 5];
    value_types.extend([
        Type::Unit,
        Type::Bool,
        Type::Bytes { len: 2 },
        Type::Atom,
        u8_type(),
    ]);
    let result = tuple_type(value_types.clone());
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        result.clone(),
        binding(
            0,
            Type::Tree,
            tree_atom("abcd"),
            call(
                1,
                vec![
                    arg(0),
                    call(0, vec![arg(0)]),
                    local(0),
                    tree_atom("0000"),
                    arg(0),
                    Expr::Unit,
                    Expr::Bool { value: false },
                    Expr::Bytes { hex: "0080".into() },
                    Expr::Atom { hex: "0000".into() },
                    int(128),
                ],
            ),
        ),
        vec![
            rightmost(0, 0),
            function(
                1,
                value_types.clone(),
                result,
                tuple((0..value_types.len()).map(arg).collect()),
            ),
        ],
    );
    let [(off, baseline), (on, optimized)] = modes(&source);
    assert!(!off && on);
    assert!(
        optimized.len() < baseline.len(),
        "forwarding fixture did not shrink: off={}, on={}",
        baseline.len(),
        optimized.len()
    );
    for depth in [0, 1, 32] {
        let leaf = atom(&[0, 0, 1]);
        let environment = right_spine(depth, &leaf);
        let expected = list(&[
            environment.clone(),
            leaf,
            atom(&[0xab, 0xcd]),
            atom(&[0, 0]),
            environment.clone(),
            atom(&[]),
            atom(&[]),
            atom(&[0, 128]),
            atom(&[0, 0]),
            uint(128),
        ]);
        let off_cost = accepted(run(&baseline, &environment, FINITE_LIMIT), &expected);
        let on_cost = accepted(run(&optimized, &environment, FINITE_LIMIT), &expected);
        assert!(
            on_cost < off_cost,
            "depth={depth}: off={off_cost}, on={on_cost}"
        );
        eprintln!(
            "forwarding depth={depth}: bytes off/on={}/{}, cost off/on={off_cost}/{on_cost}",
            baseline.len(),
            optimized.len()
        );
    }
}

#[test]
fn nested_tuple_and_byte_paths_use_fresh_callee_scopes() {
    let leaf_and_byte = tuple_type(vec![Type::Tree, u8_type()]);
    let arguments = tuple_type(vec![Type::Tree, Type::Bytes { len: 2 }]);
    let inner_result = tuple_type(vec![Type::Tree, u8_type(), Type::Tree]);
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        tuple_type(vec![inner_result.clone(), Type::Tree]),
        binding(
            0,
            Type::Tree,
            arg(0),
            binding(
                1,
                Type::Bytes { len: 2 },
                Expr::Bytes { hex: "0180".into() },
                tuple(vec![
                    call(2, vec![tuple(vec![local(0), local(1)]), tree_atom("ff")]),
                    local(0),
                ]),
            ),
        ),
        vec![
            rightmost(0, 0),
            function(
                1,
                vec![Type::Tree, Type::Bytes { len: 2 }],
                leaf_and_byte.clone(),
                binding(
                    0,
                    Type::Bytes { len: 2 },
                    arg(1),
                    binding(
                        1,
                        Type::Tree,
                        call(0, vec![arg(0)]),
                        tuple(vec![local(1), field(local(0), 1)]),
                    ),
                ),
            ),
            function(
                2,
                vec![arguments, Type::Tree],
                inner_result,
                binding(
                    0,
                    Type::Tree,
                    arg(1),
                    binding(
                        1,
                        leaf_and_byte,
                        call(1, vec![field(arg(0), 0), field(arg(0), 1)]),
                        tuple(vec![field(local(1), 0), field(local(1), 1), local(0)]),
                    ),
                ),
            ),
        ],
    );
    for (_, code) in modes(&source) {
        for depth in [0, 1, 17] {
            let leaf = atom(&[0, 1]);
            let environment = right_spine(depth, &leaf);
            let expected = list(&[list(&[leaf, uint(128), atom(&[0xff])]), environment.clone()]);
            accepted(run(&code, &environment, FINITE_LIMIT), &expected);
        }
    }
}

#[test]
fn discarded_nonleaf_values_retain_order_between_atomic_arguments() {
    let ty = u8_type();
    let ignored_arguments = function(1, vec![ty.clone(); 5], ty.clone(), int(7));
    for reverse in [false, true] {
        let (first, second, expected) = if reverse {
            (call(0, vec![]), reject(&ty), FailureCode::CostExceeded)
        } else {
            (reject(&ty), call(0, vec![]), FailureCode::Raise)
        };
        let operands = vec![int(1), first.clone(), int(2), second.clone(), int(3)];
        let bodies = [
            call(1, operands.clone()),
            field(tuple(operands.clone()), 0),
            binding(0, ty.clone(), call(1, operands), int(7)),
            add(first, second),
        ];
        for body in bodies {
            let source = program(
                Environment::Ignored,
                vec![],
                ty.clone(),
                body,
                vec![spin(&ty), ignored_arguments.clone()],
            );
            for (_, code) in modes(&source) {
                failed(run(&code, &pair(&uint(8), &uint(1)), LIMIT), expected);
            }
        }
    }
}

#[test]
fn direct_unary_operators_do_not_discard_strict_pair_siblings() {
    for reverse in [false, true] {
        let (first_value, rest_value, expected) = if reverse {
            (
                call(0, vec![]),
                reject(&Type::Tree),
                FailureCode::CostExceeded,
            )
        } else {
            (reject(&Type::Tree), call(0, vec![]), FailureCode::Raise)
        };
        let pair_value = Expr::TreePair {
            first: Box::new(first_value),
            rest: Box::new(rest_value),
        };
        let bodies = [
            (first(pair_value.clone()), Type::Tree),
            (rest(pair_value.clone()), Type::Tree),
            (is_pair(pair_value.clone()), Type::Bool),
            (atom_value(pair_value.clone()), Type::Atom),
            (
                Expr::AtomIsEmpty {
                    value: Box::new(atom_value(pair_value)),
                },
                Type::Bool,
            ),
        ];
        for (body, result) in bodies {
            let source = program(
                Environment::Ignored,
                vec![],
                result,
                body,
                vec![spin(&Type::Tree)],
            );
            for (_, code) in modes(&source) {
                failed(run(&code, &atom(&[]), LIMIT), expected);
            }
        }
    }
}

#[test]
fn checked_atom_projection_evaluates_a_nonleaf_call_once() {
    let direct = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Atom,
        atom_value(call(0, vec![arg(0)])),
        vec![rightmost(0, 0)],
    );
    let mut bound = direct.clone();
    bound.body = binding(0, Type::Tree, call(0, vec![arg(0)]), atom_value(local(0)));
    let mut duplicated = direct.clone();
    duplicated.body = sequence(call(0, vec![arg(0)]), direct.body.clone());
    let leaf = atom(&[0, 0, 1]);
    let environment = right_spine(128, &leaf);
    for (((mode, direct_code), (bound_mode, bound_code)), (duplicated_mode, duplicated_code)) in
        modes(&direct)
            .into_iter()
            .zip(modes(&bound))
            .zip(modes(&duplicated))
    {
        assert_eq!(mode, bound_mode);
        assert_eq!(mode, duplicated_mode);
        let direct_cost = accepted(run(&direct_code, &environment, FINITE_LIMIT), &leaf);
        let bound_cost = accepted(run(&bound_code, &environment, FINITE_LIMIT), &leaf);
        let twice_cost = accepted(run(&duplicated_code, &environment, FINITE_LIMIT), &leaf);
        assert!(
            twice_cost * 4 > bound_cost * 5,
            "insufficient once/twice cost separation"
        );
        let budget = bound_cost + (twice_cost - bound_cost) / 2;
        accepted(run(&direct_code, &environment, budget), &leaf);
        failed(
            run(&duplicated_code, &environment, budget),
            FailureCode::CostExceeded,
        );
        eprintln!(
            "atom projection optimize={mode}: direct={direct_cost}, bound={bound_cost}, twice={twice_cost}, budget={budget}"
        );
    }
}

#[test]
fn tail_identity_lets_preserve_initializer_failures_and_outer_lexical_scope() {
    let ty = u8_type();
    let identity = function(
        1,
        vec![ty.clone()],
        ty.clone(),
        binding(0, ty.clone(), arg(0), local(0)),
    );
    for (initializer, failure) in [
        (int(7), None),
        (call(1, vec![int(7)]), None),
        (reject(&ty), Some(FailureCode::Raise)),
        (call(0, vec![]), Some(FailureCode::CostExceeded)),
        (add(int(255), int(1)), Some(FailureCode::Raise)),
    ] {
        let source = program(
            Environment::Ignored,
            vec![],
            ty.clone(),
            binding(0, ty.clone(), initializer, local(0)),
            vec![spin(&ty), identity.clone()],
        );
        for (_, code) in modes(&source) {
            let outcome = run(&code, &atom(&[]), LIMIT);
            match failure {
                Some(expected) => failed(outcome, expected),
                None => {
                    accepted(outcome, &uint(7));
                }
            }
        }
    }

    let source = program(
        Environment::Strict,
        vec![ty.clone()],
        ty.clone(),
        binding(
            0,
            ty.clone(),
            arg(0),
            binding(
                1,
                ty.clone(),
                binding(2, ty.clone(), add(local(0), int(1)), local(2)),
                local(1),
            ),
        ),
        vec![],
    );
    for (_, code) in modes(&source) {
        accepted(run(&code, &list(&[uint(9)]), FINITE_LIMIT), &uint(10));
        failed(
            run(&code, &list(&[uint(255)]), FINITE_LIMIT),
            FailureCode::Raise,
        );
    }

    // The initializer is outside the outer binding's scope. Its independent
    // local0 may therefore reuse that ID without referring to an unbound value.
    let reused_initializer = program(
        Environment::Ignored,
        vec![],
        ty.clone(),
        binding(0, ty.clone(), binding(0, ty, int(7), local(0)), local(0)),
        vec![],
    );
    for (_, code) in modes(&reused_initializer) {
        accepted(run(&code, &atom(&[]), FINITE_LIMIT), &uint(7));
    }
}

#[test]
fn nested_guard_selectors_preserve_prompt_rejection_before_divergence() {
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        binding(
            0,
            Type::Bool,
            is_pair(arg(0)),
            branch(
                call(1, vec![local(0)]),
                sequence(call(0, vec![]), first(arg(0))),
                reject(&Type::Tree),
            ),
        ),
        vec![
            spin(&Type::Tree),
            function(
                1,
                vec![Type::Bool],
                Type::Bool,
                Expr::Not {
                    value: Box::new(Expr::Not {
                        value: Box::new(arg(0)),
                    }),
                },
            ),
        ],
    );
    for (_, code) in modes(&source) {
        for input in [atom(&[]), atom(&[0]), atom(&[0, 1])] {
            failed(run(&code, &input, LIMIT), FailureCode::Raise);
        }
        failed(
            run(&code, &pair(&atom(&[]), &atom(&[])), LIMIT),
            FailureCode::CostExceeded,
        );
    }
    // Deliberate unsound edit: it loses the atom branch's prompt rejection.
    let mut unguarded = source;
    unguarded.body = sequence(call(0, vec![]), first(arg(0)));
    for (_, code) in modes(&unguarded) {
        failed(run(&code, &atom(&[]), LIMIT), FailureCode::CostExceeded);
    }
}

#[test]
fn external_abi_checks_unused_nested_arguments_before_optimized_entry() {
    let source = program(
        Environment::Strict,
        vec![
            tuple_type(vec![Type::Bytes { len: 2 }, Type::Bool]),
            u8_type(),
        ],
        u8_type(),
        call(0, vec![]),
        vec![function(0, vec![], u8_type(), int(7))],
    );
    let nested = list(&[atom(&[1, 128]), uint(1)]);
    let valid = list(&[nested.clone(), uint(9)]);
    let malformed = [
        atom(&[]),
        list(std::slice::from_ref(&nested)),
        list(&[nested.clone(), uint(9), uint(1)]),
        pair(&nested, &uint(9)),
        list(&[pair(&atom(&[1, 128]), &uint(1)), uint(9)]),
        list(&[list(&[atom(&[1]), uint(1)]), uint(9)]),
        list(&[list(&[atom(&[1, 128]), atom(&[0])]), uint(9)]),
        list(&[list(&[atom(&[1, 128]), uint(2)]), uint(9)]),
        list(&[nested.clone(), atom(&[0, 9])]),
        list(&[nested, uint(256)]),
    ];
    for (_, code) in modes(&source) {
        accepted(run(&code, &valid, FINITE_LIMIT), &uint(7));
        for input in &malformed {
            abi_rejected(run(&code, input, LIMIT));
        }
    }
}
