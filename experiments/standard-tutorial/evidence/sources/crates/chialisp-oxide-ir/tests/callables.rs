//! Constructed-IR execution and closed wire/type controls, not source preservation.
#[path = "runtime_evaluation/support.rs"]
#[allow(dead_code)]
mod support;
use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Program, Type, compile, optimize};
use chialisp_oxide_vm::FailureCode;
use support::*;

fn callable() -> Type {
    Type::Callable {
        parameters: vec![u8_type()],
        result: Box::new(u8_type()),
    }
}
fn pointer(id: usize) -> Expr {
    Expr::FunctionRef { function_id: id }
}
fn indirect(callee: Expr, arguments: Vec<Expr>) -> Expr {
    Expr::CallIndirect {
        callee: Box::new(callee),
        arguments,
    }
}
fn fixture() -> Program {
    let callback = callable();
    let functions = vec![
        function(0, vec![u8_type()], u8_type(), add(arg(0), int(1))),
        function(
            1,
            vec![u8_type()],
            u8_type(),
            Expr::Binary {
                kind: BinaryOp::Mul,
                left: Box::new(arg(0)),
                right: Box::new(int(2)),
                ty: u8_type(),
            },
        ),
        function(
            2,
            vec![callback, u8_type()],
            u8_type(),
            indirect(arg(0), vec![arg(1)]),
        ),
    ];
    program(
        Environment::Strict,
        vec![Type::Bool, u8_type()],
        u8_type(),
        call(2, vec![branch(arg(0), pointer(0), pointer(1)), arg(1)]),
        functions,
    )
}
fn tree_pair(first: Expr, rest: Expr) -> Expr {
    Expr::TreePair {
        first: Box::new(first),
        rest: Box::new(rest),
    }
}
fn assembled(text: &str) -> Vec<u8> {
    let mut a = clvmr::Allocator::new();
    let node = rue_clvm::assemble(&mut a, text).unwrap();
    clvmr::serde::node_to_bytes(&a, node).unwrap()
}

#[test]
fn closed_callbacks_dispatch_exactly_to_target_zero_and_one() {
    let source = fixture();
    source.validate().unwrap();
    assert_eq!(source.callable_targets(&callable()).unwrap(), vec![0, 1]);
    let reference =
        chialisp_oxide_reference::compile("(mod (C X) (if C (+ X 1) (* X 2)))").unwrap();
    for optimized in [false, true] {
        let bytes = compile(&source, optimized).unwrap();
        for (input, expected) in [
            ("(() 7)", "14"),
            ("(1 7)", "8"),
            ("(() 0)", "0"),
            ("(1 254)", "255"),
        ] {
            let input = assembled(input);
            let expected = assembled(expected);
            accepted(run(&reference, &input, LIMIT), &expected);
            accepted(run(&bytes, &input, LIMIT), &expected);
        }
        for input in ["(0x00 7)", "(1 0x0007)", "(1 256)", "(1 7 8)", "(1 . 7)"] {
            abi_rejected(run(&bytes, &assembled(input), LIMIT));
        }
        abi_rejected(run(&bytes, &assembled("(1 255)"), LIMIT));
    }
}

fn fixed_failure(opcode: u8) -> Expr {
    let opcode = tree_atom(&format!("{opcode:02x}"));
    let path = tree_atom("01");
    let program = tree_pair(opcode, tree_pair(path, tree_atom("")));
    Expr::Apply {
        program: Box::new(program),
        environment: Box::new(tree_atom("")),
    }
}
fn strict(callee_failure: u8, argument_failure: u8) -> Program {
    program(
        Environment::Ignored,
        vec![],
        u8_type(),
        indirect(
            sequence(fixed_failure(callee_failure), pointer(0)),
            vec![sequence(fixed_failure(argument_failure), int(1))],
        ),
        vec![function(0, vec![u8_type()], u8_type(), int(7))],
    )
}
#[test]
fn callee_is_captured_before_arguments_even_when_target_ignores_them() {
    for optimized in [false, true] {
        for (first, second, expected) in [
            (8, 5, FailureCode::Raise),
            (5, 8, FailureCode::InvalidOpArg),
        ] {
            let p = strict(first, second);
            p.validate().unwrap();
            failed(
                run(&compile(&p, optimized).unwrap(), &assembled("()"), LIMIT),
                expected,
            );
        }
        let mut p = strict(8, 5);
        if let Expr::CallIndirect { callee, .. } = &mut p.body {
            **callee = pointer(0);
        }
        failed(
            run(&compile(&p, optimized).unwrap(), &assembled("()"), LIMIT),
            FailureCode::InvalidOpArg,
        );
    }
}

#[test]
fn wire_version_nine_is_exact_and_older_envelopes_cannot_admit_callables() {
    let p = fixture();
    let original = serde_json::to_value(&p).unwrap();
    assert_eq!(original["ir_version"], 9);
    let decoded: Program = serde_json::from_value(original.clone()).unwrap();
    assert!(decoded.uses_callables());
    for version in 2..9 {
        let mut changed = original.clone();
        changed["ir_version"] = version.into();
        assert!(
            serde_json::from_value::<Program>(changed).is_err(),
            "old version {version} accepted callables"
        );
    }
    let old = program(Environment::Strict, vec![], u8_type(), int(7), vec![]);
    let mut wire = serde_json::json!({"ir_version":9,"environment":"strict","program":{"name":"empty9","parameters":[],"result":u8_type(),"body":int(7),"functions":[]}});
    assert!(serde_json::from_value::<Program>(wire.take()).is_err());
    let before = serde_json::to_vec(&old).unwrap();
    let old_decoded: Program = serde_json::from_slice(&before).unwrap();
    assert_eq!(serde_json::to_vec(&old_decoded).unwrap(), before);
}

fn invalid(mut p: Program, modify: impl FnOnce(&mut Program)) {
    modify(&mut p);
    assert!(p.validate().is_err());
    for optimized in [false, true] {
        assert!(compile(&p, optimized).is_err());
    }
}
#[test]
fn whole_ir_validation_rejects_corrupt_and_stored_or_public_callable_values() {
    let source = fixture();
    invalid(source.clone(), |p| p.parameters[0].ty = callable());
    invalid(source.clone(), |p| p.result = callable());
    invalid(source.clone(), |p| p.functions[0].result = callable());
    invalid(source.clone(), |p| p.body = pointer(99));
    invalid(source.clone(), |p| p.body = sequence(pointer(2), int(7)));
    invalid(source.clone(), |p| p.body = indirect(int(0), vec![int(1)]));
    invalid(source.clone(), |p| p.body = indirect(pointer(0), vec![]));
    invalid(source.clone(), |p| {
        p.body = indirect(pointer(0), vec![Expr::Bool { value: true }])
    });
    invalid(source.clone(), |p| {
        p.body = sequence(
            Expr::Tuple {
                items: vec![pointer(0)],
            },
            int(7),
        )
    });
    invalid(source.clone(), |p| {
        p.body = sequence(
            Expr::Pair {
                first: Box::new(pointer(0)),
                rest: Box::new(int(1)),
            },
            int(7),
        )
    });
    invalid(source.clone(), |p| {
        p.body = sequence(
            Expr::OptionalSome {
                value: Box::new(pointer(0)),
            },
            int(7),
        )
    });
    invalid(source.clone(), |p| {
        p.functions[1].body = Expr::Arg { index: 99 }
    });
    invalid(source.clone(), |p| p.functions[1].id = 0);
    invalid(source.clone(), |p| {
        p.body = indirect(Expr::Local { local_id: 99 }, vec![int(1)])
    });
    invalid(source.clone(), |p| {
        p.functions.push(function(
            3,
            vec![Type::Tuple {
                items: vec![callable()],
            }],
            u8_type(),
            int(1),
        ))
    });
}

#[test]
fn dead_callable_values_still_keep_version_and_skip_optional_rewrites() {
    let mut p = fixture();
    p.body = sequence(pointer(0), int(7));
    assert!(p.uses_callables());
    p.validate().unwrap();
    let before = serde_json::to_vec(&p).unwrap();
    let after = optimize(&p).unwrap();
    assert_eq!(serde_json::to_vec(&after).unwrap(), before);
    p.body = branch(
        Expr::Bool { value: true },
        int(7),
        sequence(pointer(1), int(1)),
    );
    assert!(p.uses_callables());
    p.validate().unwrap();
    assert_eq!(serde_json::to_value(&p).unwrap()["ir_version"], 9);
}

#[test]
fn closed_recursive_target_is_inconclusive_only_when_selected() {
    let recursive = indirect(pointer(0), vec![]);
    let source = program(
        Environment::Strict,
        vec![Type::Bool],
        u8_type(),
        indirect(branch(arg(0), pointer(1), pointer(0)), vec![]),
        vec![
            function(0, vec![], u8_type(), recursive),
            function(1, vec![], u8_type(), int(7)),
        ],
    );
    source.validate().unwrap();
    for optimized in [false, true] {
        let bytes = compile(&source, optimized).unwrap();
        accepted(run(&bytes, &assembled("(1)"), LIMIT), &assembled("7"));
        failed(
            run(&bytes, &assembled("(())"), LIMIT),
            FailureCode::CostExceeded,
        );
    }
}

#[test]
fn complete_target_discovery_and_signature_matching_share_validation_work_budget() {
    let mut source = program(
        Environment::Ignored,
        vec![],
        u8_type(),
        int(7),
        (0..64)
            .map(|id| function(id, vec![u8_type()], u8_type(), arg(0)))
            .collect(),
    );
    for id in (0..64).rev() {
        source.body = sequence(pointer(id), source.body);
    }
    source.validate().unwrap();
    for id in 0..64 {
        let mut body = arg(0);
        for _ in 0..50 {
            body = sequence(indirect(pointer(id), vec![arg(0)]), body);
        }
        source.functions[id].body = body;
    }
    let error = source.validate().unwrap_err().to_string();
    assert!(error.contains("validation work budget"), "{error}");
    for optimized in [false, true] {
        assert!(compile(&source, optimized).is_err());
    }
}

#[test]
fn preexisting_ignored_constant_retains_its_exact_wire_artifact() {
    let source = program(Environment::Ignored, vec![], u8_type(), int(7), vec![]);
    assert!(!source.uses_callables());
    for optimized in [false, true] {
        assert_eq!(compile(&source, optimized).unwrap(), [0xff, 1, 7]);
    }
}

#[test]
fn public_type_query_bounds_unvalidated_reified_signature_metadata() {
    let mut source = fixture();
    source.functions[0].parameters = (0..65)
        .map(|index| chialisp_oxide_ir::Parameter {
            name: index.to_string(),
            ty: u8_type(),
        })
        .collect();
    assert!(
        source
            .type_of(&pointer(0))
            .unwrap_err()
            .to_string()
            .contains("too many reified target parameters")
    );
    let mut source = fixture();
    source.functions[0].result = Type::Tuple {
        items: vec![
            Type::Tuple {
                items: vec![Type::Bool; 256]
            };
            256
        ],
    };
    assert!(
        source
            .type_of(&pointer(0))
            .unwrap_err()
            .to_string()
            .contains("type node budget")
    );
}
