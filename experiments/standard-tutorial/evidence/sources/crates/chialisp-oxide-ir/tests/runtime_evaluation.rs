//! Executed runtime-IR regressions, not a termination or compiler-preservation
//! theorem. Divergent programs run only in CLVM under an explicit cost limit.
#[path = "runtime_evaluation/support.rs"]
mod support;
use chialisp_oxide_ir::{Environment, Expr, Type, optimize};
use chialisp_oxide_vm::FailureCode;
use support::*;

#[test]
fn lexical_frames_rebase_arguments_locals_and_callee_type_context() {
    let byte_helper = function(
        0,
        vec![Type::Bytes { len: 2 }],
        u8_type(),
        binding(0, u8_type(), field(arg(0), 1), local(0)),
    );
    let result_type = Type::Tuple {
        items: vec![u8_type(); 4],
    };
    let source = program(
        Environment::Strict,
        vec![u8_type()],
        result_type,
        binding(
            0,
            u8_type(),
            arg(0),
            binding(
                1,
                u8_type(),
                add(local(0), int(1)),
                Expr::Tuple {
                    items: vec![
                        local(0),
                        local(1),
                        arg(0),
                        call(0, vec![Expr::Bytes { hex: "0180".into() }]),
                    ],
                },
            ),
        ),
        vec![byte_helper],
    );
    let expected = list(&[uint(7), uint(8), uint(7), uint(128)]);
    for (_, code) in modes(&source) {
        accepted(run(&code, &list(&[uint(7)]), FINITE_LIMIT), &expected);
    }
}

#[test]
fn callees_are_static_and_raw_program_shaped_data_never_executes() {
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tuple {
            items: vec![u8_type(), u8_type()],
        },
        Expr::Tuple {
            items: vec![call(0, vec![arg(0)]), call(1, vec![arg(0)])],
        },
        vec![
            function(0, vec![Type::Tree], u8_type(), int(11)),
            function(1, vec![Type::Tree], u8_type(), int(22)),
        ],
    );
    // (x), an apply-shaped tree and an improper program-looking pair are data.
    let environments = [
        vec![0xff, 8, 0x80],
        list(&[uint(2), uint(1), atom(&[])]),
        pair(&uint(8), &uint(255)),
        atom(&[0, 1]),
    ];
    for (_, code) in modes(&source) {
        for environment in &environments {
            accepted(
                run(&code, environment, FINITE_LIMIT),
                &list(&[uint(11), uint(22)]),
            );
        }
    }
}

#[test]
fn runtime_preserves_strict_raw_and_ignored_external_abis() {
    let strict = program(
        Environment::Strict,
        vec![u8_type()],
        u8_type(),
        binding(0, u8_type(), arg(0), local(0)),
        vec![],
    );
    for (_, code) in modes(&strict) {
        for value in [0, 1, 127, 128, 255] {
            accepted(
                run(&code, &list(&[uint(value)]), FINITE_LIMIT),
                &uint(value),
            );
        }
        for malformed in [
            atom(&[]),
            uint(1),
            pair(&uint(1), &uint(2)),
            list(&[uint(1), uint(2)]),
            list(&[atom(&[0])]),
            list(&[atom(&[0xff])]),
            list(&[uint(256)]),
        ] {
            abi_rejected(run(&code, &malformed, LIMIT));
        }
    }
    let raw = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        binding(0, Type::Tree, arg(0), local(0)),
        vec![],
    );
    let ignored = program(
        Environment::Ignored,
        vec![],
        u8_type(),
        binding(0, u8_type(), int(7), local(0)),
        vec![],
    );
    let inputs = [
        atom(&[]),
        atom(&[0]),
        atom(&[0, 1]),
        pair(&atom(&[0xff]), &atom(&[0, 0])),
    ];
    for (_, code) in modes(&raw) {
        for input in &inputs {
            accepted(run(&code, input, FINITE_LIMIT), input);
        }
    }
    for (_, code) in modes(&ignored) {
        for input in &inputs {
            accepted(run(&code, input, FINITE_LIMIT), &uint(7));
        }
    }
}

#[test]
fn call_operands_evaluate_left_to_right_even_when_the_callee_ignores_them() {
    let ty = u8_type();
    let callee = function(1, vec![ty.clone(), ty.clone()], ty.clone(), int(7));
    for (arguments, expected) in [
        (vec![reject(&ty), call(0, vec![])], FailureCode::Raise),
        (
            vec![call(0, vec![]), reject(&ty)],
            FailureCode::CostExceeded,
        ),
    ] {
        let source = program(
            Environment::Ignored,
            vec![],
            ty.clone(),
            call(1, arguments),
            vec![spin(&ty), callee.clone()],
        );
        for (_, code) in modes(&source) {
            failed(run(&code, &atom(&[]), LIMIT), expected);
        }
    }
}

#[test]
fn tuples_fields_arithmetic_sequences_and_unused_lets_preserve_strict_order() {
    let ty = u8_type();
    let spin_call = call(0, vec![]);
    let rejection = reject(&ty);
    for reverse in [false, true] {
        let (first, second, expected) = if reverse {
            (
                spin_call.clone(),
                rejection.clone(),
                FailureCode::CostExceeded,
            )
        } else {
            (rejection.clone(), spin_call.clone(), FailureCode::Raise)
        };
        let tuple = Expr::Tuple {
            items: vec![first.clone(), second.clone()],
        };
        let bodies = [
            field(tuple.clone(), 0),
            field(tuple, 1),
            add(first.clone(), second.clone()),
            sequence(first.clone(), second.clone()),
            binding(0, ty.clone(), first, second),
        ];
        for body in bodies {
            let source = program(
                Environment::Ignored,
                vec![],
                ty.clone(),
                body,
                vec![spin(&ty)],
            );
            for (_, code) in modes(&source) {
                failed(run(&code, &atom(&[]), LIMIT), expected);
            }
        }
    }
    // A completely unused initializer still executes, even with a constant body.
    for (initializer, expected) in [
        (reject(&ty), FailureCode::Raise),
        (spin_call, FailureCode::CostExceeded),
    ] {
        let source = program(
            Environment::Ignored,
            vec![],
            ty.clone(),
            binding(0, ty.clone(), initializer, int(7)),
            vec![spin(&ty)],
        );
        for (_, code) in modes(&source) {
            failed(run(&code, &atom(&[]), LIMIT), expected);
        }
    }
}

#[test]
fn tree_pair_operands_are_strict_but_unselected_calls_are_lazy() {
    for reverse in [false, true] {
        let (first, rest, expected) = if reverse {
            (
                call(0, vec![]),
                reject(&Type::Tree),
                FailureCode::CostExceeded,
            )
        } else {
            (reject(&Type::Tree), call(0, vec![]), FailureCode::Raise)
        };
        let source = program(
            Environment::Ignored,
            vec![],
            Type::Tree,
            Expr::TreePair {
                first: Box::new(first),
                rest: Box::new(rest),
            },
            vec![spin(&Type::Tree)],
        );
        for (_, code) in modes(&source) {
            failed(run(&code, &atom(&[]), LIMIT), expected);
        }
    }
    for value in [false, true] {
        let body = if value {
            branch(Expr::Bool { value }, int(7), call(0, vec![]))
        } else {
            branch(Expr::Bool { value }, call(0, vec![]), int(7))
        };
        let source = program(
            Environment::Ignored,
            vec![],
            u8_type(),
            body,
            vec![spin(&u8_type())],
        );
        for (_, code) in modes(&source) {
            accepted(run(&code, &atom(&[]), LIMIT), &uint(7));
        }
    }
}

#[test]
fn guard_before_divergence_cannot_be_erased_using_success_only_pair_facts() {
    let kept = sequence(call(0, vec![]), first(arg(0)));
    let guarded = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        branch(is_pair(arg(0)), kept.clone(), reject(&Type::Tree)),
        vec![spin(&Type::Tree)],
    );
    assert_eq!(
        serde_json::to_value(optimize(&guarded).unwrap()).unwrap(),
        serde_json::to_value(&guarded).unwrap()
    );
    let mut unsafe_edit = guarded.clone();
    unsafe_edit.body = kept;
    let atom_input = atom(&[0, 1]);
    let pair_input = pair(&atom(&[]), &atom(&[]));
    for (_, code) in modes(&guarded) {
        failed(run(&code, &atom_input, LIMIT), FailureCode::Raise);
        failed(run(&code, &pair_input, LIMIT), FailureCode::CostExceeded);
    }
    // This explicit bad edit is not produced by the pass: it loses prompt atom
    // rejection. Resource exhaustion is not equivalent to program rejection.
    for (_, code) in modes(&unsafe_edit) {
        for input in [&atom_input, &pair_input] {
            failed(run(&code, input, LIMIT), FailureCode::CostExceeded);
        }
    }
}

#[test]
fn self_and_mutual_calls_preserve_padded_rightmost_leaves_at_bounded_depths() {
    for functions in [
        vec![rightmost(0, 0)],
        vec![rightmost(0, 1), rightmost(1, 0)],
    ] {
        let source = program(
            Environment::Raw,
            vec![Type::Tree],
            Type::Tree,
            call(0, vec![arg(0)]),
            functions,
        );
        for (_, code) in modes(&source) {
            for depth in [0, 1, 8, 128, 2048] {
                let leaf = atom(&[0, 0, 1]);
                accepted(run(&code, &right_spine(depth, &leaf), FINITE_LIMIT), &leaf);
            }
        }
    }
}

#[test]
fn non_tail_calls_keep_continuations_and_checked_u8_overflow() {
    let count = function(
        0,
        vec![Type::Tree],
        u8_type(),
        branch(
            is_pair(arg(0)),
            add(call(0, vec![rest(arg(0))]), int(1)),
            int(0),
        ),
    );
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        u8_type(),
        call(0, vec![arg(0)]),
        vec![count],
    );
    for (_, code) in modes(&source) {
        for depth in [0, 1, 8, 32, 128, 255] {
            accepted(
                run(&code, &right_spine(depth, &atom(&[0, 1])), FINITE_LIMIT),
                &uint(depth as u16),
            );
        }
        failed(
            run(&code, &right_spine(256, &atom(&[])), FINITE_LIMIT),
            FailureCode::Raise,
        );
    }
}

#[test]
fn let_bound_non_tail_result_executes_once_before_two_reads() {
    let result_type = Type::Tuple {
        items: vec![Type::Tree, Type::Tree],
    };
    let bound = program(
        Environment::Raw,
        vec![Type::Tree],
        result_type,
        binding(
            0,
            Type::Tree,
            call(0, vec![arg(0)]),
            Expr::Tuple {
                items: vec![local(0), local(0)],
            },
        ),
        vec![rightmost(0, 0)],
    );
    let mut duplicated = bound.clone();
    duplicated.body = Expr::Tuple {
        items: vec![call(0, vec![arg(0)]), call(0, vec![arg(0)])],
    };
    let leaf = atom(&[0, 1]);
    let environment = right_spine(128, &leaf);
    let expected = list(&[leaf.clone(), leaf]);
    let bound_modes = modes(&bound);
    let duplicated_modes = modes(&duplicated);
    for ((mode, bound_code), (other_mode, duplicated_code)) in
        bound_modes.into_iter().zip(duplicated_modes)
    {
        assert_eq!(mode, other_mode);
        let once_cost = accepted(run(&bound_code, &environment, FINITE_LIMIT), &expected);
        let twice_cost = accepted(run(&duplicated_code, &environment, FINITE_LIMIT), &expected);
        assert!(
            twice_cost * 4 > once_cost * 5,
            "optimize={mode}: insufficient measured separation: once={once_cost}, twice={twice_cost}"
        );
        let budget = once_cost + (twice_cost - once_cost) / 2;
        accepted(run(&bound_code, &environment, budget), &expected);
        failed(
            run(&duplicated_code, &environment, budget),
            FailureCode::CostExceeded,
        );
        eprintln!(
            "optimize={mode}: once={once_cost}, twice={twice_cost}, discriminating budget={budget}"
        );
    }
}
