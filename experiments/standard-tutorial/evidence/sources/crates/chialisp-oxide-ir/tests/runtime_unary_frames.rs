//! Internal unary frame compaction keeps public ABIs and strict evaluation.
#[path = "runtime_evaluation/support.rs"]
#[allow(dead_code)]
mod support;

use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Type};
use chialisp_oxide_vm::FailureCode;
use support::*;

fn binary(kind: BinaryOp, left: Expr, right: Expr, operand_type: Type) -> Expr {
    Expr::Binary {
        kind,
        left: Box::new(left),
        right: Box::new(right),
        ty: operand_type,
    }
}

#[test]
fn mutually_recursive_unary_and_binary_calls_preserve_arbitrary_tree_tails() {
    let functions = vec![
        function(
            0,
            vec![Type::Tree],
            Type::Tree,
            branch(
                is_pair(arg(0)),
                call(1, vec![first(arg(0)), rest(arg(0))]),
                arg(0),
            ),
        ),
        function(
            1,
            vec![Type::Tree, Type::Tree],
            Type::Tree,
            sequence(call(0, vec![arg(0)]), call(0, vec![arg(1)])),
        ),
    ];
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        call(0, vec![arg(0)]),
        functions,
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
                accepted(run(&code, &input, FINITE_LIMIT), &leaf);
            }
        }
    }
}

#[test]
fn original_unary_tail_and_projection_alias_survive_strict_frame_extensions() {
    let result = Type::Tuple {
        items: vec![Type::Tree; 3],
    };
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        result,
        branch(
            is_pair(arg(0)),
            binding(
                0,
                Type::Tree,
                first(arg(0)),
                binding(
                    1,
                    Type::Tree,
                    call(0, vec![rest(arg(0))]),
                    Expr::Tuple {
                        items: vec![arg(0), local(0), local(1)],
                    },
                ),
            ),
            sequence(
                reject(&Type::Tree),
                Expr::Tuple {
                    items: vec![arg(0); 3],
                },
            ),
        ),
        vec![rightmost(0, 0)],
    );
    for (_, code) in modes(&source) {
        for head in [atom(&[]), atom(&[0, 0]), pair(&atom(&[255]), &atom(&[0]))] {
            let leaf = atom(&[0, 128]);
            let input = pair(&head, &right_spine(12, &leaf));
            let expected = list(&[input.clone(), head, leaf]);
            accepted(run(&code, &input, FINITE_LIMIT), &expected);
        }
        failed(run(&code, &atom(&[]), LIMIT), FailureCode::Raise);
    }
}

#[test]
fn strict_integer_abi_and_non_tail_recursion_keep_all_u8_values() {
    let source = program(
        Environment::Strict,
        vec![u8_type()],
        u8_type(),
        call(0, vec![arg(0)]),
        vec![function(
            0,
            vec![u8_type()],
            u8_type(),
            branch(
                binary(BinaryOp::Eq, arg(0), int(0), u8_type()),
                int(0),
                add(
                    int(1),
                    call(0, vec![binary(BinaryOp::Sub, arg(0), int(1), u8_type())]),
                ),
            ),
        )],
    );
    for (_, code) in modes(&source) {
        for value in 0..=255 {
            accepted(
                run(&code, &list(&[uint(value)]), FINITE_LIMIT),
                &uint(value),
            );
        }
        for input in [
            atom(&[]),
            uint(1),
            list(&[]),
            list(&[uint(1), uint(2)]),
            list(&[atom(&[0])]),
            list(&[atom(&[0, 1])]),
            list(&[atom(&[255])]),
            list(&[uint(256)]),
            pair(&uint(1), &uint(2)),
        ] {
            abi_rejected(run(&code, &input, FINITE_LIMIT));
        }
    }
}

#[test]
fn unary_divergence_and_binary_argument_order_keep_the_first_observation() {
    let functions = vec![
        function(0, vec![Type::Tree], Type::Tree, call(0, vec![arg(0)])),
        function(1, vec![Type::Tree, Type::Tree], Type::Tree, arg(0)),
    ];
    for (body, expected) in [
        (
            sequence(reject(&Type::Tree), call(0, vec![arg(0)])),
            FailureCode::Raise,
        ),
        (
            sequence(call(0, vec![arg(0)]), reject(&Type::Tree)),
            FailureCode::CostExceeded,
        ),
        (
            call(1, vec![reject(&Type::Tree), call(0, vec![arg(0)])]),
            FailureCode::Raise,
        ),
        (
            call(1, vec![call(0, vec![arg(0)]), reject(&Type::Tree)]),
            FailureCode::CostExceeded,
        ),
    ] {
        let source = program(
            Environment::Raw,
            vec![Type::Tree],
            Type::Tree,
            body,
            functions.clone(),
        );
        for (_, code) in modes(&source) {
            // CostExceeded observes retained nontermination under this finite
            // ceiling. It is never counted as a policy rejection.
            failed(run(&code, &atom(&[0, 0]), LIMIT), expected);
        }
    }
}

#[test]
fn nullary_and_unary_edges_preserve_fresh_callees_and_ignored_entry_inputs() {
    let source = program(
        Environment::Ignored,
        vec![],
        Type::Tree,
        call(
            0,
            vec![Expr::TreePair {
                first: Box::new(tree_atom("ff")),
                rest: Box::new(tree_atom("0080")),
            }],
        ),
        vec![
            function(
                0,
                vec![Type::Tree],
                Type::Tree,
                branch(is_pair(arg(0)), call(1, vec![]), arg(0)),
            ),
            function(1, vec![], Type::Tree, call(0, vec![tree_atom("00")])),
        ],
    );
    for (_, code) in modes(&source) {
        for input in [
            atom(&[]),
            atom(&[255]),
            pair(&atom(&[0, 0]), &atom(&[0, 128])),
        ] {
            accepted(run(&code, &input, FINITE_LIMIT), &atom(&[0]));
        }
    }
}
