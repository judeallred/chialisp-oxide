//! Executed byte-order regressions. These finite native/VM comparisons do not
//! prove compiler preservation, runtime termination or arbitrary-input safety.
#[path = "runtime_evaluation/support.rs"]
#[allow(
    dead_code,
    reason = "Shared fixtures also expose unrelated operations."
)]
mod support;

use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Function, Type, compile, optimize};
use chialisp_oxide_vm::{FailureCode, FailureKind};
use support::*;

const ORDER: [BinaryOp; 4] = [BinaryOp::Lt, BinaryOp::Le, BinaryOp::Gt, BinaryOp::Ge];
fn compare(kind: BinaryOp, ty: Type, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        kind,
        ty,
        left: Box::new(left),
        right: Box::new(right),
    }
}
fn bytes(hex: &str) -> Expr {
    Expr::Atom { hex: hex.into() }
}
fn ordering(kind: BinaryOp, left: Expr, right: Expr) -> Expr {
    compare(kind, Type::Atom, left, right)
}
fn expected<T: PartialOrd>(kind: BinaryOp, left: T, right: T) -> Vec<u8> {
    let result = match kind {
        BinaryOp::Lt => left < right,
        BinaryOp::Le => left <= right,
        BinaryOp::Gt => left > right,
        BinaryOp::Ge => left >= right,
        _ => panic!("ordering fixture requires an ordering operation"),
    };
    if result { vec![1] } else { vec![0x80] }
}
fn functions(runtime: bool) -> Vec<Function> {
    if runtime {
        vec![function(0, vec![], Type::Unit, Expr::Unit)]
    } else {
        vec![]
    }
}
fn wire_atom(data: &[u8]) -> Vec<u8> {
    let len = data.len();
    let mut result = match data {
        [] => return vec![0x80],
        [byte] if *byte < 0x80 => return vec![*byte],
        _ if len < 64 => vec![0x80 | len as u8],
        _ if len < 8192 => vec![0xc0 | (len >> 8) as u8, len as u8],
        _ if len < 1_048_576 => vec![0xe0 | (len >> 16) as u8, (len >> 8) as u8, len as u8],
        _ => panic!("outside the independent finite wire fixture"),
    };
    result.extend_from_slice(data);
    result
}
fn atom_value(value: Expr) -> Expr {
    Expr::TreeAtomValue {
        value: Box::new(value),
    }
}
fn rejected_atom() -> Expr {
    sequence(
        Expr::Reject {
            message: "required byte operand".into(),
        },
        bytes(""),
    )
}

#[test]
fn raw_byte_order_matches_native_unsigned_lexicographic_order_not_integer_order() {
    let small = [
        "", "00", "0000", "01", "0001", "0100", "02", "7f", "80", "ff", "ff00",
    ];
    let mut cases = Vec::new();
    for a in small {
        for b in small {
            cases.push((hex::decode(a).unwrap(), hex::decode(b).unwrap()));
        }
    }
    for len in [63, 64, 255, 256, 4096, 4097, 8192] {
        let a = (0..len).map(|i| (i * 17 + 128) as u8).collect::<Vec<_>>();
        cases.push((a.clone(), a.clone()));
        let mut first = a.clone();
        first[0] ^= 1;
        let mut last = a.clone();
        last[len - 1] ^= 1;
        cases.push((a.clone(), first));
        cases.push((a.clone(), last));
        cases.push((a.clone(), a[..len - 1].to_vec()));
    }
    for runtime in [false, true] {
        for kind in ORDER {
            let p = program(
                Environment::Strict,
                vec![Type::Atom, Type::Atom],
                Type::Bool,
                ordering(kind, arg(0), arg(1)),
                functions(runtime),
            );
            let json = serde_json::to_value(&p).unwrap();
            assert_eq!(json.get("ir_version").is_some(), runtime);
            let decoded = serde_json::from_value(json).unwrap();
            for (_, code) in modes(&decoded) {
                for (a, b) in &cases {
                    accepted(
                        run(&code, &list(&[wire_atom(a), wire_atom(b)]), FINITE_LIMIT),
                        &expected(kind, a.as_slice(), b.as_slice()),
                    );
                }
                failed(
                    run(&code, &list(&[wire_atom(&vec![0; 8192]), atom(&[])]), 1),
                    FailureCode::CostExceeded,
                );
            }
        }
    }
    // Pin cases distinguishing prefix/unsigned-byte rules from numeric,
    // signed-byte, length-first and padded-integer interpretations.
    for (a, b) in [
        ("", "00"),
        ("00", "0000"),
        ("0001", "01"),
        ("7f", "80"),
        ("80", "ff"),
        ("0100", "02"),
    ] {
        assert_eq!(
            expected(
                BinaryOp::Lt,
                hex::decode(a).unwrap(),
                hex::decode(b).unwrap()
            ),
            vec![1]
        );
    }
}

#[test]
fn fixed_byte_order_retains_exact_width_and_strict_environment_decoding() {
    for runtime in [false, true] {
        for kind in ORDER {
            for len in [0, 1, 2, 32, 256, 4096] {
                let ty = Type::Bytes { len };
                let p = program(
                    Environment::Strict,
                    vec![ty.clone(), ty.clone()],
                    Type::Bool,
                    compare(kind, ty, arg(0), arg(1)),
                    functions(runtime),
                );
                let a = vec![0x80; len];
                let mut changed = a.clone();
                if let Some(last) = changed.last_mut() {
                    *last ^= 1;
                }
                for (_, code) in modes(&p) {
                    for b in [&a, &changed] {
                        accepted(
                            run(&code, &list(&[wire_atom(&a), wire_atom(b)]), FINITE_LIMIT),
                            &expected(kind, a.as_slice(), b.as_slice()),
                        );
                    }
                    for malformed in [
                        atom(&[]),
                        list(&[wire_atom(&a)]),
                        list(&[wire_atom(&a), wire_atom(&a), atom(&[])]),
                        pair(&wire_atom(&a), &pair(&wire_atom(&a), &atom(&[0]))),
                        list(&[pair(&uint(1), &uint(2)), wire_atom(&a)]),
                        list(&[wire_atom(&a), wire_atom(&vec![0x80; len + 1])]),
                    ] {
                        abi_rejected(run(&code, &malformed, FINITE_LIMIT));
                    }
                }
            }
            let atoms = program(
                Environment::Strict,
                vec![Type::Atom, Type::Atom],
                Type::Bool,
                ordering(kind, arg(0), arg(1)),
                functions(runtime),
            );
            for (_, code) in modes(&atoms) {
                for malformed in [
                    atom(&[]),
                    list(&[atom(&[])]),
                    list(&[atom(&[]), atom(&[]), atom(&[])]),
                    pair(&atom(&[]), &pair(&atom(&[]), &atom(&[0]))),
                    list(&[pair(&uint(1), &uint(2)), atom(&[])]),
                ] {
                    abi_rejected(run(&code, &malformed, LIMIT));
                }
            }
        }
    }
    assert_eq!(FailureCode::CostExceeded.kind(), FailureKind::Resource);
}

#[test]
fn signed_and_unsigned_integer_order_stays_numeric_and_other_types_stay_rejected() {
    for signed in [false, true] {
        let ty = Type::Int { bits: 8, signed };
        let numbers: Vec<(i16, Vec<u8>)> = if signed {
            vec![
                (-128, atom(&[0x80])),
                (-1, atom(&[0xff])),
                (0, atom(&[])),
                (1, atom(&[1])),
                (127, atom(&[127])),
            ]
        } else {
            [0, 1, 127, 128, 255].map(|x| (x as i16, uint(x))).to_vec()
        };
        for runtime in [false, true] {
            for kind in ORDER {
                let p = program(
                    Environment::Strict,
                    vec![ty.clone(), ty.clone()],
                    Type::Bool,
                    compare(kind, ty.clone(), arg(0), arg(1)),
                    functions(runtime),
                );
                for (_, code) in modes(&p) {
                    for (a, aw) in &numbers {
                        for (b, bw) in &numbers {
                            accepted(
                                run(&code, &list(&[aw.clone(), bw.clone()]), LIMIT),
                                &expected(kind, a, b),
                            );
                        }
                    }
                    for padded in [atom(&[0]), atom(&[0, 1]), atom(&[0xff, 0xff])] {
                        failed(
                            run(&code, &list(&[padded, uint(1)]), LIMIT),
                            FailureCode::Raise,
                        );
                    }
                }
            }
        }
    }
    let mut invalid = Vec::new();
    for kind in ORDER {
        for (ty, value) in [
            (Type::Bool, Expr::Bool { value: false }),
            (Type::Unit, Expr::Unit),
            (Type::Tree, tree_atom("")),
            (Type::Tuple { items: vec![] }, Expr::Tuple { items: vec![] }),
        ] {
            invalid.push(compare(kind, ty, value.clone(), value));
        }
        invalid.push(ordering(kind, bytes(""), Expr::Bytes { hex: "".into() }));
        invalid.push(compare(
            kind,
            Type::Bytes { len: 1 },
            Expr::Bytes { hex: "00".into() },
            Expr::Bytes { hex: "0000".into() },
        ));
    }
    for kind in [BinaryOp::Add, BinaryOp::Sub, BinaryOp::Mul] {
        invalid.push(ordering(kind, bytes("00"), bytes("01")));
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
            "invalid comparisons in dead code must reject"
        );
        for optimize in [false, true] {
            assert!(compile(&p, optimize).is_err());
        }
    }
}

#[test]
fn comparison_operands_and_unused_results_keep_rejection_overflow_and_laziness() {
    for kind in ORDER {
        for runtime in [false, true] {
            let overflow = sequence(add(int(255), int(1)), bytes(""));
            for body in [
                ordering(kind, rejected_atom(), bytes("")),
                ordering(kind, bytes(""), rejected_atom()),
                ordering(kind, overflow.clone(), bytes("")),
                ordering(kind, bytes(""), overflow),
            ] {
                let p = program(
                    Environment::Ignored,
                    vec![],
                    Type::Bool,
                    sequence(body, Expr::Bool { value: true }),
                    functions(runtime),
                );
                for (_, code) in modes(&p) {
                    failed(run(&code, &atom(&[]), LIMIT), FailureCode::Raise);
                }
            }
            let p = program(
                Environment::Raw,
                vec![Type::Tree],
                Type::Bool,
                ordering(kind, atom_value(arg(0)), bytes("0001")),
                functions(runtime),
            );
            for (_, code) in modes(&p) {
                failed(
                    run(&code, &pair(&uint(1), &uint(2)), LIMIT),
                    FailureCode::Raise,
                );
                accepted(
                    run(&code, &atom(&[0, 1]), LIMIT),
                    &expected(kind, &[0, 1][..], &[0, 1][..]),
                );
            }
        }
        let table = vec![
            spin(&Type::Atom),
            function(1, vec![], Type::Atom, rejected_atom()),
        ];
        for (left, right, wanted) in [
            (call(0, vec![]), call(1, vec![]), FailureCode::CostExceeded),
            (call(1, vec![]), call(0, vec![]), FailureCode::Raise),
        ] {
            let comparison = ordering(kind, left, right);
            for body in [
                comparison.clone(),
                binding(0, Type::Bool, comparison, Expr::Bool { value: true }),
            ] {
                let p = program(
                    Environment::Ignored,
                    vec![],
                    Type::Bool,
                    body,
                    table.clone(),
                );
                for (_, code) in modes(&p) {
                    failed(run(&code, &atom(&[]), LIMIT), wanted);
                }
            }
        }
        let p = program(
            Environment::Ignored,
            vec![],
            Type::Bool,
            branch(
                Expr::Bool { value: false },
                ordering(kind, call(0, vec![]), call(1, vec![])),
                Expr::Bool { value: true },
            ),
            table,
        );
        for (_, code) in modes(&p) {
            accepted(run(&code, &atom(&[]), LIMIT), &[1]);
        }
    }
}

#[test]
fn bound_call_results_are_compared_once_with_fresh_function_scopes() {
    let walk = function(
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
    for kind in ORDER {
        let once = program(
            Environment::Raw,
            vec![Type::Tree],
            Type::Bool,
            binding(
                0,
                Type::Atom,
                call(0, vec![arg(0)]),
                ordering(kind, local(0), local(0)),
            ),
            vec![walk.clone()],
        );
        let twice = program(
            Environment::Raw,
            vec![Type::Tree],
            Type::Bool,
            ordering(kind, call(0, vec![arg(0)]), call(0, vec![arg(0)])),
            vec![walk.clone()],
        );
        for optimize in [false, true] {
            let one = compile(&once, optimize).unwrap();
            let two = compile(&twice, optimize).unwrap();
            let wanted = expected(kind, &[0, 0, 1][..], &[0, 0, 1][..]);
            let one_cost = accepted(run(&one, &environment, FINITE_LIMIT), &wanted);
            let two_cost = accepted(run(&two, &environment, FINITE_LIMIT), &wanted);
            assert!(
                two_cost > one_cost,
                "a deliberately repeated traversal must cost more"
            );
            let budget = one_cost + (two_cost - one_cost) / 2;
            accepted(run(&one, &environment, budget), &wanted);
            failed(run(&two, &environment, budget), FailureCode::CostExceeded);
        }
    }
}

#[test]
fn existing_finite_pair_guard_rewrite_remains_safe_with_byte_ordering() {
    for kind in ORDER {
        let p = program(
            Environment::Raw,
            vec![Type::Tree],
            Type::Tree,
            branch(
                is_pair(arg(0)),
                branch(
                    ordering(kind, atom_value(first(arg(0))), bytes("80")),
                    first(arg(0)),
                    rest(arg(0)),
                ),
                Expr::Reject {
                    message: "reject atoms".into(),
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
                for head in [
                    &[][..],
                    &[0][..],
                    &[0x7f][..],
                    &[0x80][..],
                    &[0xff][..],
                    &[0x80, 0][..],
                ] {
                    let tail = &[0, 1];
                    let wanted = if expected(kind, head, &[0x80][..]) == [1] {
                        head
                    } else {
                        tail
                    };
                    accepted(
                        run(&code, &pair(&atom(head), &atom(tail)), LIMIT),
                        &atom(wanted),
                    );
                }
                for bad in [atom(&[0]), pair(&pair(&atom(&[]), &atom(&[])), &atom(&[]))] {
                    abi_rejected(run(&code, &bad, LIMIT));
                }
            }
        }
        let mut runtime = p;
        runtime.functions = functions(true);
        assert_eq!(
            serde_json::to_value(optimize(&runtime).unwrap()).unwrap(),
            serde_json::to_value(runtime).unwrap()
        );
    }
}
