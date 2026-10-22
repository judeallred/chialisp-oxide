//! Executed decoder-continuation regressions, not a preservation theorem.
use super::*;
use crate::{Environment, Function, Parameter};
use chialisp_oxide_vm::{FailureCode, FailureKind, classify_failure};
use clvmr::{ChiaDialect, MEMPOOL_MODE, run_program, serde::node_from_bytes};

const PAYLOADS: usize = 20;
const BYTES: usize = 4096;

#[derive(Clone, Copy)]
enum Shape {
    Flat,
    Tuple,
    Pair,
}

fn uint_type() -> Type {
    Type::Int {
        bits: 64,
        signed: false,
    }
}

fn parameter(index: usize, ty: Type) -> Parameter {
    Parameter {
        name: format!("arg_{index}"),
        ty,
    }
}

fn fixture(shape: Shape, payloads: usize, bytes: usize) -> Program {
    let (parameters, first, second, flag) = match shape {
        Shape::Flat => (
            vec![
                parameter(0, uint_type()),
                parameter(1, uint_type()),
                parameter(2, Type::Bool),
            ],
            Expr::Arg { index: 0 },
            Expr::Arg { index: 1 },
            2,
        ),
        Shape::Tuple => (
            vec![
                parameter(
                    0,
                    Type::Tuple {
                        items: vec![uint_type(); 2],
                    },
                ),
                parameter(1, Type::Bool),
            ],
            Expr::Field {
                value: Box::new(Expr::Arg { index: 0 }),
                index: 0,
            },
            Expr::Field {
                value: Box::new(Expr::Arg { index: 0 }),
                index: 1,
            },
            1,
        ),
        Shape::Pair => (
            vec![
                parameter(
                    0,
                    Type::Pair {
                        first: Box::new(uint_type()),
                        rest: Box::new(uint_type()),
                    },
                ),
                parameter(1, Type::Bool),
            ],
            Expr::PairFirst {
                value: Box::new(Expr::Arg { index: 0 }),
            },
            Expr::PairRest {
                value: Box::new(Expr::Arg { index: 0 }),
            },
            1,
        ),
    };
    let mut items = (0..payloads)
        .map(|index| Expr::Bytes {
            hex: hex::encode(vec![0x40 + u8::try_from(index).unwrap(); bytes]),
        })
        .collect::<Vec<_>>();
    items.extend([first, second]);
    let mut types = vec![Type::Bytes { len: bytes }; payloads];
    types.extend([uint_type(), uint_type()]);
    let item = Type::Tuple { items: types };
    Program {
        name: "decoder-continuation".into(),
        environment: Environment::Strict,
        parameters,
        result: Type::Optional {
            item: Box::new(item.clone()),
        },
        body: Expr::If {
            condition: Box::new(Expr::Arg { index: flag }),
            then_value: Box::new(Expr::OptionalSome {
                value: Box::new(Expr::Tuple { items }),
            }),
            else_value: Box::new(Expr::OptionalNone { item }),
        },
        functions: vec![],
    }
}

// Independent bounded wire construction; no assembler or compiler output is
// used to supply expected argument values, payload order or optional shape.
fn atom(payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() <= BYTES);
    if payload.len() == 1 && payload[0] < 128 {
        return payload.to_vec();
    }
    let mut result = if payload.len() < 64 {
        vec![0x80 | u8::try_from(payload.len()).unwrap()]
    } else {
        vec![
            0xc0 | u8::try_from(payload.len() >> 8).unwrap(),
            payload.len() as u8,
        ]
    };
    result.extend_from_slice(payload);
    result
}

fn uint(value: u64) -> Vec<u8> {
    if value == 0 {
        return atom(&[]);
    }
    let bytes = value.to_be_bytes();
    let first = bytes.iter().position(|byte| *byte != 0).unwrap();
    let mut payload = Vec::new();
    if bytes[first] & 128 != 0 {
        payload.push(0);
    }
    payload.extend_from_slice(&bytes[first..]);
    atom(&payload)
}

fn pair(first: &[u8], rest: &[u8]) -> Vec<u8> {
    assert!(first.len() + rest.len() <= 2_000_000);
    let mut result = vec![0xff];
    result.extend_from_slice(first);
    result.extend_from_slice(rest);
    result
}

fn list(values: &[Vec<u8>]) -> Vec<u8> {
    values
        .iter()
        .rev()
        .fold(atom(&[]), |rest, first| pair(first, &rest))
}

fn environment(shape: Shape, first: &[u8], second: &[u8], flag: &[u8]) -> Vec<u8> {
    match shape {
        Shape::Flat => list(&[first.to_vec(), second.to_vec(), flag.to_vec()]),
        Shape::Tuple => list(&[list(&[first.to_vec(), second.to_vec()]), flag.to_vec()]),
        Shape::Pair => list(&[pair(first, second), flag.to_vec()]),
    }
}

fn expected(first: u64, second: u64, flag: bool) -> Vec<u8> {
    if !flag {
        return atom(&[]);
    }
    let mut payload = (0..PAYLOADS)
        .map(|index| atom(&vec![0x40 + u8::try_from(index).unwrap(); BYTES]))
        .collect::<Vec<_>>();
    payload.extend([uint(first), uint(second)]);
    list(&[list(&payload)])
}

fn run(code: &[u8], environment: &[u8], budget: u64) -> std::result::Result<Vec<u8>, FailureCode> {
    let mut allocator = Allocator::new();
    let code = node_from_bytes(&mut allocator, code).unwrap();
    let environment = node_from_bytes(&mut allocator, environment).unwrap();
    match run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        code,
        environment,
        budget,
    ) {
        Ok(result) => Ok(node_to_bytes(&allocator, result.1).unwrap()),
        Err(error) => Err(classify_failure(&error).code()),
    }
}

fn attempt(program: &Program, optimize: bool, separate: bool) -> Result<Vec<u8>> {
    decoder_attempt(
        program,
        optimize,
        if separate {
            Decoder::Separate
        } else {
            Decoder::Inline
        },
    )
}

fn decoder_attempt(program: &Program, optimize: bool, decoder: Decoder) -> Result<Vec<u8>> {
    emit_decoder_attempt(
        program,
        optimize,
        program.uses_runtime(),
        false,
        false,
        false,
        decoder,
    )
}

fn size_error(error: &anyhow::Error) {
    assert!(
        matches!(
            error.downcast_ref::<limits::Limit>(),
            Some(limits::Limit::ExpansionSize)
        ),
        "{error:?}"
    );
}

fn natural_retry(program: &Program) -> Vec<u8> {
    program.validate().unwrap();
    size_error(&attempt(program, false, false).unwrap_err());
    let separated = attempt(program, false, true).unwrap();
    size_error(
        &emit_layout_attempt(program, false, program.uses_runtime(), false, false, false)
            .unwrap_err(),
    );
    size_error(&compile_existing(program, false).unwrap_err());
    assert_eq!(compile(program, false).unwrap(), separated);
    separated
}

#[test]
fn typed_size_retry_keeps_original_environment_and_both_optional_branches() {
    let source = fixture(Shape::Flat, PAYLOADS, BYTES);
    let codes = [
        natural_retry(&source),
        attempt(&source, true, true).unwrap(),
        compile(&source, true).unwrap(),
    ];
    for value in [0, 1, 127, 128, 255, 256, (1 << 63) - 1, 1 << 63, u64::MAX] {
        for flag in [false, true] {
            let second = u64::MAX - value;
            let input = environment(
                Shape::Flat,
                &uint(value),
                &uint(second),
                &uint(u64::from(flag)),
            );
            for code in &codes {
                assert_eq!(
                    run(code, &input, 10_000_000).unwrap(),
                    expected(value, second, flag)
                );
            }
        }
    }
    let mut malformed = vec![
        atom(&[]),
        uint(1),
        list(&[uint(1), uint(2)]),
        list(&[uint(1), uint(2), uint(1), uint(4)]),
        pair(&uint(1), &uint(2)),
    ];
    for bad in [
        atom(&[0]),
        atom(&[0, 1]),
        atom(&[0xff]),
        atom(&[0, 0x80, 0, 0, 0, 0, 0, 0, 0, 0]),
        atom(&[1, 0, 0, 0, 0, 0, 0, 0, 0]),
        list(&[uint(1)]),
    ] {
        for flag in [false, true] {
            malformed.push(environment(
                Shape::Flat,
                &bad,
                &uint(7),
                &uint(u64::from(flag)),
            ));
            malformed.push(environment(
                Shape::Flat,
                &uint(7),
                &bad,
                &uint(u64::from(flag)),
            ));
        }
    }
    for bad in [uint(2), atom(&[0]), atom(&[0, 1]), list(&[uint(1)])] {
        malformed.push(environment(Shape::Flat, &uint(5), &uint(7), &bad));
    }
    for tail in [uint(1), atom(&[0]), atom(&[0, 0])] {
        malformed.push(pair(&uint(5), &pair(&uint(7), &pair(&uint(1), &tail))));
    }
    for input in malformed {
        for code in &codes {
            let failure = run(code, &input, 1_000_000).unwrap_err();
            assert_eq!(
                failure.kind(),
                FailureKind::Program,
                "malformed ABI must not exhaust resources"
            );
        }
    }
}

#[test]
fn tuple_nil_and_explicit_pair_tails_keep_distinct_abi_contracts() {
    for shape in [Shape::Tuple, Shape::Pair] {
        let code = natural_retry(&fixture(shape, PAYLOADS, BYTES));
        for flag in [false, true] {
            let input = environment(shape, &uint(128), &uint(u64::MAX), &uint(u64::from(flag)));
            assert_eq!(
                run(&code, &input, 10_000_000).unwrap(),
                expected(128, u64::MAX, flag)
            );
        }
        let wrong_shape = match shape {
            Shape::Tuple => pair(&uint(5), &uint(7)),
            Shape::Pair => list(&[uint(5), uint(7)]),
            Shape::Flat => unreachable!(),
        };
        let mut malformed = vec![list(&[wrong_shape, uint(1)]), list(&[atom(&[]), uint(1)])];
        for tail in [atom(&[0]), atom(&[0, 0]), uint(1)] {
            let tuple = pair(&uint(5), &pair(&uint(7), &tail));
            malformed.push(list(&[tuple, uint(1)]));
        }
        for input in malformed {
            assert_eq!(
                run(&code, &input, 1_000_000).unwrap_err().kind(),
                FailureKind::Program
            );
        }
    }
}

#[test]
fn abi_failure_precedes_divergence_and_unused_initializers_remain_strict() {
    let mut diverging = fixture(Shape::Flat, PAYLOADS, BYTES);
    diverging.functions.push(Function {
        id: 0,
        name: "spin".into(),
        parameters: vec![],
        result: Type::Unit,
        body: Expr::Call {
            function_id: 0,
            arguments: vec![],
        },
    });
    diverging.body = Expr::Sequence {
        first: Box::new(Expr::Call {
            function_id: 0,
            arguments: vec![],
        }),
        then_value: Box::new(diverging.body),
    };
    let code = natural_retry(&diverging);
    let valid = environment(Shape::Flat, &uint(5), &uint(7), &uint(0));
    let padded = environment(Shape::Flat, &atom(&[0]), &uint(7), &uint(0));
    assert_eq!(run(&code, &valid, 50_000), Err(FailureCode::CostExceeded));
    assert_eq!(run(&code, &padded, 50_000), Err(FailureCode::Raise));

    let mut strict = fixture(Shape::Flat, PAYLOADS, BYTES);
    strict.body = Expr::Let {
        local_id: 0,
        ty: Type::Unit,
        value: Box::new(Expr::Reject {
            message: "unused initializer still rejects".into(),
        }),
        body: Box::new(strict.body),
    };
    let code = natural_retry(&strict);
    for flag in [false, true] {
        let input = environment(Shape::Flat, &uint(5), &uint(7), &uint(u64::from(flag)));
        assert_eq!(run(&code, &input, 1_000_000), Err(FailureCode::Raise));
        let optimized_separate = attempt(&strict, true, true).unwrap();
        assert_eq!(
            run(&optimized_separate, &input, 1_000_000),
            Err(FailureCode::Raise)
        );
    }
}

#[test]
fn successful_original_emitters_keep_exact_bytes_including_non_strict_policies() {
    let small = fixture(Shape::Flat, 1, 1);
    let mut raw = small.clone();
    raw.environment = Environment::Raw;
    raw.parameters = vec![parameter(0, Type::Tree)];
    raw.result = Type::Tree;
    raw.body = Expr::Arg { index: 0 };
    let mut ignored = small.clone();
    ignored.environment = Environment::Ignored;
    ignored.parameters.clear();
    ignored.result = Type::Bool;
    ignored.body = Expr::Bool { value: true };
    for source in [small, raw, ignored] {
        for optimize in [false, true] {
            assert_eq!(
                compile(&source, optimize).unwrap(),
                compile_existing(&source, optimize).unwrap()
            );
            let previous = attempt(&source, optimize, false).unwrap();
            assert_eq!(
                emit_layout_attempt(
                    &source,
                    optimize,
                    source.uses_runtime(),
                    false,
                    false,
                    false
                )
                .unwrap(),
                previous
            );
        }
    }
}

#[test]
fn decoder_or_body_that_exceeds_existing_limits_still_rejects() {
    // Earlier emitters still reject a duplicated decoder; only the final
    // sequenced retry admits it (see sequenced_retry_* below).
    let decoder = wide_decoder(5);
    for separate in [false, true] {
        size_error(&attempt(&decoder, false, separate).unwrap_err());
    }

    // A balanced tree reaches the existing four-million expansion budget
    // without deep native recursion or an oversized individual byte literal.
    let mut body = Expr::Bytes {
        hex: "42".repeat(BYTES),
    };
    for _ in 0..10 {
        body = Expr::If {
            condition: Box::new(Expr::Arg { index: 2 }),
            then_value: Box::new(body.clone()),
            else_value: Box::new(body),
        };
    }
    let mut oversized = fixture(Shape::Flat, 0, 1);
    oversized.result = Type::Bytes { len: BYTES };
    oversized.body = body;
    oversized.functions.push(Function {
        id: 0,
        name: "unused".into(),
        parameters: vec![],
        result: Type::Unit,
        body: Expr::Unit,
    });
    oversized.validate().unwrap();
    for separate in [false, true] {
        size_error(&attempt(&oversized, false, separate).unwrap_err());
    }
    size_error(&decoder_attempt(&oversized, false, Decoder::Sequenced).unwrap_err());
    size_error(&compile(&oversized, false).unwrap_err());

    let mut deep = fixture(Shape::Flat, 0, 1);
    deep.result = Type::Bool;
    deep.body = (0..100).fold(Expr::Arg { index: 2 }, |value, _| Expr::Not {
        value: Box::new(value),
    });
    deep.functions.push(Function {
        id: 0,
        name: "unused".into(),
        parameters: vec![],
        result: Type::Unit,
        body: Expr::Unit,
    });
    deep.validate().unwrap();
    for separate in [false, true] {
        let error = attempt(&deep, false, separate).unwrap_err();
        assert!(
            matches!(
                error.downcast_ref::<limits::Limit>(),
                Some(limits::Limit::ExpansionDepth)
            ),
            "{error:?}"
        );
    }
    let error = decoder_attempt(&deep, false, Decoder::Sequenced).unwrap_err();
    assert!(
        matches!(
            error.downcast_ref::<limits::Limit>(),
            Some(limits::Limit::ExpansionDepth)
        ),
        "{error:?}"
    );
    let error = compile(&deep, false).unwrap_err();
    assert!(
        matches!(
            error.downcast_ref::<limits::Limit>(),
            Some(limits::Limit::ExpansionDepth)
        ),
        "{error:?}"
    );
}

#[test]
fn retry_never_skips_validation_of_unused_invalid_metadata() {
    let mut source = fixture(Shape::Flat, PAYLOADS, BYTES);
    source.functions.push(Function {
        id: 0,
        name: "unused-invalid".into(),
        parameters: vec![],
        result: uint_type(),
        body: Expr::Arg { index: 99 },
    });
    for separate in [false, true] {
        let error = attempt(&source, false, separate).unwrap_err();
        assert!(error.to_string().contains("invalid argument 99"));
        assert!(!error.is::<limits::Limit>());
    }
}

// Scalar integers whose strict guards each duplicate their continuation.
fn wide_decoder(integers: usize) -> Program {
    let mut decoder = fixture(Shape::Flat, 0, 1);
    decoder.parameters = (0..integers)
        .map(|index| parameter(index, uint_type()))
        .collect();
    decoder.parameters.push(parameter(integers, Type::Bool));
    decoder.result = Type::Tuple {
        items: vec![uint_type(), Type::Bool],
    };
    decoder.body = Expr::Tuple {
        items: vec![
            Expr::Arg {
                index: integers - 1,
            },
            Expr::Arg { index: integers },
        ],
    };
    decoder.validate().unwrap();
    decoder
}

fn wide_environment(values: &[Vec<u8>], flag: &[u8]) -> Vec<u8> {
    let mut items = values.to_vec();
    items.push(flag.to_vec());
    list(&items)
}

#[test]
fn sequenced_retry_admits_wide_integer_decoders_in_both_modes() {
    // Twelve integers still exceed the existing nesting limit; no bound moved.
    let error = compile(&wide_decoder(12), true).unwrap_err();
    assert!(
        matches!(
            error.downcast_ref::<limits::Limit>(),
            Some(limits::Limit::ExpansionDepth)
        ),
        "{error:?}"
    );
    for integers in [4, 5, 8, 10] {
        let source = wide_decoder(integers);
        for optimize in [false, true] {
            let code = compile(&source, optimize).unwrap();
            if !optimize {
                // These sizes exhaust both earlier emitters in this mode.
                size_error(&attempt(&source, optimize, false).unwrap_err());
                size_error(&attempt(&source, optimize, true).unwrap_err());
                assert_eq!(
                    code,
                    decoder_attempt(&source, optimize, Decoder::Sequenced).unwrap()
                );
            }
            let values = (0..integers as u64)
                .map(|index| [0, 127, 128, u64::MAX][index as usize % 4])
                .collect::<Vec<_>>();
            for flag in [false, true] {
                let input = wide_environment(
                    &values.iter().map(|v| uint(*v)).collect::<Vec<_>>(),
                    &uint(u64::from(flag)),
                );
                assert_eq!(
                    run(&code, &input, 10_000_000).unwrap(),
                    list(&[uint(values[integers - 1]), uint(u64::from(flag))])
                );
            }
            // Every position rejects padded, oversized and non-atom values
            // as a typed program failure, never a resource failure.
            for position in 0..integers {
                for bad in [
                    atom(&[0]),
                    atom(&[0, 1]),
                    atom(&[0xff]),
                    atom(&[1, 0, 0, 0, 0, 0, 0, 0, 0]),
                    list(&[uint(1)]),
                ] {
                    let mut fields = vec![uint(7); integers];
                    fields[position] = bad;
                    let input = wide_environment(&fields, &uint(1));
                    assert_eq!(run(&code, &input, 1_000_000), Err(FailureCode::Raise));
                }
            }
            let valid = vec![uint(7); integers];
            for bad_flag in [uint(2), atom(&[0]), list(&[uint(1)])] {
                let input = wide_environment(&valid, &bad_flag);
                assert_eq!(run(&code, &input, 1_000_000), Err(FailureCode::Raise));
            }
            let mut short = valid.clone();
            short.pop();
            for input in [
                list(&short),
                wide_environment(&[valid.clone(), vec![uint(1)]].concat(), &uint(1)),
                pair(&uint(7), &uint(7)),
                atom(&[]),
            ] {
                let failure = run(&code, &input, 1_000_000).unwrap_err();
                assert_eq!(failure.kind(), FailureKind::Program, "{failure:?}");
            }
        }
    }
}

// Three integers still fit the original inline emitter, so the sequenced
// emitter must reproduce its exact outputs and typed failure codes.
#[test]
fn sequenced_decoder_matches_inline_outcomes_where_both_fit() {
    let source = wide_decoder(3);
    let mut inputs = vec![
        atom(&[]),
        uint(1),
        pair(&uint(7), &uint(7)),
        list(&[uint(7), uint(7)]),
        list(&[uint(7), uint(7), uint(7)]),
        list(&[uint(7), uint(7), uint(7), uint(1), uint(1)]),
        pair(
            &uint(7),
            &pair(&uint(7), &pair(&uint(7), &pair(&uint(1), &uint(1)))),
        ),
    ];
    for position in 0..4 {
        for bad in [
            atom(&[]),
            atom(&[0]),
            atom(&[0, 1]),
            atom(&[0xff]),
            atom(&[0, 0x80]),
            atom(&[0xff, 0x7f]),
            atom(&[1, 0, 0, 0, 0, 0, 0, 0, 0]),
            atom(&[0, 0x80, 0, 0, 0, 0, 0, 0, 0, 0]),
            uint(2),
            list(&[uint(1)]),
            pair(&uint(1), &uint(1)),
        ] {
            let mut fields = vec![uint(127), uint(128), uint(u64::MAX), uint(1)];
            fields[position] = bad;
            inputs.push(list(&fields));
        }
    }
    for optimize in [false, true] {
        let inline = decoder_attempt(&source, optimize, Decoder::Inline).unwrap();
        let sequenced = decoder_attempt(&source, optimize, Decoder::Sequenced).unwrap();
        assert_ne!(inline, sequenced);
        for input in &inputs {
            assert_eq!(
                run(&sequenced, input, 1_000_000),
                run(&inline, input, 1_000_000),
                "optimize={optimize}"
            );
        }
    }
}

#[test]
fn sequenced_retry_keeps_guard_order_before_divergence_and_strict_initializers() {
    let mut diverging = wide_decoder(5);
    diverging.functions.push(Function {
        id: 0,
        name: "spin".into(),
        parameters: vec![],
        result: Type::Unit,
        body: Expr::Call {
            function_id: 0,
            arguments: vec![],
        },
    });
    diverging.body = Expr::Sequence {
        first: Box::new(Expr::Call {
            function_id: 0,
            arguments: vec![],
        }),
        then_value: Box::new(diverging.body),
    };
    let mut strict = wide_decoder(5);
    strict.body = Expr::Let {
        local_id: 0,
        ty: Type::Unit,
        value: Box::new(Expr::Reject {
            message: "unused initializer still rejects".into(),
        }),
        body: Box::new(strict.body),
    };
    let valid = wide_environment(&vec![uint(7); 5], &uint(1));
    let mut padded_last = vec![uint(7); 5];
    padded_last[4] = atom(&[0]);
    let padded_last = wide_environment(&padded_last, &uint(1));
    for optimize in [false, true] {
        let code = compile(&diverging, optimize).unwrap();
        assert_eq!(run(&code, &valid, 50_000), Err(FailureCode::CostExceeded));
        assert_eq!(run(&code, &padded_last, 50_000), Err(FailureCode::Raise));
        let code = compile(&strict, optimize).unwrap();
        assert_eq!(run(&code, &valid, 1_000_000), Err(FailureCode::Raise));
    }
}

#[test]
fn sequenced_nested_tuple_and_pair_decoders_keep_their_shapes() {
    for explicit_pair in [false, true] {
        let item = if explicit_pair {
            (0..5).rev().fold(uint_type(), |rest, _| Type::Pair {
                first: Box::new(uint_type()),
                rest: Box::new(rest),
            })
        } else {
            Type::Tuple {
                items: vec![uint_type(); 6],
            }
        };
        let source = Program {
            name: "nested-wide-decoder".into(),
            environment: Environment::Strict,
            parameters: vec![parameter(0, item.clone())],
            result: item,
            body: Expr::Arg { index: 0 },
            functions: vec![],
        };
        source.validate().unwrap();
        let fields = (0..6u64).map(uint).collect::<Vec<_>>();
        let value = if explicit_pair {
            fields[..5]
                .iter()
                .rev()
                .fold(fields[5].clone(), |rest, first| pair(first, &rest))
        } else {
            list(&fields)
        };
        let wrong = if explicit_pair {
            list(&fields)
        } else {
            fields[..5]
                .iter()
                .rev()
                .fold(fields[5].clone(), |rest, first| pair(first, &rest))
        };
        for optimize in [false, true] {
            let code = compile(&source, optimize).unwrap();
            assert_eq!(
                run(&code, &list(std::slice::from_ref(&value)), 10_000_000).unwrap(),
                value
            );
            assert_eq!(
                run(&code, &list(std::slice::from_ref(&wrong)), 1_000_000),
                Err(FailureCode::Raise)
            );
        }
    }
}
