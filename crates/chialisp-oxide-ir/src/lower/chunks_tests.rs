//! Executed continuation-table controls, not a compiler-preservation proof.
use super::*;
use crate::{Environment, Function, Parameter};
use chialisp_oxide_vm::{FailureCode, FailureKind, classify_failure};
use clvmr::{
    ChiaDialect, MEMPOOL_MODE, run_program,
    serde::{node_from_bytes, node_to_bytes},
};

fn run(
    code: &[u8],
    input: &[u8],
    cost: u64,
) -> std::result::Result<Vec<u8>, (FailureKind, FailureCode)> {
    let mut allocator = Allocator::new();
    let code = node_from_bytes(&mut allocator, code).unwrap();
    let input = node_from_bytes(&mut allocator, input).unwrap();
    match run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        code,
        input,
        cost,
    ) {
        Ok(result) => Ok(node_to_bytes(&allocator, result.1).unwrap()),
        Err(error) => {
            let failure = classify_failure(&error);
            Err((failure.kind(), failure.code()))
        }
    }
}

fn layered(body: Expr) -> Expr {
    (0..50).rev().fold(body, |body, index| Expr::Let {
        local_id: index + 1,
        ty: Type::Bool,
        value: Box::new(Expr::Not {
            value: Box::new(Expr::Local { local_id: index }),
        }),
        body: Box::new(body),
    })
}

fn fixture() -> Program {
    Program {
        name: "strict-continuation-chunks".into(),
        environment: Environment::Strict,
        parameters: vec![Parameter {
            name: "value".into(),
            ty: Type::Bool,
        }],
        result: Type::Bool,
        body: Expr::Let {
            local_id: 0,
            ty: Type::Bool,
            value: Box::new(Expr::Arg { index: 0 }),
            body: Box::new(layered(Expr::Local { local_id: 50 })),
        },
        functions: vec![Function {
            id: 0,
            name: "unused".into(),
            parameters: vec![],
            result: Type::Unit,
            body: Expr::Unit,
        }],
    }
}

#[test]
fn hoisted_continuations_keep_immutable_shadowed_generations_and_strict_abi() {
    let source = fixture();
    assert!(
        compile_existing(&source, false)
            .unwrap_err()
            .is::<limits::Limit>()
    );
    let code = compile(&source, false).unwrap();
    assert_eq!(run(&code, &[0xff, 1, 0x80], 10_000_000).unwrap(), [1]);
    assert_eq!(run(&code, &[0xff, 0x80, 0x80], 10_000_000).unwrap(), [0x80]);
    for input in [&[0x80][..], &[0xff, 2, 0x80], &[0xff, 1, 1]] {
        assert_eq!(
            run(&code, input, 10_000_000),
            Err((FailureKind::Program, FailureCode::Raise))
        );
    }
}

#[test]
fn retained_actual_source_diagnostics_now_emit_in_both_modes() {
    for project in ["continuation-batches", "spuds"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../experiments/{project}/evidence/diagnostics/rust-on.ir.json"
        ));
        let source: Program = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        for optimize in [false, true] {
            let result = compile(&source, optimize);
            assert!(result.is_ok(), "{project} optimize={optimize}: {result:?}");
        }
    }
}

#[test]
fn chunked_strict_failures_and_divergence_keep_source_order() {
    let bad = Expr::TreeFirst {
        value: Box::new(Expr::Arg { index: 1 }),
    };
    for first_bad in [false, true] {
        let mut source = fixture();
        source.parameters.push(Parameter {
            name: "tree".into(),
            ty: Type::Tree,
        });
        let reject = Expr::Reject {
            message: "test strict failure".into(),
        };
        let body = if first_bad {
            Expr::Sequence {
                first: Box::new(bad.clone()),
                then_value: Box::new(reject),
            }
        } else {
            Expr::Sequence {
                first: Box::new(reject),
                then_value: Box::new(Expr::Sequence {
                    first: Box::new(bad.clone()),
                    then_value: Box::new(Expr::Bool { value: true }),
                }),
            }
        };
        if let Expr::Let { body: tail, .. } = &mut source.body {
            **tail = layered(body);
        }
        let code = compile(&source, false).unwrap();
        assert_eq!(
            run(&code, &[0xff, 1, 0xff, 0x80, 0x80], 10_000_000),
            Err((
                FailureKind::Program,
                if first_bad {
                    FailureCode::InvalidOpArg
                } else {
                    FailureCode::Raise
                }
            ))
        );
    }
    let mut source = fixture();
    source.functions[0] = Function {
        id: 0,
        name: "diverge".into(),
        parameters: vec![],
        result: Type::Unit,
        body: Expr::Call {
            function_id: 0,
            arguments: vec![],
        },
    };
    if let Expr::Let { body, .. } = &mut source.body {
        **body = layered(Expr::Sequence {
            first: Box::new(Expr::Call {
                function_id: 0,
                arguments: vec![],
            }),
            then_value: Box::new(Expr::Reject {
                message: "test strict failure".into(),
            }),
        });
    }
    let code = compile(&source, false).unwrap();
    assert_eq!(
        run(&code, &[0xff, 1, 0x80], 500_000),
        Err((FailureKind::Resource, FailureCode::CostExceeded))
    );
    assert_eq!(
        run(&code, &[0xff, 2, 0x80], 500_000),
        Err((FailureKind::Program, FailureCode::Raise))
    );
}

#[test]
fn returned_program_data_is_not_hoisted_as_executable_code() {
    let mut source = fixture();
    source.result = Type::Tree;
    if let Expr::Let { body, .. } = &mut source.body {
        **body = layered(Expr::TreePair {
            first: Box::new(Expr::TreeAtom {
                value: Box::new(Expr::Atom { hex: "01".into() }),
            }),
            rest: Box::new(Expr::TreeAtom {
                value: Box::new(Expr::Atom { hex: String::new() }),
            }),
        });
    }
    let code = compile(&source, false).unwrap();
    let produced = run(&code, &[0xff, 1, 0x80], 10_000_000).unwrap();
    assert_eq!(produced, [0xff, 1, 0x80]);
    assert_eq!(run(&produced, &[0x80], 10_000).unwrap(), [0x80]);
}

#[test]
fn complete_validation_and_successful_prior_artifacts_are_preserved() {
    let mut invalid = fixture();
    invalid.functions[0].body = Expr::Arg { index: 99 };
    for optimize in [false, true] {
        let error = compile(&invalid, optimize).unwrap_err();
        assert!(!error.is::<limits::Limit>());
        assert!(error.to_string().contains("invalid argument 99"));
        let chunk_error = emit_decoder_with_chunks(
            &invalid,
            optimize,
            true,
            false,
            false,
            false,
            Decoder::Inline,
            true,
        )
        .unwrap_err();
        assert!(!chunk_error.is::<limits::Limit>());
        assert!(chunk_error.to_string().contains("invalid argument 99"));
        let mut small = fixture();
        small.body = Expr::Arg { index: 0 };
        assert_eq!(
            compile(&small, optimize).unwrap(),
            compile_unchunked(&small, optimize).unwrap()
        );
    }
}

#[test]
fn code_hoisting_never_retries_an_intrinsically_deep_decoder() {
    let mut source = fixture();
    source.parameters[0].ty = (0..31).fold(Type::Bool, |ty, _| Type::Tuple { items: vec![ty] });
    source.parameters = vec![source.parameters[0].clone(); 8];
    if let Expr::Let { value, .. } = &mut source.body {
        **value = Expr::Bool { value: true };
    }
    source.validate().unwrap();
    for optimize in [false, true] {
        let error = compile(&source, optimize).unwrap_err();
        assert!(matches!(
            error.downcast_ref::<limits::Limit>(),
            Some(limits::Limit::ExpansionDepth)
        ));
    }
}

#[test]
fn heap_table_does_not_evade_original_expansion_size_budget() {
    let mut source = fixture();
    source.parameters = vec![Parameter {
        name: "bytes".into(),
        ty: Type::Bytes { len: 32 },
    }];
    source.result = Type::Unit;
    source.body = Expr::Unit;
    let result = Type::Tuple {
        items: vec![Type::Bytes { len: 32 }; 50],
    };
    source.functions = (0..64)
        .map(|id| Function {
            id,
            name: format!("wide_{id}"),
            parameters: source.parameters.clone(),
            result: result.clone(),
            body: Expr::Tuple {
                items: vec![Expr::Arg { index: 0 }; 50],
            },
        })
        .collect();
    source.validate().unwrap();
    let chunked = emit_decoder_with_chunks(
        &source,
        false,
        true,
        false,
        false,
        false,
        Decoder::Inline,
        true,
    );
    for result in [
        compile(&source, false),
        finish_decoder_retry(&source, false, chunked, true),
    ] {
        let error = result.unwrap_err();
        assert!(
            matches!(
                error.downcast_ref::<limits::Limit>(),
                Some(limits::Limit::ExpansionSize)
            ),
            "{error:?}"
        );
    }
}

#[test]
fn shared_heap_function_calls_have_fresh_argument_and_local_scopes() {
    let mut source = fixture();
    source.functions = (0..9)
        .map(|id| Function {
            id,
            name: format!("function_{id}"),
            parameters: vec![Parameter {
                name: "value".into(),
                ty: Type::Bool,
            }],
            result: Type::Bool,
            body: Expr::Let {
                local_id: 0,
                ty: Type::Bool,
                value: Box::new(Expr::Not {
                    value: Box::new(Expr::Arg { index: 0 }),
                }),
                body: Box::new(if id == 0 {
                    Expr::Local { local_id: 0 }
                } else {
                    Expr::Call {
                        function_id: id - 1,
                        arguments: vec![Expr::Local { local_id: 0 }],
                    }
                }),
            },
        })
        .collect();
    if let Expr::Let { body, .. } = &mut source.body {
        **body = layered(Expr::Call {
            function_id: 8,
            arguments: vec![Expr::Local { local_id: 50 }],
        });
    }
    let code = compile(&source, false).unwrap();
    assert_eq!(run(&code, &[0xff, 1, 0x80], 10_000_000).unwrap(), [0x80]);
    assert_eq!(run(&code, &[0xff, 0x80, 0x80], 10_000_000).unwrap(), [1]);
}

#[test]
fn strict_sha_chain_chunks_compare_with_independent_chialisp_in_both_modes() {
    let ty = Type::Bytes { len: 32 };
    let mut body = Expr::Local { local_id: 60 };
    for id in (1..=60).rev() {
        body = Expr::Let {
            local_id: id,
            ty: ty.clone(),
            value: Box::new(Expr::Sha256 {
                parts: vec![if id == 1 {
                    Expr::Arg { index: 0 }
                } else {
                    Expr::Local { local_id: id - 1 }
                }],
            }),
            body: Box::new(body),
        };
    }
    let source = Program {
        name: "strict-sha-chunks".into(),
        environment: Environment::Strict,
        parameters: vec![Parameter {
            name: "bytes".into(),
            ty: ty.clone(),
        }],
        result: ty,
        body,
        functions: vec![],
    };
    let mut reference = "X".to_owned();
    for _ in 0..60 {
        reference = format!("(sha256 {reference})");
    }
    let reference = chialisp_oxide_reference::compile(&format!("(mod (X) {reference})")).unwrap();
    let mut input = vec![0xff, 0xa0];
    input.extend_from_slice(&[0x31; 32]);
    input.push(0x80);
    let expected = run(&reference, &input, 10_000_000).unwrap();
    for optimize in [false, true] {
        assert!(matches!(
            compile_unchunked(&source, optimize)
                .unwrap_err()
                .downcast_ref::<limits::Limit>(),
            Some(limits::Limit::ExpansionDepth)
        ));
        let code = compile(&source, optimize).unwrap();
        assert_eq!(run(&code, &input, 10_000_000).unwrap(), expected);
    }
}
