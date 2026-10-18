//! Finite execution and structural controls for optional closed-helper inlining.
//! This is not a compiler-preservation or universal cost proof.
#[path = "runtime_evaluation/support.rs"]
#[allow(dead_code, reason = "Shared helpers also serve other runtime tests.")]
mod support;

use chialisp_oxide_ir::{Environment, Expr, Program, Type, compile, optimize};
use chialisp_oxide_vm::FailureCode;
use support::*;

fn wire(program: &Program) -> Vec<u8> {
    serde_json::to_vec(program).unwrap()
}
fn tuple_type(items: Vec<Type>) -> Type {
    Type::Tuple { items }
}
fn hash(value: Expr) -> Expr {
    Expr::Sha256 { parts: vec![value] }
}
fn literal(hex: &str) -> Expr {
    Expr::Atom { hex: hex.into() }
}
fn count(expr: &Expr, wanted: &str) -> usize {
    fn visit(value: &serde_json::Value, wanted: &str) -> usize {
        match value {
            serde_json::Value::Object(map) => {
                usize::from(map.get("op").and_then(|v| v.as_str()) == Some(wanted))
                    + map.values().map(|v| visit(v, wanted)).sum::<usize>()
            }
            serde_json::Value::Array(items) => items.iter().map(|v| visit(v, wanted)).sum(),
            _ => 0,
        }
    }
    visit(&serde_json::to_value(expr).unwrap(), wanted)
}

#[test]
fn identity_entry_and_tiny_constructors_prune_without_changing_strict_abi_or_off() {
    let result = tuple_type(vec![u8_type(), u8_type()]);
    let source = program(
        Environment::Strict,
        vec![u8_type()],
        result.clone(),
        call(1, vec![arg(0)]),
        vec![
            function(
                0,
                vec![u8_type()],
                result.clone(),
                Expr::Tuple {
                    items: vec![int(51), arg(0)],
                },
            ),
            function(1, vec![u8_type()], result, call(0, vec![arg(0)])),
        ],
    );
    let original = wire(&source);
    let off = compile(&source, false).unwrap();
    let reduced = optimize(&source).unwrap();
    reduced.validate().unwrap();
    assert!(reduced.functions.is_empty());
    assert_eq!(count(&reduced.body, "call"), 0);
    assert_eq!(wire(&source), original);
    assert_eq!(compile(&source, false).unwrap(), off);
    let on = compile(&source, true).unwrap();
    assert!(on.len() < off.len());
    for value in [0, 1, 127, 128, 255] {
        let environment = list(&[uint(value)]);
        let expected = list(&[uint(51), uint(value)]);
        for code in [&off, &on] {
            accepted(run(code, &environment, LIMIT), &expected);
        }
    }
    for environment in [
        atom(&[]),
        list(&[atom(&[0])]),
        list(&[uint(256)]),
        list(&[pair(&atom(&[]), &atom(&[]))]),
        list(&[uint(1), uint(2)]),
    ] {
        for code in [&off, &on] {
            abi_rejected(run(code, &environment, LIMIT));
        }
    }
}

#[test]
fn alpha_renaming_preserves_caller_arguments_and_disjoint_local_id_reuse() {
    let pair_type = tuple_type(vec![Type::Tree, Type::Tree]);
    let result = tuple_type(vec![pair_type.clone(), Type::Tree, Type::Tree]);
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        result,
        binding(
            0,
            Type::Tree,
            tree_atom("aa"),
            Expr::Tuple {
                items: vec![
                    call(
                        0,
                        vec![
                            binding(usize::MAX, Type::Tree, arg(0), local(usize::MAX)),
                            local(0),
                        ],
                    ),
                    binding(2, Type::Tree, tree_atom("bb"), local(2)),
                    local(0),
                ],
            },
        ),
        vec![function(
            0,
            vec![Type::Tree, Type::Tree],
            pair_type,
            Expr::Tuple {
                items: vec![
                    binding(0, Type::Tree, arg(0), local(0)),
                    binding(0, Type::Tree, arg(1), local(0)),
                ],
            },
        )],
    );
    // Callee local zero is legally reused in disjoint sibling scopes.
    let reduced = optimize(&source).unwrap();
    reduced.validate().unwrap();
    assert!(reduced.functions.is_empty());
    for input in [atom(&[0, 0]), pair(&atom(&[1]), &atom(&[255]))] {
        let expected = list(&[
            list(&[input.clone(), atom(&[170])]),
            atom(&[187]),
            atom(&[170]),
        ]);
        for (_, code) in modes(&source) {
            accepted(run(&code, &input, LIMIT), &expected);
        }
    }
}

#[test]
fn unused_arguments_keep_overflow_rejection_divergence_and_left_to_right_order() {
    for spin_first in [false, true] {
        let (left, right) = if spin_first {
            (call(1, vec![]), add(int(255), int(1)))
        } else {
            (add(int(255), int(1)), call(1, vec![]))
        };
        let source = program(
            Environment::Ignored,
            vec![],
            u8_type(),
            call(0, vec![left, right]),
            vec![
                function(0, vec![u8_type(), u8_type()], u8_type(), int(7)),
                function(1, vec![], u8_type(), call(1, vec![])),
            ],
        );
        let reduced = optimize(&source).unwrap();
        assert_eq!(
            reduced.functions.len(),
            1,
            "only the recursive function remains"
        );
        for (_, code) in modes(&source) {
            failed(
                run(&code, &atom(&[]), LIMIT),
                if spin_first {
                    FailureCode::CostExceeded
                } else {
                    FailureCode::Raise
                },
            );
        }
    }
}

#[test]
fn unused_hash_arguments_keep_resource_failure_and_order_relative_to_raise() {
    let bytes = Type::Bytes { len: 32 };
    let large = hash(literal(&"aa".repeat(4096)));
    for hash_first in [false, true] {
        let reject = Expr::Reject {
            message: "before or after hash".into(),
        };
        let args = if hash_first {
            vec![large.clone(), reject]
        } else {
            vec![reject, large.clone()]
        };
        let source = program(
            Environment::Ignored,
            vec![],
            u8_type(),
            call(0, args),
            vec![function(
                0,
                vec![bytes.clone(), bytes.clone()],
                u8_type(),
                int(7),
            )],
        );
        let reduced = optimize(&source).unwrap();
        assert!(reduced.functions.is_empty());
        assert_eq!(count(&reduced.body, "sha256"), 1);
        for (_, code) in modes(&source) {
            failed(
                run(&code, &atom(&[]), 1_000),
                if hash_first {
                    FailureCode::CostExceeded
                } else {
                    FailureCode::Raise
                },
            );
        }
    }
    let source = program(
        Environment::Ignored,
        vec![],
        u8_type(),
        call(0, vec![large]),
        vec![function(0, vec![bytes], u8_type(), int(7))],
    );
    for (_, code) in modes(&source) {
        failed(run(&code, &atom(&[]), 1_000), FailureCode::CostExceeded);
        accepted(run(&code, &atom(&[]), FINITE_LIMIT), &uint(7));
    }
}

#[test]
fn a_reused_hash_argument_is_computed_once_and_matches_an_independent_known_answer() {
    let bytes = Type::Bytes { len: 32 };
    let result = tuple_type(vec![bytes.clone(), bytes.clone()]);
    let source = program(
        Environment::Strict,
        vec![Type::Atom],
        result.clone(),
        call(0, vec![hash(arg(0))]),
        vec![function(
            0,
            vec![bytes],
            result.clone(),
            Expr::Tuple {
                items: vec![arg(0), arg(0)],
            },
        )],
    );
    let reduced = optimize(&source).unwrap();
    assert!(reduced.functions.is_empty());
    assert_eq!(count(&reduced.body, "sha256"), 1);
    let expected =
        hex::decode("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad").unwrap();
    let expected = list(&[atom(&expected), atom(&expected)]);
    let input = list(&[atom(b"abc")]);
    let [(_, off), (_, on)] = modes(&source);
    accepted(run(&off, &input, LIMIT), &expected);
    let once_cost = accepted(run(&on, &input, LIMIT), &expected);
    let duplicated = program(
        Environment::Strict,
        vec![Type::Atom],
        result,
        Expr::Tuple {
            items: vec![hash(arg(0)), hash(arg(0))],
        },
        vec![],
    );
    let twice = compile(&duplicated, true).unwrap();
    let twice_cost = accepted(run(&twice, &input, LIMIT), &expected);
    assert!(
        once_cost < twice_cost,
        "one hash {once_cost}, two hashes {twice_cost}"
    );
    accepted(run(&on, &input, once_cost), &expected);
    failed(run(&twice, &input, once_cost), FailureCode::CostExceeded);
}

#[test]
fn lazy_calls_and_mandatory_decoder_checks_survive_pruning() {
    let source = program(
        Environment::Strict,
        vec![Type::Bool],
        u8_type(),
        branch(
            arg(0),
            call(
                0,
                vec![Expr::Reject {
                    message: "selected only".into(),
                }],
            ),
            int(9),
        ),
        vec![function(0, vec![u8_type()], u8_type(), arg(0))],
    );
    assert!(optimize(&source).unwrap().functions.is_empty());
    for (_, code) in modes(&source) {
        accepted(run(&code, &list(&[atom(&[])]), LIMIT), &uint(9));
        failed(run(&code, &list(&[atom(&[1])]), LIMIT), FailureCode::Raise);
        for invalid in [
            list(&[atom(&[0])]),
            list(&[atom(&[2])]),
            list(&[]),
            list(&[atom(&[]), atom(&[])]),
        ] {
            abi_rejected(run(&code, &invalid, LIMIT));
        }
    }
}

#[test]
fn recursive_sccs_stay_closed_and_surviving_function_ids_are_remapped() {
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        call(1, vec![arg(0)]),
        vec![
            function(0, vec![], Type::Tree, tree_atom("ff")),
            function(1, vec![Type::Tree], Type::Tree, call(2, vec![arg(0)])),
            rightmost(2, 2),
        ],
    );
    let reduced = optimize(&source).unwrap();
    assert_eq!(reduced.functions.len(), 1);
    assert_eq!(reduced.functions[0].id, 0);
    assert_eq!(reduced.functions[0].name, "function_2");
    assert!(matches!(reduced.body, Expr::Call { function_id: 0, .. }));
    for (_, code) in modes(&source) {
        let input = right_spine(32, &atom(&[0, 1]));
        accepted(run(&code, &input, FINITE_LIMIT), &atom(&[0, 1]));
    }
    let mutual = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        call(0, vec![arg(0)]),
        vec![rightmost(0, 1), rightmost(1, 0)],
    );
    let reduced = optimize(&mutual).unwrap();
    assert_eq!(reduced.functions.len(), 2);
    assert!(matches!(reduced.body, Expr::Call { function_id: 0, .. }));
    for (_, code) in modes(&mutual) {
        accepted(
            run(&code, &right_spine(31, &atom(&[128])), FINITE_LIMIT),
            &atom(&[128]),
        );
    }
}

#[test]
fn unreachable_invalid_functions_are_checked_before_any_pruning() {
    let invalid = program(
        Environment::Ignored,
        vec![],
        u8_type(),
        int(7),
        vec![function(0, vec![], u8_type(), arg(0))],
    );
    assert!(
        optimize(&invalid)
            .unwrap_err()
            .to_string()
            .contains("invalid argument")
    );
}

#[test]
fn optional_growth_and_literal_copy_limits_leave_valid_original_tables_intact() {
    let mut body = arg(0);
    for _ in 0..15 {
        body = Expr::Not {
            value: Box::new(body),
        };
    }
    let row_type = tuple_type(vec![Type::Bool; 32]);
    let rows = (0..256)
        .map(|_| Expr::Tuple {
            items: (0..32).map(|_| call(0, vec![arg(0)])).collect(),
        })
        .collect();
    let growth = program(
        Environment::Strict,
        vec![Type::Bool],
        tuple_type(vec![row_type; 256]),
        Expr::Tuple { items: rows },
        vec![function(0, vec![Type::Bool], Type::Bool, body)],
    );
    growth.validate().unwrap();
    let reduced = optimize(&growth).unwrap();
    assert_eq!(
        reduced.functions.len(),
        1,
        "unbounded body multiplication must skip inlining"
    );
    assert_eq!(count(&reduced.body, "call"), 8192);

    let copies = program(
        Environment::Ignored,
        vec![],
        tuple_type(vec![tuple_type(vec![Type::Atom; 256]); 3]),
        Expr::Tuple {
            items: (0..3)
                .map(|_| Expr::Tuple {
                    items: (0..256).map(|_| call(0, vec![])).collect(),
                })
                .collect(),
        },
        vec![function(0, vec![], Type::Atom, literal(&"aa".repeat(4096)))],
    );
    copies.validate().unwrap();
    let reduced = optimize(&copies).unwrap();
    assert_eq!(
        reduced.functions.len(),
        1,
        "literal copying must be charged before allocation"
    );
    assert_eq!(count(&reduced.body, "call"), 768);
}

#[test]
fn wide_operand_siblings_cannot_hide_new_inlined_alias_frames() {
    let pair_type = tuple_type(vec![Type::Tree, Type::Tree]);
    let helper = function(
        0,
        vec![Type::Tree, Type::Tree],
        pair_type.clone(),
        binding(
            0,
            Type::Tree,
            arg(0),
            Expr::Tuple {
                items: vec![local(0), arg(1)],
            },
        ),
    );
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        tuple_type(vec![pair_type; 127]),
        Expr::Tuple {
            items: (0..127).map(|_| call(0, vec![arg(0), arg(0)])).collect(),
        },
        vec![helper],
    );
    source.validate().unwrap();
    let reduced = optimize(&source).unwrap();
    assert_eq!(
        reduced.functions.len(),
        1,
        "last sibling would have 126 outer temporaries plus three new aliases"
    );
    assert_eq!(count(&reduced.body, "call"), 127);
}

#[test]
fn deep_inputs_skip_optional_inlining_before_cloning_or_pruning() {
    let mut body = arg(0);
    for _ in 0..64 {
        body = Expr::Not {
            value: Box::new(body),
        };
    }
    let source = program(
        Environment::Strict,
        vec![Type::Bool],
        Type::Bool,
        body,
        vec![function(0, vec![], u8_type(), int(7))],
    );
    source.validate().unwrap();
    assert_eq!(wire(&optimize(&source).unwrap()), wire(&source));
}

#[test]
fn serialized_optimization_keeps_runtime_order_when_pruning_the_last_unused_table() {
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Bool,
        Expr::Binary {
            kind: chialisp_oxide_ir::BinaryOp::Eq,
            ty: Type::Atom,
            left: Box::new(Expr::Reject {
                message: "first operand rejects".into(),
            }),
            right: Box::new(Expr::TreeAtomValue {
                value: Box::new(first(arg(0))),
            }),
        },
        vec![function(0, vec![], Type::Unit, Expr::Unit)],
    );
    let reduced = optimize(&source).unwrap();
    assert!(
        reduced.uses_runtime(),
        "the wire format has no separate evaluation-mode field"
    );
    assert_eq!(reduced.functions.len(), 1);
    let roundtrip: Program = serde_json::from_slice(&wire(&reduced)).unwrap();
    for (_, code) in modes(&roundtrip) {
        // Legacy equality's right-first evaluation would execute the checked
        // projection first and produce a different typed VM failure.
        failed(run(&code, &atom(&[]), LIMIT), FailureCode::Raise);
    }
}

#[test]
fn serialized_optimization_keeps_runtime_order_when_refinement_removes_the_last_let() {
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Bool,
        binding(
            0,
            Type::Unit,
            Expr::Unit,
            Expr::Binary {
                kind: chialisp_oxide_ir::BinaryOp::Eq,
                ty: Type::Atom,
                left: Box::new(Expr::Reject {
                    message: "first operand rejects".into(),
                }),
                right: Box::new(Expr::TreeAtomValue {
                    value: Box::new(first(arg(0))),
                }),
            },
        ),
        vec![],
    );
    let original = wire(&source);
    assert!(source.uses_runtime());
    assert!(source.functions.is_empty());

    let reduced = optimize(&source).unwrap();
    let roundtrip: Program = serde_json::from_slice(&wire(&reduced)).unwrap();
    for mode in [false, true] {
        let original_code = compile(&source, mode).unwrap();
        failed(run(&original_code, &atom(&[]), LIMIT), FailureCode::Raise);
        let roundtrip_code = compile(&roundtrip, mode).unwrap();
        // The unused total initializer can disappear in the private compiler
        // plan, which separately pins runtime evaluation. A public serialized
        // Program must keep that mode: legacy equality evaluates the checked
        // projection before the explicit first-operand Raise on this input.
        failed(run(&roundtrip_code, &atom(&[]), LIMIT), FailureCode::Raise);
    }
    assert_eq!(
        wire(&source),
        original,
        "optimization must not mutate input"
    );
    assert_eq!(
        wire(&reduced),
        original,
        "without a separate wire-mode field the public API must retain its source"
    );
    assert!(roundtrip.uses_runtime());
    assert!(
        roundtrip.functions.is_empty(),
        "do not invent a table sentinel"
    );
}

#[test]
fn public_refinement_can_return_total_leaves_without_changing_strict_abi_decoding() {
    let examples = [
        (Type::Atom, arg(0), None),
        (Type::Atom, literal("0000"), Some(atom(&[0, 0]))),
        (
            Type::Bytes { len: 2 },
            Expr::Bytes { hex: "00ff".into() },
            Some(atom(&[0, 255])),
        ),
        (Type::Bool, Expr::Bool { value: true }, Some(atom(&[1]))),
        (u8_type(), int(7), Some(uint(7))),
        (Type::Unit, Expr::Unit, Some(atom(&[]))),
    ];
    for (result, leaf, expected) in examples {
        let source = program(
            Environment::Strict,
            vec![Type::Atom],
            result,
            binding(0, Type::Unit, Expr::Unit, leaf),
            vec![],
        );
        let original = wire(&source);
        let reduced = optimize(&source).unwrap();
        assert!(!reduced.uses_runtime(), "a total leaf needs no mode marker");
        let roundtrip: Program = serde_json::from_slice(&wire(&reduced)).unwrap();
        for value in [&source, &roundtrip] {
            for (_, code) in modes(value) {
                for payload in [&[][..], &[0, 0], &[0x80, 0xff]] {
                    let input = atom(payload);
                    accepted(
                        run(&code, &list(std::slice::from_ref(&input)), LIMIT),
                        expected.as_ref().unwrap_or(&input),
                    );
                }
                for malformed in [
                    atom(&[]),
                    list(&[atom(&[]), atom(&[])]),
                    list(&[pair(&atom(&[]), &atom(&[]))]),
                    pair(&atom(&[]), &atom(&[1])),
                ] {
                    // The complete ABI decoder can reject through a semantic
                    // opcode before reaching its explicit Raise. Require a
                    // conclusive program failure in every mode.
                    abi_rejected(run(&code, &malformed, LIMIT));
                }
            }
        }
        assert_eq!(wire(&source), original);
    }
}
