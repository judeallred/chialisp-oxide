//! Byte equality uses exact contents, not numeric or pointer identity. These
//! finite VM executions do not prove compiler preservation or termination.
#[path = "runtime_evaluation/support.rs"]
#[allow(
    dead_code,
    reason = "The shared runtime fixture also exposes non-byte operations."
)]
mod support;

use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Type, compile, optimize};
use chialisp_oxide_vm::{FailureCode, FailureKind};
use support::*;

fn bytes(hex: &str) -> Expr {
    Expr::Atom { hex: hex.into() }
}
fn compare(kind: BinaryOp, ty: Type, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        kind,
        ty,
        left: Box::new(left),
        right: Box::new(right),
    }
}
fn equality(kind: BinaryOp, left: Expr, right: Expr) -> Expr {
    compare(kind, Type::Atom, left, right)
}
fn atom_value(value: Expr) -> Expr {
    Expr::TreeAtomValue {
        value: Box::new(value),
    }
}
fn rejected_atom() -> Expr {
    sequence(
        Expr::Reject {
            message: "required atom operand".into(),
        },
        bytes(""),
    )
}
fn runtime_marker() -> Vec<chialisp_oxide_ir::Function> {
    vec![function(0, vec![], Type::Unit, Expr::Unit)]
}
fn wire_atom(data: &[u8]) -> Vec<u8> {
    // Independent serialized-input oracle, including the 8192-byte boundary.
    let len = data.len();
    let mut result = match data {
        [] => return vec![0x80],
        [byte] if *byte < 0x80 => return vec![*byte],
        _ if len < 64 => vec![0x80 | len as u8],
        _ if len < 8192 => vec![0xc0 | (len >> 8) as u8, len as u8],
        _ if len < 1_048_576 => vec![0xe0 | (len >> 16) as u8, (len >> 8) as u8, len as u8],
        _ => panic!("outside this test's finite atom grammar"),
    };
    result.extend_from_slice(data);
    result
}
fn boolean(value: bool) -> Vec<u8> {
    if value { vec![1] } else { vec![0x80] }
}

#[test]
fn byte_equality_matches_native_slices_across_lengths_and_both_lowering_paths() {
    let small = [
        vec![],
        vec![0],
        vec![0, 0],
        vec![1],
        vec![0, 1],
        vec![0x80],
        vec![0xff],
    ];
    let mut cases = Vec::new();
    for a in &small {
        for b in &small {
            cases.push((a.clone(), b.clone()));
        }
    }
    for len in [63, 64, 255, 256, 4096, 4097, 8192] {
        let a = (0..len).map(|i| (i * 17 + 128) as u8).collect::<Vec<_>>();
        cases.push((a.clone(), a.clone()));
        let mut changed_first = a.clone();
        changed_first[0] ^= 1;
        cases.push((a.clone(), changed_first));
        let mut changed_last = a.clone();
        changed_last[len - 1] ^= 1;
        cases.push((a.clone(), changed_last));
        cases.push((a.clone(), a[..len - 1].to_vec()));
    }
    for runtime in [false, true] {
        for kind in [BinaryOp::Eq, BinaryOp::Ne] {
            let p = program(
                Environment::Strict,
                vec![Type::Atom, Type::Atom],
                Type::Bool,
                equality(kind, arg(0), arg(1)),
                if runtime { runtime_marker() } else { vec![] },
            );
            let wire = serde_json::to_value(&p).unwrap();
            if runtime {
                assert_eq!(wire["ir_version"], 4);
            } else {
                assert!(wire.get("ir_version").is_none());
            }
            let decoded = serde_json::from_value(wire).unwrap();
            for (_, code) in modes(&decoded) {
                for (a, b) in &cases {
                    let wanted = if kind == BinaryOp::Eq { a == b } else { a != b };
                    let environment = list(&[wire_atom(a), wire_atom(b)]);
                    accepted(run(&code, &environment, FINITE_LIMIT), &boolean(wanted));
                }
                for malformed in [
                    atom(&[]),
                    list(&[atom(&[])]),
                    list(&[atom(&[]), atom(&[]), uint(1)]),
                    pair(&atom(&[]), &pair(&atom(&[]), &atom(&[0]))),
                    list(&[pair(&uint(1), &uint(2)), atom(&[])]),
                ] {
                    abi_rejected(run(&code, &malformed, FINITE_LIMIT));
                }
                failed(
                    run(&code, &list(&[wire_atom(&vec![0; 8192]), atom(&[])]), 1),
                    FailureCode::CostExceeded,
                );
                assert_eq!(FailureCode::CostExceeded.kind(), FailureKind::Resource);
            }
        }
    }
}

#[test]
fn fixed_bytes_keep_exact_width_checks_and_nonbyte_comparisons_stay_rejected() {
    for len in [0, 1, 32, 256, 4096] {
        for kind in [BinaryOp::Eq, BinaryOp::Ne] {
            let ty = Type::Bytes { len };
            let p = program(
                Environment::Strict,
                vec![ty.clone(), ty.clone()],
                Type::Bool,
                compare(kind, ty, arg(0), arg(1)),
                vec![],
            );
            for (_, code) in modes(&p) {
                let a = vec![0x80; len];
                let mut b = a.clone();
                if let Some(last) = b.last_mut() {
                    *last ^= 1;
                }
                for b in [&a, &b] {
                    let wanted = if kind == BinaryOp::Eq {
                        a == *b
                    } else {
                        a != *b
                    };
                    accepted(
                        run(&code, &list(&[wire_atom(&a), wire_atom(b)]), FINITE_LIMIT),
                        &boolean(wanted),
                    );
                }
                abi_rejected(run(
                    &code,
                    &list(&[wire_atom(&a), wire_atom(&vec![0x80; len + 1])]),
                    FINITE_LIMIT,
                ));
            }
        }
    }
    let mut invalid = Vec::new();
    for kind in [BinaryOp::Add, BinaryOp::Sub, BinaryOp::Mul] {
        invalid.push(equality(kind, bytes(""), bytes("00")));
    }
    invalid.push(equality(
        BinaryOp::Eq,
        bytes(""),
        Expr::Bytes { hex: "".into() },
    ));
    invalid.push(compare(
        BinaryOp::Eq,
        Type::Bytes { len: 1 },
        Expr::Bytes { hex: "00".into() },
        Expr::Bytes { hex: "0000".into() },
    ));
    for (ty, value) in [
        (Type::Unit, Expr::Unit),
        (Type::Tree, tree_atom("")),
        (Type::Tuple { items: vec![] }, Expr::Tuple { items: vec![] }),
    ] {
        invalid.push(compare(BinaryOp::Eq, ty, value.clone(), value));
    }
    for body in invalid {
        let p = program(
            Environment::Ignored,
            vec![],
            Type::Bool,
            branch(Expr::Bool { value: true }, Expr::Bool { value: true }, body),
            vec![],
        );
        assert!(
            p.validate().is_err(),
            "invalid comparisons must fail even in dead branches"
        );
        for mode in [false, true] {
            assert!(compile(&p, mode).is_err());
        }
    }
}

#[test]
fn operands_remain_strict_checked_and_lazy_when_the_result_is_unused() {
    for kind in [BinaryOp::Eq, BinaryOp::Ne] {
        for runtime in [false, true] {
            for body in [
                equality(kind, rejected_atom(), bytes("")),
                equality(kind, bytes(""), rejected_atom()),
            ] {
                let p = program(
                    Environment::Ignored,
                    vec![],
                    Type::Bool,
                    sequence(body, Expr::Bool { value: true }),
                    if runtime { runtime_marker() } else { vec![] },
                );
                for (_, code) in modes(&p) {
                    failed(run(&code, &atom(&[]), LIMIT), FailureCode::Raise);
                }
            }
            let p = program(
                Environment::Raw,
                vec![Type::Tree],
                Type::Bool,
                equality(kind, atom_value(arg(0)), bytes("0001")),
                if runtime { runtime_marker() } else { vec![] },
            );
            for (_, code) in modes(&p) {
                failed(
                    run(&code, &pair(&uint(1), &uint(2)), LIMIT),
                    FailureCode::Raise,
                );
                accepted(
                    run(&code, &atom(&[0, 1]), LIMIT),
                    &boolean(kind == BinaryOp::Eq),
                );
                accepted(
                    run(&code, &atom(&[1]), LIMIT),
                    &boolean(kind == BinaryOp::Ne),
                );
            }
        }
        let functions = vec![
            spin(&Type::Atom),
            function(1, vec![], Type::Atom, rejected_atom()),
        ];
        for (left, right, expected) in [
            (call(0, vec![]), call(1, vec![]), FailureCode::CostExceeded),
            (call(1, vec![]), call(0, vec![]), FailureCode::Raise),
        ] {
            let comparison = equality(kind, left, right);
            for body in [
                comparison.clone(),
                binding(0, Type::Bool, comparison, Expr::Bool { value: true }),
            ] {
                let p = program(
                    Environment::Ignored,
                    vec![],
                    Type::Bool,
                    body,
                    functions.clone(),
                );
                for (_, code) in modes(&p) {
                    failed(run(&code, &atom(&[]), LIMIT), expected);
                }
            }
        }
        let p = program(
            Environment::Ignored,
            vec![],
            Type::Bool,
            branch(
                Expr::Bool { value: false },
                equality(kind, call(0, vec![]), call(1, vec![])),
                Expr::Bool { value: true },
            ),
            functions,
        );
        for (_, code) in modes(&p) {
            accepted(run(&code, &atom(&[]), LIMIT), &[1]);
        }
    }
}

#[test]
fn a_reused_call_result_is_evaluated_once_rather_than_once_per_equality_operand() {
    let traverse = function(
        0,
        vec![Type::Tree],
        Type::Atom,
        branch(
            is_pair(arg(0)),
            call(0, vec![rest(arg(0))]),
            atom_value(arg(0)),
        ),
    );
    let environment = right_spine(128, &atom(&[0, 0, 1]));
    for kind in [BinaryOp::Eq, BinaryOp::Ne] {
        let once = program(
            Environment::Raw,
            vec![Type::Tree],
            Type::Bool,
            binding(
                0,
                Type::Atom,
                call(0, vec![arg(0)]),
                equality(kind, local(0), local(0)),
            ),
            vec![traverse.clone()],
        );
        let twice = program(
            Environment::Raw,
            vec![Type::Tree],
            Type::Bool,
            equality(kind, call(0, vec![arg(0)]), call(0, vec![arg(0)])),
            vec![traverse.clone()],
        );
        for mode in [false, true] {
            let one_code = compile(&once, mode).unwrap();
            let two_code = compile(&twice, mode).unwrap();
            let wanted = boolean(kind == BinaryOp::Eq);
            let one_cost = accepted(run(&one_code, &environment, FINITE_LIMIT), &wanted);
            let two_cost = accepted(run(&two_code, &environment, FINITE_LIMIT), &wanted);
            assert!(
                two_cost > one_cost,
                "deliberately duplicated traversal must cost more"
            );
            let budget = one_cost + (two_cost - one_cost) / 2;
            accepted(run(&one_code, &environment, budget), &wanted);
            failed(
                run(&two_code, &environment, budget),
                FailureCode::CostExceeded,
            );
        }
    }
}

#[test]
fn equality_child_facts_allow_only_the_existing_finite_pair_guard_rewrite() {
    let p = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Tree,
        branch(
            is_pair(arg(0)),
            branch(
                equality(BinaryOp::Eq, atom_value(first(arg(0))), bytes("00")),
                first(arg(0)),
                rest(arg(0)),
            ),
            Expr::Reject {
                message: "reject non-pairs".into(),
            },
        ),
        vec![],
    );
    let after = optimize(&p).unwrap();
    assert_ne!(
        serde_json::to_value(&p.body).unwrap(),
        serde_json::to_value(&after.body).unwrap()
    );
    for p in [&p, &after] {
        for (_, code) in modes(p) {
            for (head, tail) in [
                (&[0][..], &[0, 1][..]),
                (&[0, 0][..], &[1][..]),
                (&[][..], &[0][..]),
            ] {
                let environment = pair(&atom(head), &atom(tail));
                let wanted = atom(if head == [0] { head } else { tail });
                accepted(run(&code, &environment, LIMIT), &wanted);
            }
            for bad in [atom(&[0]), pair(&pair(&atom(&[]), &atom(&[])), &atom(&[]))] {
                abi_rejected(run(&code, &bad, LIMIT));
            }
        }
    }
    let mut runtime = p;
    runtime.functions = runtime_marker();
    assert_eq!(
        serde_json::to_value(optimize(&runtime).unwrap()).unwrap(),
        serde_json::to_value(runtime).unwrap()
    );
}
