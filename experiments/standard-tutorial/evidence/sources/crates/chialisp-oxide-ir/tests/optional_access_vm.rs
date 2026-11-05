//! Executed v8 option decoding/access controls. Finite VM observations only.
#[path = "runtime_evaluation/support.rs"]
#[allow(dead_code)]
mod support;
use chialisp_oxide_ir::{Environment, Expr, OptionalAccess, Program, Type};
use chialisp_oxide_vm::FailureCode;
use support::*;

fn optional(item: Type) -> Type {
    Type::Optional {
        item: Box::new(item),
    }
}
fn present(value: Expr) -> Expr {
    Expr::OptionalIsSome(OptionalAccess {
        value: Box::new(value),
    })
}
fn payload(value: Expr) -> Expr {
    Expr::OptionalValue(OptionalAccess {
        value: Box::new(value),
    })
}
fn some(value: Expr) -> Expr {
    Expr::OptionalSome {
        value: Box::new(value),
    }
}
fn none(item: Type) -> Expr {
    Expr::OptionalNone { item }
}
fn fixture(item: Type, result: Type, body: Expr) -> Program {
    program(
        Environment::Strict,
        vec![optional(item)],
        result,
        body,
        vec![],
    )
}

#[test]
fn optional_inputs_preserve_exact_tags_and_validate_ignored_payloads() {
    let source = fixture(u8_type(), Type::Bool, present(arg(0)));
    for (_, code) in modes(&source) {
        accepted(run(&code, &list(&[atom(&[])]), FINITE_LIMIT), &uint(0));
        for value in [0, 1, 127, 128, 255] {
            accepted(
                run(&code, &list(&[list(&[uint(value)])]), FINITE_LIMIT),
                &uint(1),
            );
        }
        for wrong in [
            atom(&[0]),
            atom(&[1]),
            atom(&[0, 0]),
            list(&[uint(1), uint(2)]),
            pair(&uint(1), &uint(2)),
            list(&[atom(&[0])]),
            list(&[atom(&[0, 1])]),
            list(&[uint(256)]),
            list(&[atom(&[0xff])]),
            list(&[pair(&uint(1), &uint(2))]),
        ] {
            abi_rejected(run(&code, &list(&[wrong]), FINITE_LIMIT));
        }
        for wrong_environment in [
            atom(&[]),
            uint(1),
            list(&[atom(&[]), uint(1)]),
            pair(&atom(&[]), &uint(1)),
        ] {
            abi_rejected(run(&code, &wrong_environment, FINITE_LIMIT));
        }
    }
    // Ignoring the optional parameter does not bypass any of its ABI guards.
    let ignored = fixture(u8_type(), u8_type(), int(7));
    for (_, code) in modes(&ignored) {
        accepted(run(&code, &list(&[atom(&[])]), FINITE_LIMIT), &uint(7));
        abi_rejected(run(&code, &list(&[list(&[atom(&[0, 1])])]), FINITE_LIMIT));
    }
}

#[test]
fn nested_optional_unit_tuple_pair_and_bytes_keep_distinct_shapes() {
    let shapes = [
        (Type::Unit, list(&[atom(&[])])),
        (optional(u8_type()), list(&[atom(&[])])),
        (optional(u8_type()), list(&[list(&[uint(0)])])),
        (
            Type::Tuple {
                items: vec![u8_type(), Type::Bool],
            },
            list(&[list(&[uint(7), uint(1)])]),
        ),
        (
            Type::Pair {
                first: Box::new(u8_type()),
                rest: Box::new(u8_type()),
            },
            list(&[pair(&uint(7), &uint(9))]),
        ),
        (Type::Bytes { len: 2 }, list(&[atom(&[0, 1])])),
        (Type::Tree, list(&[pair(&atom(&[0]), &atom(&[1]))])),
    ];
    for (item, value) in shapes {
        let source = fixture(item.clone(), optional(item), arg(0));
        for (_, code) in modes(&source) {
            accepted(
                run(&code, &list(std::slice::from_ref(&value)), FINITE_LIMIT),
                &value,
            );
            accepted(run(&code, &list(&[atom(&[])]), FINITE_LIMIT), &atom(&[]));
        }
    }
    let unit = fixture(Type::Unit, Type::Bool, present(arg(0)));
    for (_, code) in modes(&unit) {
        accepted(run(&code, &list(&[atom(&[])]), FINITE_LIMIT), &uint(0));
        accepted(
            run(&code, &list(&[list(&[atom(&[])])]), FINITE_LIMIT),
            &uint(1),
        );
        abi_rejected(run(&code, &list(&[list(&[atom(&[0])])]), FINITE_LIMIT));
    }
}

#[test]
fn match_projection_is_lazy_and_an_unused_none_projection_still_rejects() {
    let source = fixture(
        u8_type(),
        u8_type(),
        branch(present(arg(0)), payload(arg(0)), int(99)),
    );
    for (_, code) in modes(&source) {
        accepted(run(&code, &list(&[atom(&[])]), FINITE_LIMIT), &uint(99));
        accepted(
            run(&code, &list(&[list(&[uint(0)])]), FINITE_LIMIT),
            &uint(0),
        );
        accepted(
            run(&code, &list(&[list(&[uint(255)])]), FINITE_LIMIT),
            &uint(255),
        );
    }
    for body in [
        sequence(payload(none(u8_type())), int(7)),
        binding(0, u8_type(), payload(none(u8_type())), int(7)),
    ] {
        let source = program(Environment::Strict, vec![], u8_type(), body, vec![]);
        for (_, code) in modes(&source) {
            failed(run(&code, &list(&[]), FINITE_LIMIT), FailureCode::Raise);
        }
    }
    // A new node must not evaluate its operand zero times just because the
    // presence result or payload binding itself is unused.
    let strict = program(
        Environment::Strict,
        vec![],
        u8_type(),
        sequence(
            present(Expr::Reject {
                message: "strict optional operand".into(),
            }),
            int(7),
        ),
        vec![],
    );
    for (_, code) in modes(&strict) {
        failed(run(&code, &list(&[]), FINITE_LIMIT), FailureCode::Raise);
    }
}

#[test]
fn malformed_optional_guards_run_before_divergence_and_valid_inputs_do_not_fake_rejection() {
    let source = program(
        Environment::Strict,
        vec![optional(u8_type())],
        Type::Unit,
        call(0, vec![]),
        vec![spin(&Type::Unit)],
    );
    for (_, code) in modes(&source) {
        failed(
            run(&code, &list(&[atom(&[])]), 50_000),
            FailureCode::CostExceeded,
        );
        failed(
            run(&code, &list(&[list(&[uint(7)])]), 50_000),
            FailureCode::CostExceeded,
        );
        // Last payload check precedes the recursive source body.
        abi_rejected(run(&code, &list(&[list(&[atom(&[0, 7])])]), 50_000));
        abi_rejected(run(&code, &list(&[list(&[uint(7), uint(8)])]), 50_000));
    }
    // Source-order snapshot: the first optional value remains unchanged after
    // the later lexical binding shadows the conceptual source variable.
    let source = program(
        Environment::Strict,
        vec![],
        Type::Tuple {
            items: vec![u8_type(), Type::Bool],
        },
        binding(
            0,
            optional(u8_type()),
            some(int(7)),
            binding(
                1,
                optional(u8_type()),
                none(u8_type()),
                Expr::Tuple {
                    items: vec![payload(local(0)), present(local(1))],
                },
            ),
        ),
        vec![],
    );
    for (_, code) in modes(&source) {
        accepted(
            run(&code, &list(&[]), FINITE_LIMIT),
            &list(&[uint(7), uint(0)]),
        );
    }
}
