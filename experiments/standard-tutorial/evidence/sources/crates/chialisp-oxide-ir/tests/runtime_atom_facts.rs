//! Branch-local atom facts preserve strict operands and physical identities.
#[path = "runtime_evaluation/support.rs"]
#[allow(dead_code)]
mod support;

use chialisp_oxide_ir::{Environment, Expr, Type};
use chialisp_oxide_vm::FailureCode;
use support::*;

fn bytes(value: Expr) -> Expr {
    Expr::TreeAtomValue {
        value: Box::new(value),
    }
}
fn not(value: Expr) -> Expr {
    Expr::Not {
        value: Box::new(value),
    }
}
fn pair_value(first: Expr, rest: Expr) -> Expr {
    Expr::TreePair {
        first: Box::new(first),
        rest: Box::new(rest),
    }
}
fn raw(body: Expr, mut functions: Vec<chialisp_oxide_ir::Function>) -> chialisp_oxide_ir::Program {
    // Keep the strict runtime convention explicit, including fixtures whose
    // entry otherwise has no calls or bindings after optional simplification.
    if functions.is_empty() {
        functions.push(rightmost(0, 0));
    }
    program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Atom,
        body,
        functions,
    )
}
fn failure() -> Expr {
    Expr::Reject {
        message: "pair rejected".into(),
    }
}

#[test]
fn both_branch_polarities_preserve_every_atom_byte() {
    for condition in [
        is_pair(arg(0)),
        not(is_pair(arg(0))),
        not(not(is_pair(arg(0)))),
    ] {
        let atom_on_true = matches!(&condition, Expr::Not { value } if !matches!(value.as_ref(), Expr::Not { .. }));
        let source = raw(
            branch(
                condition,
                if atom_on_true {
                    bytes(arg(0))
                } else {
                    failure()
                },
                if atom_on_true {
                    failure()
                } else {
                    bytes(arg(0))
                },
            ),
            vec![],
        );
        for (_, code) in modes(&source) {
            for input in [
                atom(&[]),
                atom(&[0]),
                atom(&[0, 0]),
                atom(&[0, 128]),
                atom(&[255]),
            ] {
                accepted(run(&code, &input, LIMIT), &input);
            }
            failed(
                run(&code, &pair(&atom(&[]), &atom(&[])), LIMIT),
                FailureCode::Raise,
            );
        }
    }
}

#[test]
fn a_fact_never_applies_to_another_argument_or_after_a_join() {
    let separate = program(
        Environment::Strict,
        vec![Type::Tree, Type::Tree],
        Type::Atom,
        branch(is_pair(arg(0)), failure(), bytes(arg(1))),
        vec![rightmost(0, 0)],
    );
    let joined = raw(
        sequence(
            branch(
                is_pair(arg(0)),
                Expr::Bool { value: true },
                Expr::Bool { value: false },
            ),
            bytes(arg(0)),
        ),
        vec![],
    );
    for (_, code) in modes(&separate) {
        failed(
            run(
                &code,
                &list(&[atom(&[]), pair(&atom(&[]), &atom(&[]))]),
                LIMIT,
            ),
            FailureCode::Raise,
        );
    }
    for (_, code) in modes(&joined) {
        failed(
            run(&code, &pair(&atom(&[]), &atom(&[])), LIMIT),
            FailureCode::Raise,
        );
    }
}

#[test]
fn recursive_callees_start_with_fresh_shape_facts() {
    let recursive = function(
        0,
        vec![Type::Tree],
        Type::Atom,
        branch(
            is_pair(arg(0)),
            bytes(arg(0)),
            call(0, vec![pair_value(arg(0), tree_atom(""))]),
        ),
    );
    let source = raw(
        branch(
            is_pair(arg(0)),
            failure(),
            call(0, vec![pair_value(arg(0), tree_atom(""))]),
        ),
        vec![recursive],
    );
    for (_, code) in modes(&source) {
        failed(run(&code, &atom(&[0, 0]), LIMIT), FailureCode::Raise);
    }
}

#[test]
fn effectful_atom_operands_and_unused_initializers_retain_failure_order() {
    for (first, expected) in [
        (
            sequence(reject(&Type::Tree), call(0, vec![])),
            FailureCode::Raise,
        ),
        (
            sequence(call(0, vec![]), reject(&Type::Tree)),
            FailureCode::CostExceeded,
        ),
    ] {
        let source = raw(
            branch(is_pair(arg(0)), failure(), bytes(first)),
            vec![spin(&Type::Tree)],
        );
        for (_, code) in modes(&source) {
            failed(run(&code, &atom(&[]), LIMIT), expected);
        }
    }
    for initializer in [
        add(int(255), int(1)),
        Expr::Sha256 {
            parts: vec![bytes(pair_value(tree_atom(""), tree_atom("")))],
        },
    ] {
        let ty = if matches!(initializer, Expr::Sha256 { .. }) {
            Type::Bytes { len: 32 }
        } else {
            u8_type()
        };
        let source = raw(
            branch(
                is_pair(arg(0)),
                failure(),
                binding(0, ty, initializer, bytes(arg(0))),
            ),
            vec![],
        );
        for (_, code) in modes(&source) {
            failed(run(&code, &atom(&[]), LIMIT), FailureCode::Raise);
        }
    }
}

#[test]
fn guarded_atom_projection_rebases_after_a_strict_call_frame() {
    let source = raw(
        branch(
            is_pair(arg(0)),
            binding(
                0,
                Type::Tree,
                first(arg(0)),
                branch(
                    is_pair(local(0)),
                    failure(),
                    sequence(call(0, vec![arg(0)]), bytes(local(0))),
                ),
            ),
            failure(),
        ),
        vec![rightmost(0, 0)],
    );
    for (_, code) in modes(&source) {
        for value in [atom(&[]), atom(&[0]), atom(&[0, 0, 128])] {
            accepted(
                run(
                    &code,
                    &pair(&value, &pair(&atom(&[255]), &atom(&[]))),
                    LIMIT,
                ),
                &value,
            );
        }
        failed(
            run(
                &code,
                &pair(&pair(&atom(&[]), &atom(&[])), &atom(&[])),
                LIMIT,
            ),
            FailureCode::Raise,
        );
    }
}

#[test]
fn sibling_local_generations_do_not_share_atom_facts() {
    let source = raw(
        sequence(
            binding(
                0,
                Type::Tree,
                arg(0),
                branch(is_pair(local(0)), failure(), bytes(local(0))),
            ),
            binding(
                0,
                Type::Tree,
                pair_value(arg(0), tree_atom("")),
                bytes(local(0)),
            ),
        ),
        vec![],
    );
    for (_, code) in modes(&source) {
        failed(run(&code, &atom(&[]), LIMIT), FailureCode::Raise);
    }
}
