//! Finite interval controls over constructed IR, not compiler preservation.
#[path = "runtime_evaluation/support.rs"]
#[allow(dead_code, reason = "Shared independent VM fixtures.")]
mod support;
use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Program, Type, optimize};
use chialisp_oxide_vm::FailureCode;
use num_bigint::BigInt;
use support::*;

fn integer(bits: u16, signed: bool) -> Type {
    Type::Int { bits, signed }
}
fn lit(value: impl ToString, ty: &Type) -> Expr {
    Expr::Int {
        value: value.to_string(),
        ty: ty.clone(),
    }
}
fn bin(kind: BinaryOp, left: Expr, right: Expr, ty: &Type) -> Expr {
    Expr::Binary {
        kind,
        left: Box::new(left),
        right: Box::new(right),
        ty: ty.clone(),
    }
}
fn explicit_reject() -> Expr {
    Expr::Reject {
        message: "policy guard".into(),
    }
}
fn repeat(value: Expr, result: Expr) -> Expr {
    sequence(
        Expr::Tuple {
            items: vec![value, Expr::Bool { value: false }],
        },
        result,
    )
}
fn guarded(condition: Expr, value: Expr) -> Expr {
    branch(
        bin(
            BinaryOp::Eq,
            condition,
            Expr::Bool { value: false },
            &Type::Bool,
        ),
        explicit_reject(),
        value,
    )
}
fn increment(ty: &Type, threshold: &str) -> Program {
    let op = bin(BinaryOp::Add, arg(0), lit(1, ty), ty);
    program(
        Environment::Strict,
        vec![ty.clone()],
        ty.clone(),
        guarded(
            bin(BinaryOp::Lt, arg(0), lit(threshold, ty), ty),
            repeat(op.clone(), op),
        ),
        vec![],
    )
}
fn count(expr: &Expr, op: &str) -> usize {
    fn visit(v: &serde_json::Value, op: &str) -> usize {
        match v {
            serde_json::Value::Object(m) => {
                usize::from(m.get("op").and_then(|v| v.as_str()) == Some(op))
                    + m.values().map(|v| visit(v, op)).sum::<usize>()
            }
            serde_json::Value::Array(a) => a.iter().map(|v| visit(v, op)).sum(),
            _ => 0,
        }
    }
    visit(&serde_json::to_value(expr).unwrap(), op)
}
fn number(value: impl Into<BigInt>) -> Vec<u8> {
    let value = value.into();
    atom(&if value == BigInt::from(0) {
        vec![]
    } else {
        value.to_signed_bytes_be()
    })
}

#[test]
fn successful_guard_removes_only_the_discarded_checked_initializer() {
    let p = increment(&u8_type(), "100");
    let reduced = optimize(&p).unwrap();
    assert_eq!(
        count(&reduced.body, "sequence"),
        0,
        "branch bound must prove the initializer total"
    );
    let wire = serde_json::to_value(&reduced).unwrap().to_string();
    assert_eq!(
        wire.matches("\"kind\":\"add\"").count(),
        1,
        "the final checked operation remains"
    );
    for (_, code) in modes(&p) {
        for x in 0_u16..=255 {
            let input = list(&[uint(x)]);
            if x < 100 {
                accepted(run(&code, &input, LIMIT), &uint(x + 1));
            } else {
                failed(run(&code, &input, LIMIT), FailureCode::Raise);
            }
        }
        for input in [
            atom(&[]),
            list(&[atom(&[0])]),
            list(&[atom(&[0, 1])]),
            list(&[uint(256)]),
            list(&[pair(&atom(&[]), &atom(&[]))]),
            list(&[uint(1), uint(2)]),
        ] {
            abi_rejected(run(&code, &input, LIMIT));
        }
    }
}

#[test]
fn false_guards_signed_edges_and_u128_bounds_use_mathematical_integers() {
    for ty in [integer(8, true), integer(128, true), integer(128, false)] {
        let Type::Int { bits, signed } = ty else {
            unreachable!()
        };
        let max: BigInt = (BigInt::from(1) << (if signed { bits - 1 } else { bits }) as usize) - 1;
        let p = increment(&ty, &max.to_string());
        assert_eq!(count(&optimize(&p).unwrap().body, "sequence"), 0);
        for (_, code) in modes(&p) {
            for x in [BigInt::from(0), &max - 1] {
                accepted(
                    run(&code, &list(&[number(x.clone())]), LIMIT),
                    &number(x + 1),
                );
            }
            failed(
                run(&code, &list(&[number(max.clone())]), LIMIT),
                FailureCode::Raise,
            );
        }
        if signed {
            let min = -(BigInt::from(1) << (bits - 1) as usize);
            let op = bin(BinaryOp::Sub, arg(0), lit(1, &ty), &ty);
            // False x <= MIN branch means x > MIN, so subtraction is safe.
            let p = program(
                Environment::Strict,
                vec![ty.clone()],
                ty.clone(),
                branch(
                    bin(BinaryOp::Le, arg(0), lit(&min, &ty), &ty),
                    explicit_reject(),
                    repeat(op.clone(), op),
                ),
                vec![],
            );
            assert_eq!(count(&optimize(&p).unwrap().body, "sequence"), 0);
            for (_, code) in modes(&p) {
                failed(
                    run(&code, &list(&[number(min.clone())]), LIMIT),
                    FailureCode::Raise,
                );
                accepted(
                    run(&code, &list(&[number(&min + 1)]), LIMIT),
                    &number(min.clone()),
                );
            }
        }
    }
}

#[test]
fn a_closed_overflow_boundary_cannot_discard_an_initializer_before_a_lazy_result() {
    for (ty, kind, bound) in [
        (u8_type(), BinaryOp::Add, BigInt::from(255)),
        (integer(8, true), BinaryOp::Sub, BigInt::from(-128)),
    ] {
        let op = bin(kind, arg(0), lit(1, &ty), &ty);
        let result = branch(arg(1), op.clone(), lit(7, &ty));
        let condition = bin(
            if kind == BinaryOp::Add {
                BinaryOp::Le
            } else {
                BinaryOp::Ge
            },
            arg(0),
            lit(&bound, &ty),
            &ty,
        );
        let p = program(
            Environment::Strict,
            vec![ty.clone(), Type::Bool],
            ty.clone(),
            guarded(condition, repeat(op, result)),
            vec![],
        );
        assert_eq!(count(&optimize(&p).unwrap().body, "sequence"), 1);
        for (_, code) in modes(&p) {
            failed(
                run(&code, &list(&[number(bound.clone()), atom(&[])]), LIMIT),
                FailureCode::Raise,
            );
            accepted(
                run(&code, &list(&[number(3), atom(&[])]), LIMIT),
                &number(7),
            );
        }
    }
}

#[test]
fn aliases_successful_joins_and_function_scopes_do_not_share_wrong_facts() {
    let ty = u8_type();
    let op = bin(BinaryOp::Add, arg(0), lit(1, &ty), &ty);
    let p = program(
        Environment::Strict,
        vec![ty.clone()],
        ty.clone(),
        binding(
            0,
            ty.clone(),
            arg(0),
            sequence(
                branch(
                    bin(BinaryOp::Lt, local(0), lit(100, &ty), &ty),
                    Expr::Unit,
                    explicit_reject(),
                ),
                repeat(op.clone(), op),
            ),
        ),
        vec![],
    );
    assert_eq!(
        count(&optimize(&p).unwrap().body, "sequence"),
        1,
        "keep the assertion, drop only its safe arithmetic successor"
    );
    for (_, code) in modes(&p) {
        accepted(run(&code, &list(&[uint(99)]), LIMIT), &uint(100));
        failed(run(&code, &list(&[uint(100)]), LIMIT), FailureCode::Raise);
    }

    let op = bin(BinaryOp::Add, arg(0), lit(1, &ty), &ty);
    let f = function(
        0,
        vec![ty.clone(), Type::Bool],
        ty.clone(),
        repeat(op.clone(), branch(arg(1), op, lit(7, &ty))),
    );
    let p = program(
        Environment::Strict,
        vec![ty.clone()],
        ty.clone(),
        guarded(
            bin(BinaryOp::Lt, arg(0), lit(100, &ty), &ty),
            call(0, vec![lit(255, &ty), Expr::Bool { value: false }]),
        ),
        vec![f],
    );
    for (_, code) in modes(&p) {
        failed(run(&code, &list(&[uint(1)]), LIMIT), FailureCode::Raise);
    }
}

#[test]
fn overflow_stays_before_spin_and_later_facts_do_not_remove_earlier_failure() {
    let ty = u8_type();
    let op = bin(BinaryOp::Add, arg(0), lit(1, &ty), &ty);
    let p = program(
        Environment::Strict,
        vec![ty.clone()],
        ty.clone(),
        repeat(op.clone(), sequence(call(0, vec![]), op)),
        vec![spin(&Type::Unit)],
    );
    for (_, code) in modes(&p) {
        failed(run(&code, &list(&[uint(255)]), LIMIT), FailureCode::Raise);
        failed(
            run(&code, &list(&[uint(1)]), LIMIT),
            FailureCode::CostExceeded,
        );
    }
    let op = bin(BinaryOp::Add, arg(0), lit(1, &ty), &ty);
    let p = program(
        Environment::Strict,
        vec![ty.clone()],
        ty.clone(),
        repeat(
            op.clone(),
            guarded(bin(BinaryOp::Lt, arg(0), lit(100, &ty), &ty), op),
        ),
        vec![],
    );
    assert_eq!(
        count(&optimize(&p).unwrap().body, "sequence"),
        1,
        "a future guard is not available at the first initializer"
    );
}

#[test]
fn comparison_polarity_literal_left_and_boolean_wrappers_keep_the_same_bound() {
    let ty = u8_type();
    let pred = bin(BinaryOp::Lt, arg(0), lit(100, &ty), &ty);
    let neg = Expr::Not {
        value: Box::new(pred.clone()),
    };
    let mut conditions = vec![
        pred.clone(),
        bin(BinaryOp::Gt, lit(100, &ty), arg(0), &ty),
        Expr::Not {
            value: Box::new(bin(BinaryOp::Ge, arg(0), lit(100, &ty), &ty)),
        },
    ];
    for kind in [BinaryOp::Eq, BinaryOp::Ne] {
        for value in [false, true] {
            let input = if (kind == BinaryOp::Eq) == value {
                pred.clone()
            } else {
                neg.clone()
            };
            for reverse in [false, true] {
                let truth = Expr::Bool { value };
                conditions.push(if reverse {
                    bin(kind, truth, input.clone(), &Type::Bool)
                } else {
                    bin(kind, input.clone(), truth, &Type::Bool)
                });
            }
        }
    }
    for condition in conditions {
        let op = bin(BinaryOp::Add, arg(0), lit(1, &ty), &ty);
        let p = program(
            Environment::Strict,
            vec![ty.clone()],
            ty.clone(),
            branch(condition, repeat(op.clone(), op), explicit_reject()),
            vec![],
        );
        assert_eq!(count(&optimize(&p).unwrap().body, "sequence"), 0);
        for (_, code) in modes(&p) {
            for x in [0, 99, 100, 255] {
                if x < 100 {
                    accepted(run(&code, &list(&[uint(x)]), LIMIT), &uint(x + 1));
                } else {
                    failed(run(&code, &list(&[uint(x)]), LIMIT), FailureCode::Raise);
                }
            }
        }
    }
}

#[test]
fn two_successful_branch_intervals_join_by_hull_and_cannot_hide_overflow() {
    let ty = u8_type();
    let op = bin(BinaryOp::Add, arg(0), lit(1, &ty), &ty);
    let check = branch(
        local(0),
        branch(
            bin(BinaryOp::Lt, arg(0), lit(100, &ty), &ty),
            Expr::Unit,
            explicit_reject(),
        ),
        branch(
            bin(BinaryOp::Gt, arg(0), lit(200, &ty), &ty),
            Expr::Unit,
            explicit_reject(),
        ),
    );
    let p = program(
        Environment::Strict,
        vec![ty.clone(), Type::Bool, Type::Bool],
        ty.clone(),
        binding(
            0,
            Type::Bool,
            arg(1),
            sequence(check, repeat(op.clone(), branch(arg(2), op, lit(7, &ty)))),
        ),
        vec![],
    );
    assert_eq!(count(&optimize(&p).unwrap().body, "sequence"), 2);
    for (_, code) in modes(&p) {
        failed(
            run(&code, &list(&[uint(255), atom(&[]), atom(&[])]), LIMIT),
            FailureCode::Raise,
        );
        accepted(
            run(&code, &list(&[uint(99), atom(&[1]), atom(&[])]), LIMIT),
            &uint(7),
        );
        accepted(
            run(&code, &list(&[uint(201), atom(&[]), atom(&[])]), LIMIT),
            &uint(7),
        );
    }
}

#[test]
fn disjoint_local_id_reuse_does_not_leak_a_first_argument_bound_into_the_second() {
    let ty = u8_type();
    let op = bin(BinaryOp::Add, local(0), lit(1, &ty), &ty);
    let first = binding(
        0,
        ty.clone(),
        arg(0),
        guarded(
            bin(BinaryOp::Lt, local(0), lit(100, &ty), &ty),
            repeat(op.clone(), op.clone()),
        ),
    );
    let second = binding(
        0,
        ty.clone(),
        arg(1),
        repeat(op.clone(), branch(arg(2), op, lit(7, &ty))),
    );
    let p = program(
        Environment::Strict,
        vec![ty.clone(), ty.clone(), Type::Bool],
        Type::Tuple {
            items: vec![ty.clone(), ty],
        },
        Expr::Tuple {
            items: vec![first, second],
        },
        vec![],
    );
    for (_, code) in modes(&p) {
        failed(
            run(&code, &list(&[uint(1), uint(255), atom(&[])]), LIMIT),
            FailureCode::Raise,
        );
        accepted(
            run(&code, &list(&[uint(1), uint(2), atom(&[])]), LIMIT),
            &list(&[uint(2), uint(7)]),
        );
    }
}

#[test]
fn hashing_calls_and_checked_raw_projections_never_become_total_initializers() {
    let ty = u8_type();
    let hashed = Expr::Sha256 {
        parts: vec![Expr::Atom {
            hex: "616263".into(),
        }],
    };
    let hash_op = bin(BinaryOp::Add, field(hashed, 0), lit(0, &ty), &ty);
    let hash_p = program(
        Environment::Strict,
        vec![Type::Bool],
        ty.clone(),
        repeat(hash_op.clone(), branch(arg(0), hash_op, lit(7, &ty))),
        vec![],
    );
    assert_eq!(count(&optimize(&hash_p).unwrap().body, "sequence"), 1);
    for (_, code) in modes(&hash_p) {
        let cost = accepted(run(&code, &list(&[atom(&[])]), LIMIT), &uint(7));
        failed(
            run(&code, &list(&[atom(&[])]), cost - 1),
            FailureCode::CostExceeded,
        );
        // First SHA256("abc") byte is 0xba, independently specified.
        accepted(run(&code, &list(&[atom(&[1])]), LIMIT), &uint(186));
    }
    let called = bin(BinaryOp::Add, call(0, vec![]), lit(0, &ty), &ty);
    let p = program(
        Environment::Strict,
        vec![Type::Bool],
        ty.clone(),
        repeat(called.clone(), branch(arg(0), called, lit(7, &ty))),
        vec![function(0, vec![], ty.clone(), explicit_reject())],
    );
    for (_, code) in modes(&p) {
        failed(run(&code, &list(&[atom(&[])]), LIMIT), FailureCode::Raise);
    }

    let projected = field(
        Expr::TreeAtomValue {
            value: Box::new(first(arg(0))),
        },
        0,
    );
    // Atom indexing is deliberately not admitted by IR: malformed dead input
    // must fail before interval analysis rather than gaining a numeric bound.
    let invalid = program(Environment::Raw, vec![Type::Tree], ty, projected, vec![]);
    assert!(optimize(&invalid).is_err());
}

#[test]
fn deep_input_and_oversized_literal_work_fall_back_without_new_admission_limits() {
    let mut body = arg(0);
    for _ in 0..193 {
        body = Expr::Not {
            value: Box::new(body),
        };
    }
    let p = program(
        Environment::Strict,
        vec![Type::Bool],
        Type::Bool,
        body,
        vec![function(0, vec![], Type::Unit, Expr::Unit)],
    );
    p.validate().unwrap();
    assert_eq!(
        serde_json::to_value(optimize(&p).unwrap()).unwrap(),
        serde_json::to_value(&p).unwrap()
    );
    assert!(
        chialisp_oxide_ir::compile(&p, true)
            .unwrap_err()
            .to_string()
            .contains("runtime expression depth budget exceeded")
    );

    let p = increment(&u8_type(), "100");
    let mut wide = p.clone();
    wide.body = Expr::Tuple {
        items: vec![Expr::Bool { value: true }; 32768],
    };
    assert!(
        optimize(&wide)
            .unwrap_err()
            .to_string()
            .contains("tuple too large")
    );
}

#[test]
fn a_checked_projection_in_an_arithmetic_operand_remains_strict() {
    let ty = u8_type();
    let operand = sequence(first(arg(0)), lit(1, &ty));
    let op = bin(BinaryOp::Add, operand, lit(0, &ty), &ty);
    let p = program(
        Environment::Raw,
        vec![Type::Tree],
        ty.clone(),
        repeat(op.clone(), branch(is_pair(arg(0)), op, lit(7, &ty))),
        vec![],
    );
    assert_eq!(count(&optimize(&p).unwrap().body, "sequence"), 3);
    for (_, code) in modes(&p) {
        abi_rejected(run(&code, &atom(&[]), LIMIT));
        accepted(run(&code, &pair(&atom(&[]), &atom(&[])), LIMIT), &uint(1));
    }
}

#[test]
fn large_valid_literal_payloads_skip_optional_interval_work_before_copying() {
    let ty = u8_type();
    let op = bin(BinaryOp::Add, arg(0), lit(1, &ty), &ty);
    let mut values = vec![op.clone()];
    values.extend((0..255).map(|_| Expr::Atom {
        hex: "61".repeat(4096),
    }));
    let mut types = vec![ty.clone()];
    types.extend(vec![Type::Atom; 255]);
    let p = program(
        Environment::Strict,
        vec![ty.clone()],
        Type::Tuple { items: types },
        guarded(
            bin(BinaryOp::Lt, arg(0), lit(100, &ty), &ty),
            repeat(op, Expr::Tuple { items: values }),
        ),
        vec![],
    );
    p.validate().unwrap();
    let reduced =
        optimize(&p).expect("optional interval budget must not reject valid prior optimization");
    assert_eq!(
        count(&reduced.body, "sequence"),
        1,
        "literal work exceeds this pass's pre-copy budget, so retain the old route"
    );
}

#[test]
fn integer_equality_and_endpoint_exclusion_keep_interior_holes_conservative() {
    let ty = u8_type();
    // Excluding an endpoint can tighten an interval. Excluding an interior
    // point cannot remove the still-accepted overflow boundary at 255.
    for (kind, boundary, arithmetic, removed) in [
        (BinaryOp::Ne, 255, BinaryOp::Add, true),
        (BinaryOp::Eq, 4, BinaryOp::Add, true),
        (BinaryOp::Ne, 0, BinaryOp::Sub, true),
        (BinaryOp::Ne, 128, BinaryOp::Add, false),
    ] {
        let op = bin(arithmetic, arg(0), lit(1, &ty), &ty);
        let p = program(
            Environment::Strict,
            vec![ty.clone(), Type::Bool],
            ty.clone(),
            branch(
                bin(kind, arg(0), lit(boundary, &ty), &ty),
                repeat(op.clone(), branch(arg(1), op, lit(7, &ty))),
                explicit_reject(),
            ),
            vec![],
        );
        assert_eq!(
            count(&optimize(&p).unwrap().body, "sequence"),
            usize::from(!removed)
        );
        for (_, code) in modes(&p) {
            for x in [0_u16, 1, 4, 127, 128, 254, 255] {
                for include_result in [false, true] {
                    let guard = if kind == BinaryOp::Eq {
                        x == boundary
                    } else {
                        x != boundary
                    };
                    let arithmetic = if arithmetic == BinaryOp::Add {
                        x.checked_add(1).filter(|n| *n <= 255)
                    } else {
                        x.checked_sub(1)
                    };
                    let input = list(&[uint(x), atom(if include_result { &[1] } else { &[] })]);
                    if let Some(result) = arithmetic.filter(|_| guard) {
                        accepted(
                            run(&code, &input, LIMIT),
                            &uint(if include_result { result } else { 7 }),
                        );
                    } else {
                        failed(run(&code, &input, LIMIT), FailureCode::Raise);
                    }
                }
            }
        }
    }
}

#[test]
fn bounded_computed_values_and_aliases_retain_their_own_intervals() {
    let ty = u8_type();
    let op = bin(BinaryOp::Add, local(1), lit(1, &ty), &ty);
    let p = program(
        Environment::Strict,
        vec![ty.clone()],
        ty.clone(),
        guarded(
            bin(BinaryOp::Lt, arg(0), lit(100, &ty), &ty),
            binding(
                0,
                ty.clone(),
                bin(BinaryOp::Add, arg(0), lit(1, &ty), &ty),
                binding(1, ty.clone(), local(0), repeat(op.clone(), op)),
            ),
        ),
        vec![],
    );
    assert_eq!(count(&optimize(&p).unwrap().body, "sequence"), 0);
    for (_, code) in modes(&p) {
        for x in [0, 1, 99] {
            accepted(run(&code, &list(&[uint(x)]), LIMIT), &uint(x + 2));
        }
        for x in [100, 255] {
            failed(run(&code, &list(&[uint(x)]), LIMIT), FailureCode::Raise);
        }
    }

    // A computed initializer is a fresh value, not an alias of its operand.
    // Bounding 255 - x below 100 does not prove x + 1 safe at x == 255.
    let op = bin(BinaryOp::Add, arg(0), lit(1, &ty), &ty);
    let p = program(
        Environment::Strict,
        vec![ty.clone(), Type::Bool],
        ty.clone(),
        binding(
            0,
            ty.clone(),
            bin(BinaryOp::Sub, lit(255, &ty), arg(0), &ty),
            guarded(
                bin(BinaryOp::Lt, local(0), lit(100, &ty), &ty),
                repeat(op.clone(), branch(arg(1), op, lit(7, &ty))),
            ),
        ),
        vec![],
    );
    assert_eq!(count(&optimize(&p).unwrap().body, "sequence"), 1);
    for (_, code) in modes(&p) {
        failed(
            run(&code, &list(&[uint(255), atom(&[])]), LIMIT),
            FailureCode::Raise,
        );
        failed(
            run(&code, &list(&[uint(155), atom(&[])]), LIMIT),
            FailureCode::Raise,
        );
        accepted(run(&code, &list(&[uint(200), atom(&[])]), LIMIT), &uint(7));
        accepted(
            run(&code, &list(&[uint(200), atom(&[1])]), LIMIT),
            &uint(201),
        );
    }
}

#[test]
fn two_independently_bounded_operands_and_nested_arithmetic_can_be_total() {
    let ty = u8_type();
    let sum = bin(BinaryOp::Add, arg(0), arg(1), &ty);
    let op = bin(BinaryOp::Sub, sum, lit(1, &ty), &ty);
    let p = program(
        Environment::Strict,
        vec![ty.clone(), ty.clone()],
        ty.clone(),
        guarded(
            bin(BinaryOp::Lt, arg(0), lit(100, &ty), &ty),
            guarded(
                bin(BinaryOp::Lt, arg(1), lit(100, &ty), &ty),
                guarded(
                    bin(BinaryOp::Ge, arg(0), lit(1, &ty), &ty),
                    repeat(op.clone(), op),
                ),
            ),
        ),
        vec![],
    );
    assert_eq!(count(&optimize(&p).unwrap().body, "sequence"), 0);
    for (_, code) in modes(&p) {
        for (x, y) in [(1, 0), (99, 99), (1, 99)] {
            accepted(
                run(&code, &list(&[uint(x), uint(y)]), LIMIT),
                &uint(x + y - 1),
            );
        }
        for (x, y) in [(0, 0), (100, 1), (1, 100), (255, 255)] {
            failed(
                run(&code, &list(&[uint(x), uint(y)]), LIMIT),
                FailureCode::Raise,
            );
        }
    }
}

#[test]
fn successful_branch_hulls_prove_only_the_shared_safe_range() {
    let ty = u8_type();
    let op = bin(BinaryOp::Add, arg(0), lit(1, &ty), &ty);
    let check = branch(
        local(0),
        branch(
            bin(BinaryOp::Lt, arg(0), lit(100, &ty), &ty),
            Expr::Unit,
            explicit_reject(),
        ),
        branch(
            bin(BinaryOp::Gt, arg(0), lit(200, &ty), &ty),
            branch(
                bin(BinaryOp::Lt, arg(0), lit(250, &ty), &ty),
                Expr::Unit,
                explicit_reject(),
            ),
            explicit_reject(),
        ),
    );
    let p = program(
        Environment::Strict,
        vec![ty.clone(), Type::Bool],
        ty,
        binding(
            0,
            Type::Bool,
            arg(1),
            sequence(check, repeat(op.clone(), op)),
        ),
        vec![],
    );
    assert_eq!(count(&optimize(&p).unwrap().body, "sequence"), 1);
    for (_, code) in modes(&p) {
        for (x, selector) in [(99, true), (201, false), (249, false)] {
            accepted(
                run(
                    &code,
                    &list(&[uint(x), atom(if selector { &[1] } else { &[] })]),
                    LIMIT,
                ),
                &uint(x + 1),
            );
        }
        for (x, selector) in [(100, true), (200, false), (250, false), (255, false)] {
            failed(
                run(
                    &code,
                    &list(&[uint(x), atom(if selector { &[1] } else { &[] })]),
                    LIMIT,
                ),
                FailureCode::Raise,
            );
        }
    }
}

#[test]
fn a_numeric_bound_cannot_make_a_checked_tree_operand_total() {
    let ty = u8_type();
    let operand = sequence(first(arg(0)), arg(1));
    let op = bin(BinaryOp::Add, operand, lit(1, &ty), &ty);
    let p = program(
        Environment::Strict,
        vec![Type::Tree, ty.clone(), Type::Bool],
        ty.clone(),
        binding(
            0,
            Type::Tree,
            arg(0),
            guarded(
                bin(BinaryOp::Lt, arg(1), lit(100, &ty), &ty),
                repeat(op.clone(), branch(arg(2), op, lit(7, &ty))),
            ),
        ),
        vec![],
    );
    assert_eq!(count(&optimize(&p).unwrap().body, "sequence"), 3);
    for (_, code) in modes(&p) {
        abi_rejected(run(&code, &list(&[atom(&[]), uint(1), atom(&[])]), LIMIT));
        accepted(
            run(
                &code,
                &list(&[pair(&atom(&[]), &atom(&[])), uint(1), atom(&[])]),
                LIMIT,
            ),
            &uint(7),
        );
    }
}
