//! Finite output-ABI and strict-evaluation regressions, not preservation proofs.
#[path = "runtime_evaluation/support.rs"]
#[allow(dead_code)]
mod support;

use chialisp_oxide_ir::{Environment, Expr, Program, Type, compile, optimize};
use chialisp_oxide_vm::FailureCode;
use support::*;

fn optional(item: Type) -> Type {
    Type::Optional {
        item: Box::new(item),
    }
}
fn none(item: Type) -> Expr {
    Expr::OptionalNone { item }
}
fn some(value: Expr) -> Expr {
    Expr::OptionalSome {
        value: Box::new(value),
    }
}
fn fixture(item: Type, body: Expr) -> Program {
    program(Environment::Strict, vec![], optional(item), body, vec![])
}

#[test]
fn none_and_some_unit_have_distinct_exact_bytes() {
    let examples = [
        (fixture(Type::Unit, none(Type::Unit)), list(&[])),
        (fixture(Type::Unit, some(Expr::Unit)), list(&[list(&[])])),
        (fixture(u8_type(), some(int(0))), list(&[uint(0)])),
        (
            fixture(Type::Atom, some(Expr::Atom { hex: "0000".into() })),
            list(&[atom(&[0, 0])]),
        ),
    ];
    assert_ne!(examples[0].1, examples[1].1);
    for (source, wanted) in examples {
        assert!(source.uses_optional() && source.uses_runtime());
        assert!(!source.uses_pairs());
        for (_, code) in modes(&source) {
            accepted(run(&code, &list(&[]), FINITE_LIMIT), &wanted);
            abi_rejected(run(&code, &list(&[uint(1)]), FINITE_LIMIT));
        }
    }
}

#[test]
fn optional_condition_tail_and_nested_optional_preserve_their_declared_types() {
    let item = Type::Tuple {
        items: vec![u8_type(), u8_type()],
    };
    for (tail, expected) in [
        (none(item.clone()), list(&[uint(73)])),
        (
            some(Expr::Tuple {
                items: vec![int(50), int(7)],
            }),
            list(&[uint(73), list(&[uint(50), uint(7)])]),
        ),
    ] {
        let source = program(
            Environment::Strict,
            vec![],
            Type::Pair {
                first: Box::new(u8_type()),
                rest: Box::new(optional(item.clone())),
            },
            Expr::Pair {
                first: Box::new(int(73)),
                rest: Box::new(tail),
            },
            vec![],
        );
        assert!(source.uses_optional() && source.uses_pairs());
        let wire = serde_json::to_value(&source).unwrap();
        assert_eq!(wire["ir_version"], 6);
        assert!(
            serde_json::from_value::<Program>(wire)
                .unwrap()
                .uses_pairs()
        );
        for (_, code) in modes(&source) {
            accepted(run(&code, &list(&[]), FINITE_LIMIT), &expected);
        }
    }
    let source = fixture(optional(Type::Unit), some(none(Type::Unit)));
    assert_eq!(
        source.type_of(&source.body).unwrap(),
        Some(optional(optional(Type::Unit)))
    );
    for (_, code) in modes(&source) {
        accepted(run(&code, &list(&[]), FINITE_LIMIT), &list(&[list(&[])]));
    }
}

#[test]
fn ordinary_internal_helpers_can_pass_constructed_optional_values() {
    let source = program(
        Environment::Strict,
        vec![Type::Bool, u8_type()],
        optional(u8_type()),
        call(0, vec![branch(arg(0), some(arg(1)), none(u8_type()))]),
        vec![function(
            0,
            vec![optional(u8_type())],
            optional(u8_type()),
            arg(0),
        )],
    );
    for (_, code) in modes(&source) {
        accepted(
            run(&code, &list(&[uint(1), uint(7)]), FINITE_LIMIT),
            &list(&[uint(7)]),
        );
        accepted(
            run(&code, &list(&[uint(0), uint(7)]), FINITE_LIMIT),
            &list(&[]),
        );
        abi_rejected(run(&code, &list(&[uint(2), uint(7)]), FINITE_LIMIT));
        abi_rejected(run(&code, &list(&[uint(1), atom(&[0, 7])]), FINITE_LIMIT));
    }
}

#[test]
fn optional_payloads_keep_failure_order_and_lazy_branch_selection() {
    let body = some(Expr::Tuple {
        items: vec![reject(&u8_type()), first(arg(0))],
    });
    let source = program(
        Environment::Strict,
        vec![Type::Tree],
        optional(Type::Tuple {
            items: vec![u8_type(), Type::Tree],
        }),
        body,
        vec![],
    );
    for (_, code) in modes(&source) {
        failed(
            run(&code, &list(&[list(&[])]), FINITE_LIMIT),
            FailureCode::Raise,
        );
    }
    let source = program(
        Environment::Strict,
        vec![Type::Bool],
        optional(u8_type()),
        branch(arg(0), some(add(int(255), int(1))), none(u8_type())),
        vec![],
    );
    for (_, code) in modes(&source) {
        accepted(run(&code, &list(&[uint(0)]), FINITE_LIMIT), &list(&[]));
        failed(
            run(&code, &list(&[uint(1)]), FINITE_LIMIT),
            FailureCode::Raise,
        );
    }
}

#[test]
fn discarded_optional_payloads_still_reject_or_exhaust_the_budget() {
    let sources = [
        program(
            Environment::Strict,
            vec![],
            u8_type(),
            sequence(some(reject(&u8_type())), int(7)),
            vec![],
        ),
        program(
            Environment::Strict,
            vec![],
            u8_type(),
            binding(0, optional(u8_type()), some(call(0, vec![])), int(7)),
            vec![spin(&u8_type())],
        ),
    ];
    for (index, source) in sources.iter().enumerate() {
        let before = serde_json::to_value(source).unwrap();
        assert_eq!(
            serde_json::to_value(optimize(source).unwrap()).unwrap(),
            before
        );
        for (_, code) in modes(source) {
            failed(
                run(&code, &list(&[]), LIMIT),
                if index == 0 {
                    FailureCode::Raise
                } else {
                    FailureCode::CostExceeded
                },
            );
        }
    }
}

#[test]
fn every_export_parameter_container_rejects_optional_values() {
    for ty in [
        optional(u8_type()),
        Type::Tuple {
            items: vec![u8_type(), optional(u8_type())],
        },
        Type::Pair {
            first: Box::new(u8_type()),
            rest: Box::new(optional(u8_type())),
        },
    ] {
        let source = program(Environment::Strict, vec![ty], u8_type(), int(7), vec![]);
        assert!(
            source
                .validate()
                .unwrap_err()
                .to_string()
                .contains("output-only")
        );
        for mode in [false, true] {
            assert!(
                compile(&source, mode)
                    .unwrap_err()
                    .to_string()
                    .contains("output-only")
            );
        }
        assert!(optimize(&source).is_err());
        assert!(serde_json::to_value(&source).is_err());
    }
}

#[test]
fn version_six_is_exact_and_cannot_be_downgraded_to_older_envelopes() {
    let source = fixture(Type::Unit, some(Expr::Unit));
    let wire = serde_json::to_value(&source).unwrap();
    assert_eq!(wire["ir_version"], 6);
    let reread: Program = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(&reread).unwrap(), wire);
    let mut input = wire.clone();
    input["program"]["parameters"] =
        serde_json::json!([{"name":"untrusted", "ty":{"kind":"optional", "item":{"kind":"unit"}}}]);
    assert!(serde_json::from_value::<Program>(input).is_err());
    let mut mismatched = wire.clone();
    mismatched["program"]["result"]["item"] = serde_json::json!({"kind":"bool"});
    assert!(serde_json::from_value::<Program>(mismatched).is_err());
    for version in [2, 3, 4, 5, 7] {
        let mut bad = wire.clone();
        bad["ir_version"] = version.into();
        assert!(
            serde_json::from_value::<Program>(bad).is_err(),
            "version {version}"
        );
    }
    let mut legacy = wire["program"].clone();
    legacy.as_object_mut().unwrap().remove("functions");
    assert!(serde_json::from_value::<Program>(legacy).is_err());
    let mut no_capability = serde_json::to_value(program(
        Environment::Strict,
        vec![],
        u8_type(),
        binding(0, u8_type(), int(1), local(0)),
        vec![],
    ))
    .unwrap();
    assert_eq!(no_capability["ir_version"], 4);
    no_capability["ir_version"] = 6.into();
    assert!(serde_json::from_value::<Program>(no_capability).is_err());
}

#[test]
fn optional_capability_checks_unused_signatures_and_constructor_metadata() {
    let mut source = program(
        Environment::Strict,
        vec![],
        Type::Bool,
        Expr::Bool { value: true },
        vec![function(
            0,
            vec![optional(u8_type())],
            Type::Unit,
            Expr::Unit,
        )],
    );
    assert!(source.uses_optional());
    assert_eq!(serde_json::to_value(&source).unwrap()["ir_version"], 6);
    source.functions[0].result = optional(u8_type());
    source.functions[0].body = none(Type::Int {
        bits: 7,
        signed: false,
    });
    assert!(source.validate().is_err());
    assert!(optimize(&source).is_err());
    let mismatched = fixture(u8_type(), none(Type::Bool));
    assert!(mismatched.validate().is_err());
    let mismatched = fixture(
        u8_type(),
        branch(Expr::Bool { value: true }, some(int(1)), none(Type::Bool)),
    );
    assert!(mismatched.validate().is_err());
}

#[test]
fn none_item_metadata_is_bounded_even_when_the_constructor_is_unused() {
    let mut item = u8_type();
    for _ in 0..32 {
        item = optional(item);
    }
    let source = program(
        Environment::Strict,
        vec![],
        Type::Unit,
        sequence(none(item), Expr::Unit),
        vec![],
    );
    assert!(source.validate().is_err());
    assert!(optimize(&source).is_err());
    let mut value = Expr::Unit;
    for _ in 0..40 {
        value = some(value);
    }
    let source = program(
        Environment::Strict,
        vec![],
        Type::Unit,
        sequence(value, Expr::Unit),
        vec![],
    );
    assert!(
        source
            .validate()
            .unwrap_err()
            .to_string()
            .contains("type nesting limit")
    );
    assert!(optimize(&source).is_err());
}
