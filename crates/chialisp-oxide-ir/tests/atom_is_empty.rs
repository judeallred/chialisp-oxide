//! Byte-level emptiness, strict evaluation and typed failure controls. These
//! executions do not establish compiler preservation or recursive termination.
use chialisp_oxide_ir::{Environment, Expr, Function, Parameter, Program, Type, compile, optimize};
use chialisp_oxide_vm::{FailureCode, FailureKind, classify_failure};
use clvmr::{
    Allocator, ChiaDialect, MEMPOOL_MODE, run_program,
    serde::{node_from_bytes, node_to_bytes},
};

fn atom(hex: &str) -> Expr {
    Expr::Atom { hex: hex.into() }
}
fn empty(value: Expr) -> Expr {
    Expr::AtomIsEmpty {
        value: Box::new(value),
    }
}
fn arg() -> Expr {
    Expr::Arg { index: 0 }
}
fn parameter(ty: Type) -> Parameter {
    Parameter {
        name: "value".into(),
        ty,
    }
}
fn tree_atom(hex: &str) -> Expr {
    Expr::TreeAtom {
        value: Box::new(atom(hex)),
    }
}
fn atom_value(value: Expr) -> Expr {
    Expr::TreeAtomValue {
        value: Box::new(value),
    }
}
fn sequence(first: Expr, then_value: Expr) -> Expr {
    Expr::Sequence {
        first: Box::new(first),
        then_value: Box::new(then_value),
    }
}
fn rejection() -> Expr {
    Expr::Reject {
        message: "required operand rejection".into(),
    }
}
fn call(arguments: Vec<Expr>) -> Expr {
    Expr::Call {
        function_id: 0,
        arguments,
    }
}
fn program(ty: Type, body: Expr) -> Program {
    Program {
        name: "atom-emptiness".into(),
        environment: Environment::Strict,
        parameters: vec![parameter(ty)],
        result: Type::Bool,
        body,
        functions: vec![],
    }
}
fn unused_function() -> Function {
    Function {
        id: 0,
        name: "unused".into(),
        parameters: vec![],
        result: Type::Unit,
        body: Expr::Unit,
    }
}
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Accepted(Vec<u8>, u64),
    Failed(FailureKind, FailureCode),
}
fn execute(code: &[u8], solution: &str, max_cost: u64) -> Outcome {
    let mut allocator = Allocator::new();
    let code = node_from_bytes(&mut allocator, code).unwrap();
    let environment = rue_clvm::assemble(&mut allocator, solution).unwrap();
    match run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        code,
        environment,
        max_cost,
    ) {
        Ok(result) => Outcome::Accepted(node_to_bytes(&allocator, result.1).unwrap(), result.0),
        Err(error) => {
            let failure = classify_failure(&error);
            Outcome::Failed(failure.kind(), failure.code())
        }
    }
}
fn accepted(outcome: Outcome, expected: &[u8]) -> u64 {
    match outcome {
        Outcome::Accepted(bytes, cost) => {
            assert_eq!(bytes, expected);
            assert!(cost > 0);
            cost
        }
        other => panic!("unexpected execution: {other:?}"),
    }
}
fn modes(p: &Program) -> [(bool, Vec<u8>); 2] {
    p.validate().unwrap();
    [false, true].map(|optimize| (optimize, compile(p, optimize).unwrap()))
}
fn wire_roundtrip(p: &Program, version: Option<u32>) {
    let wire = serde_json::to_value(p).unwrap();
    match version {
        Some(version) => assert_eq!(wire["ir_version"], version),
        None => assert!(wire.get("ir_version").is_none()),
    }
    let decoded: Program = serde_json::from_value(wire.clone()).unwrap();
    decoded.validate().unwrap();
    assert_eq!(decoded.uses_runtime(), p.uses_runtime());
    assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
}
fn assert_failure(outcome: Outcome, kind: FailureKind, code: FailureCode) {
    assert_eq!(outcome, Outcome::Failed(kind, code));
}

#[test]
fn compiled_empty_predicate_matches_native_bytes_without_numeric_coercion() {
    let direct = program(Type::Atom, empty(arg()));
    assert!(!direct.uses_runtime());
    assert!(
        serde_json::to_value(&direct)
            .unwrap()
            .get("ir_version")
            .is_none()
    );
    let mut bound = direct.clone();
    bound.body = Expr::Let {
        local_id: 0,
        ty: Type::Atom,
        value: Box::new(arg()),
        body: Box::new(empty(Expr::Local { local_id: 0 })),
    };
    let mut nested = direct.clone();
    nested.body = empty(Expr::Let {
        local_id: 0,
        ty: Type::Atom,
        value: Box::new(arg()),
        body: Box::new(Expr::Local { local_id: 0 }),
    });
    assert!(nested.uses_runtime());
    assert_eq!(serde_json::to_value(&nested).unwrap()["ir_version"], 4);
    let mut helper = direct.clone();
    helper.body = call(vec![arg()]);
    helper.functions = vec![Function {
        id: 0,
        name: "empty-helper".into(),
        parameters: vec![parameter(Type::Atom)],
        result: Type::Bool,
        body: empty(arg()),
    }];
    let mut payloads = vec![
        vec![],
        vec![0],
        vec![0, 0],
        vec![0, 1],
        vec![1],
        vec![0x80],
        vec![0xff],
        vec![0; 4096],
    ];
    payloads.extend((1..=32).map(|len| (0..len).map(|i| (i * 17 + len) as u8).collect::<Vec<_>>()));
    for p in [direct, bound, nested, helper] {
        wire_roundtrip(&p, p.uses_runtime().then_some(4));
        for (_, code) in modes(&p) {
            for payload in &payloads {
                let atom = if payload.is_empty() {
                    "()".into()
                } else {
                    format!("0x{}", hex::encode(payload))
                };
                // The native oracle tests byte length, never integer conversion.
                let expected: &[u8] = if payload.is_empty() { &[1] } else { &[0x80] };
                accepted(execute(&code, &format!("({atom})"), 1_000_000), expected);
            }
            for malformed in ["()", "1", "(1 . 2)", "(1 2)", "((1 . 2))"] {
                // Existing strict ABI guards may raise or reject an invalid
                // opcode operand (for example, equality on a non-atom tail).
                // Their exact diagnostic is not part of this byte predicate.
                assert!(
                    matches!(
                        execute(&code, malformed, 1_000_000),
                        Outcome::Failed(FailureKind::Program, _)
                    ),
                    "malformed ABI input {malformed:?} must be a conclusive program rejection"
                );
            }
        }
    }
}

#[test]
fn checked_raw_atom_projection_keeps_pair_rejection_and_byte_identity() {
    let mut p = program(Type::Tree, empty(atom_value(arg())));
    p.environment = Environment::Raw;
    for runtime in [false, true] {
        p.functions = if runtime {
            vec![unused_function()]
        } else {
            vec![]
        };
        wire_roundtrip(&p, Some(if runtime { 4 } else { 3 }));
        for (_, code) in modes(&p) {
            for (input, expected) in [
                ("()", &[1][..]),
                ("0x00", &[0x80][..]),
                ("0x0001", &[0x80][..]),
            ] {
                accepted(execute(&code, input, 1_000_000), expected);
            }
            assert_failure(
                execute(&code, "(1 . 2)", 1_000_000),
                FailureKind::Program,
                FailureCode::Raise,
            );
        }
    }
}

#[test]
fn emptiness_requires_atom_type_even_when_a_bad_operand_is_unreachable() {
    for value in [
        Expr::Unit,
        Expr::Bool { value: false },
        tree_atom(""),
        Expr::Bytes { hex: "".into() },
        Expr::Bytes { hex: "00".into() },
        Expr::Int {
            value: "0".into(),
            ty: Type::Int {
                bits: 8,
                signed: false,
            },
        },
        Expr::Tuple {
            items: vec![atom("")],
        },
    ] {
        let mut p = program(Type::Atom, empty(value.clone()));
        assert!(
            p.validate()
                .unwrap_err()
                .to_string()
                .contains("requires an atom")
        );
        p.body = Expr::If {
            condition: Box::new(Expr::Bool { value: true }),
            then_value: Box::new(Expr::Bool { value: true }),
            else_value: Box::new(empty(value)),
        };
        assert!(p.validate().is_err());
        for optimize in [false, true] {
            assert!(compile(&p, optimize).is_err());
        }
    }
}

#[test]
fn empty_tests_preserve_failing_operands_and_lazy_branches_in_both_pipelines() {
    for runtime in [false, true] {
        for operand in [rejection(), sequence(rejection(), atom(""))] {
            let mut p = program(Type::Atom, empty(operand));
            if runtime {
                p.functions.push(unused_function());
            }
            for (_, code) in modes(&p) {
                assert_failure(
                    execute(&code, "(())", 1_000_000),
                    FailureKind::Program,
                    FailureCode::Raise,
                );
            }
        }
        let mut p = program(
            Type::Atom,
            empty(Expr::If {
                condition: Box::new(Expr::Bool { value: true }),
                then_value: Box::new(atom("")),
                else_value: Box::new(rejection()),
            }),
        );
        if runtime {
            p.functions.push(unused_function());
        }
        for (_, code) in modes(&p) {
            accepted(execute(&code, "(())", 1_000_000), &[1]);
        }
    }
}

#[test]
fn guard_facts_traverse_atom_predicates_but_runtime_guards_remain_untouched() {
    let mut p = program(
        Type::Tree,
        Expr::If {
            condition: Box::new(Expr::TreeIsPair {
                value: Box::new(arg()),
            }),
            then_value: Box::new(sequence(
                empty(atom_value(Expr::TreeFirst {
                    value: Box::new(arg()),
                })),
                tree_atom("aa"),
            )),
            else_value: Box::new(rejection()),
        },
    );
    p.environment = Environment::Raw;
    p.result = Type::Tree;
    let after = optimize(&p).unwrap();
    assert!(
        matches!(after.body, Expr::Sequence { .. }),
        "the predicate's checked child supplies the only pair requirement"
    );
    for (_, code) in modes(&p) {
        for input in ["(() . 1)", "(0x00 . 2)"] {
            accepted(execute(&code, input, 1_000_000), &[0x81, 0xaa]);
        }
        for input in ["()", "((1 . 2) . 3)"] {
            assert!(matches!(
                execute(&code, input, 1_000_000),
                Outcome::Failed(FailureKind::Program, _)
            ));
        }
    }
    p.functions.push(unused_function());
    assert_eq!(
        serde_json::to_value(optimize(&p).unwrap()).unwrap(),
        serde_json::to_value(&p).unwrap()
    );
}

#[test]
fn cost_exhaustion_in_an_atom_operand_is_not_reclassified_as_false_or_rejection() {
    let mut p = program(Type::Atom, empty(call(vec![])));
    p.functions = vec![Function {
        id: 0,
        name: "spin-atom".into(),
        parameters: vec![],
        result: Type::Atom,
        body: call(vec![]),
    }];
    for (_, code) in modes(&p) {
        assert_failure(
            execute(&code, "(())", 100_000),
            FailureKind::Resource,
            FailureCode::CostExceeded,
        );
    }
}

#[test]
fn runtime_atom_operand_executes_once_even_when_it_traverses_a_tree() {
    let mut once = program(Type::Tree, empty(call(vec![arg()])));
    once.environment = Environment::Raw;
    once.functions = vec![Function {
        id: 0,
        name: "rightmost-atom".into(),
        parameters: vec![parameter(Type::Tree)],
        result: Type::Atom,
        body: Expr::If {
            condition: Box::new(Expr::TreeIsPair {
                value: Box::new(arg()),
            }),
            then_value: Box::new(call(vec![Expr::TreeRest {
                value: Box::new(arg()),
            }])),
            else_value: Box::new(atom_value(arg())),
        },
    }];
    let mut twice = once.clone();
    twice.body = empty(sequence(call(vec![arg()]), call(vec![arg()])));
    let mut input = "0x0001".to_owned();
    for _ in 0..128 {
        input = format!("(1 . {input})");
    }
    for optimize in [false, true] {
        let one_code = compile(&once, optimize).unwrap();
        let two_code = compile(&twice, optimize).unwrap();
        let one_cost = accepted(execute(&one_code, &input, 100_000_000), &[0x80]);
        let two_cost = accepted(execute(&two_code, &input, 100_000_000), &[0x80]);
        assert!(two_cost > one_cost, "duplicated traversal must cost more");
        let budget = one_cost + (two_cost - one_cost) / 2;
        accepted(execute(&one_code, &input, budget), &[0x80]);
        assert_failure(
            execute(&two_code, &input, budget),
            FailureKind::Resource,
            FailureCode::CostExceeded,
        );
    }
}
