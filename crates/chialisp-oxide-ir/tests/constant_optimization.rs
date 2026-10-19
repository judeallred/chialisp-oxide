//! Finite regressions for typed constant evaluation, strict effects and ABI
//! preservation. These test the owned IR pass, not Rust/compiler preservation.
#[path = "runtime_evaluation/support.rs"]
#[allow(
    dead_code,
    reason = "Shared fixtures also exercise other runtime operations."
)]
mod support;

use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Program, Type, compile, optimize};
use chialisp_oxide_vm::FailureCode;
use num_bigint::BigInt;
use support::*;

macro_rules! assert_ir_eq {
    ($left:expr, $right:expr $(,)?) => {
        assert_eq!(serde_json::to_value(&$left).unwrap(), serde_json::to_value(&$right).unwrap())
    };
    ($left:expr, $right:expr, $($message:tt)+) => {
        assert_eq!(serde_json::to_value(&$left).unwrap(), serde_json::to_value(&$right).unwrap(), $($message)+)
    };
}

fn word(value: impl ToString, ty: &Type) -> Expr {
    Expr::Int {
        value: value.to_string(),
        ty: ty.clone(),
    }
}
fn binary(kind: BinaryOp, ty: &Type, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        kind,
        ty: ty.clone(),
        left: Box::new(left),
        right: Box::new(right),
    }
}
fn literal(bytes: &[u8]) -> Expr {
    Expr::Atom {
        hex: hex::encode(bytes),
    }
}
fn boolean(value: bool) -> Expr {
    Expr::Bool { value }
}
fn ignored(result: Type, body: Expr) -> Program {
    program(Environment::Ignored, vec![], result, body, vec![])
}
fn integer_wire(value: &str) -> Vec<u8> {
    let value: BigInt = value.parse().unwrap();
    atom(&if value == BigInt::from(0) {
        vec![]
    } else {
        value.to_signed_bytes_be()
    })
}
fn contains(expr: &Expr, predicate: impl Fn(&Expr) -> bool) -> bool {
    let mut pending = vec![expr];
    while let Some(expr) = pending.pop() {
        if predicate(expr) {
            return true;
        }
        match expr {
            Expr::Tuple { items }
            | Expr::Sha256 { parts: items }
            | Expr::Call {
                arguments: items, ..
            } => pending.extend(items),
            Expr::Binary { left, right, .. } => pending.extend([left.as_ref(), right.as_ref()]),
            Expr::Let { value, body, .. }
            | Expr::Sequence {
                first: value,
                then_value: body,
            } => pending.extend([value.as_ref(), body.as_ref()]),
            Expr::If {
                condition,
                then_value,
                else_value,
            } => pending.extend([condition.as_ref(), then_value.as_ref(), else_value.as_ref()]),
            Expr::TreePair { first, rest } | Expr::Pair { first, rest } => {
                pending.extend([first.as_ref(), rest.as_ref()])
            }
            Expr::Not { value }
            | Expr::AtomIsEmpty { value }
            | Expr::Field { value, .. }
            | Expr::TreeAtom { value }
            | Expr::TreeAtomValue { value }
            | Expr::TreeIsPair { value }
            | Expr::TreeFirst { value }
            | Expr::PairFirst { value }
            | Expr::PairRest { value }
            | Expr::TreeRest { value } => pending.push(value),
            Expr::Unit
            | Expr::Bool { .. }
            | Expr::Int { .. }
            | Expr::Bytes { .. }
            | Expr::Atom { .. }
            | Expr::Arg { .. }
            | Expr::Local { .. }
            | Expr::Reject { .. } => (),
        }
    }
    false
}

#[test]
fn literal_tutorial_arithmetic_reaches_independent_chialisp_quotes_without_changing_off() {
    let ty = Type::Int {
        bits: 64,
        signed: false,
    };
    for (body, expected, source) in [
        (
            binary(BinaryOp::Add, &ty, word(2, &ty), word(3, &ty)),
            5,
            "(+ 2 3)",
        ),
        (
            binary(
                BinaryOp::Mul,
                &ty,
                branch(
                    binary(BinaryOp::Gt, &ty, word(3, &ty), word(2, &ty)),
                    word(10, &ty),
                    word(5, &ty),
                ),
                word(10, &ty),
            ),
            100,
            "(* (if (> 3 2) 10 5) 10)",
        ),
    ] {
        let p = ignored(ty.clone(), body);
        let off = compile(&p, false).unwrap();
        let folded = optimize(&p).unwrap();
        assert_ir_eq!(folded.body, word(expected, &ty));
        assert_ir_eq!(
            optimize(&folded).unwrap(),
            folded,
            "literal folding is idempotent"
        );
        assert_eq!(
            compile(&p, false).unwrap(),
            off,
            "on never mutates the source/off path"
        );
        let on = compile(&p, true).unwrap();
        let independent = chialisp_oxide_reference::compile_expression(source).unwrap();
        assert_eq!(on, independent);
        assert_eq!(on.len(), 3);
        assert!(off.len() > on.len());
        for environment in [atom(&[]), atom(&[0]), pair(&atom(&[1]), &atom(&[2]))] {
            let wanted = integer_wire(&expected.to_string());
            let before = accepted(run(&off, &environment, LIMIT), &wanted);
            let after = accepted(run(&on, &environment, LIMIT), &wanted);
            assert_eq!(after, 20);
            assert!(after < before);
        }
    }
}

fn check_word_case(
    ty: &Type,
    operation: BinaryOp,
    left: String,
    right: String,
    expected: Option<String>,
) {
    let p = ignored(
        ty.clone(),
        binary(operation, ty, word(left, ty), word(right, ty)),
    );
    let folded = optimize(&p).unwrap();
    match expected {
        Some(value) => {
            assert_ir_eq!(folded.body, word(&value, ty), "{operation:?}, {ty:?}");
            for (_, code) in modes(&p) {
                accepted(run(&code, &atom(&[]), LIMIT), &integer_wire(&value));
            }
        }
        None => {
            assert!(
                contains(
                    &folded.body,
                    |expr| matches!(expr, Expr::Binary { kind, .. } if *kind == operation)
                ),
                "overflow must remain evaluated: {operation:?}, {ty:?}"
            );
            for (_, code) in modes(&p) {
                failed(run(&code, &atom(&[]), LIMIT), FailureCode::Raise);
            }
        }
    }
}

#[test]
fn all_integer_widths_follow_native_checked_arithmetic_and_twos_complement_bit_values() {
    macro_rules! native {
        ($native:ty, $bits:literal, $signed:literal) => {{
            let ty = Type::Int {
                bits: $bits,
                signed: $signed,
            };
            let a: $native = 5;
            let b: $native = 3;
            for (op, value) in [
                (BinaryOp::Add, a.checked_add(b)),
                (BinaryOp::Sub, a.checked_sub(b)),
                (BinaryOp::Mul, a.checked_mul(b)),
                (BinaryOp::Bitand, Some(a & b)),
                (BinaryOp::Bitor, Some(a | b)),
                (BinaryOp::Bitxor, Some(a ^ b)),
            ] {
                check_word_case(
                    &ty,
                    op,
                    a.to_string(),
                    b.to_string(),
                    value.map(|n| n.to_string()),
                );
            }
            for (op, a, b, result) in [
                (
                    BinaryOp::Add,
                    <$native>::MAX,
                    1,
                    <$native>::MAX.checked_add(1),
                ),
                (
                    BinaryOp::Sub,
                    <$native>::MIN,
                    1,
                    <$native>::MIN.checked_sub(1),
                ),
                (
                    BinaryOp::Mul,
                    <$native>::MAX,
                    2,
                    <$native>::MAX.checked_mul(2),
                ),
                (
                    BinaryOp::Bitxor,
                    <$native>::MIN,
                    <$native>::MAX,
                    Some(<$native>::MIN ^ <$native>::MAX),
                ),
            ] {
                check_word_case(
                    &ty,
                    op,
                    a.to_string(),
                    b.to_string(),
                    result.map(|n| n.to_string()),
                );
            }
        }};
    }
    native!(u8, 8, false);
    native!(i8, 8, true);
    native!(u16, 16, false);
    native!(i16, 16, true);
    native!(u32, 32, false);
    native!(i32, 32, true);
    native!(u64, 64, false);
    native!(i64, 64, true);
    native!(u128, 128, false);
    native!(i128, 128, true);
    // Signed MIN * -1 must not become an unbounded CLVM positive integer.
    for bits in [8, 16, 32, 64, 128] {
        let ty = Type::Int { bits, signed: true };
        let min = -(BigInt::from(1) << usize::from(bits - 1));
        check_word_case(&ty, BinaryOp::Mul, min.to_string(), "-1".into(), None);
    }
}

#[test]
fn boolean_and_byte_folds_preserve_canonical_boolean_and_exact_lexical_bytes() {
    for left in [false, true] {
        for right in [false, true] {
            for (kind, expected) in [
                (BinaryOp::Eq, left == right),
                (BinaryOp::Ne, left != right),
                (BinaryOp::Bitand, left & right),
                (BinaryOp::Bitor, left | right),
                (BinaryOp::Bitxor, left ^ right),
            ] {
                let p = ignored(
                    Type::Bool,
                    binary(kind, &Type::Bool, boolean(left), boolean(right)),
                );
                assert_ir_eq!(optimize(&p).unwrap().body, boolean(expected));
                for (_, code) in modes(&p) {
                    accepted(
                        run(&code, &atom(&[]), LIMIT),
                        &atom(if expected { &[1] } else { &[] }),
                    );
                }
            }
        }
    }
    let vectors: &[&[u8]] = &[&[], &[0], &[0, 0], &[1], &[0, 1], &[127], &[128], &[255]];
    for left in vectors {
        for right in vectors {
            for (kind, expected) in [
                (BinaryOp::Eq, left == right),
                (BinaryOp::Ne, left != right),
                (BinaryOp::Lt, left < right),
                (BinaryOp::Le, left <= right),
                (BinaryOp::Gt, left > right),
                (BinaryOp::Ge, left >= right),
            ] {
                for fixed in [false, true] {
                    if fixed && left.len() != right.len() {
                        continue;
                    }
                    let ty = if fixed {
                        Type::Bytes { len: left.len() }
                    } else {
                        Type::Atom
                    };
                    let value = |bytes: &[u8]| {
                        if fixed {
                            Expr::Bytes {
                                hex: hex::encode(bytes),
                            }
                        } else {
                            literal(bytes)
                        }
                    };
                    let p = ignored(Type::Bool, binary(kind, &ty, value(left), value(right)));
                    assert_ir_eq!(optimize(&p).unwrap().body, boolean(expected));
                    // All folded relations execute; the off/on comparison is
                    // separately exhaustive over these same payloads in the
                    // existing atom_ordering/atom_equality regression targets.
                    accepted(
                        run(&compile(&p, true).unwrap(), &atom(&[]), LIMIT),
                        &atom(if expected { &[1] } else { &[] }),
                    );
                }
            }
        }
    }
    for value in [false, true] {
        let p = ignored(
            Type::Bool,
            Expr::Not {
                value: Box::new(boolean(value)),
            },
        );
        assert_ir_eq!(optimize(&p).unwrap().body, boolean(!value));
    }
}

#[test]
fn lexical_constants_do_not_capture_callee_or_sibling_scopes() {
    let p = program(
        Environment::Ignored,
        vec![],
        u8_type(),
        binding(0, u8_type(), int(5), call(0, vec![local(0)])),
        vec![
            function(
                0,
                vec![u8_type()],
                u8_type(),
                binding(0, u8_type(), int(7), add(local(0), arg(0))),
            ),
            function(
                1,
                vec![],
                u8_type(),
                binding(0, u8_type(), int(8), add(local(0), int(1))),
            ),
        ],
    );
    let folded = optimize(&p).unwrap();
    assert!(folded.functions.is_empty());
    let Expr::Let {
        local_id,
        ty,
        value,
        body,
    } = &folded.body
    else {
        panic!("the inlined argument must retain its lexical binding");
    };
    assert_eq!(*ty, u8_type());
    assert_ir_eq!(**value, int(5));
    assert_ir_eq!(**body, add(int(7), local(*local_id)));
    for (_, code) in modes(&p) {
        accepted(run(&code, &atom(&[]), LIMIT), &uint(12));
    }

    let p = program(
        Environment::Strict,
        vec![Type::Bool],
        u8_type(),
        branch(
            arg(0),
            binding(0, u8_type(), int(2), add(local(0), int(1))),
            binding(0, u8_type(), int(8), add(local(0), int(1))),
        ),
        vec![],
    );
    let folded = optimize(&p).unwrap();
    assert_ir_eq!(folded.body, branch(arg(0), int(3), int(9)));
    for (_, code) in modes(&p) {
        accepted(run(&code, &list(&[atom(&[1])]), LIMIT), &uint(3));
        accepted(run(&code, &list(&[atom(&[])]), LIMIT), &uint(9));
    }
}

#[test]
fn unused_total_discriminators_and_fields_drop_but_all_parameter_decoders_remain() {
    let discriminator = branch(is_pair(arg(0)), int(1), int(0));
    for body in [
        binding(0, u8_type(), discriminator.clone(), int(9)),
        sequence(discriminator, int(9)),
    ] {
        let p = program(Environment::Raw, vec![Type::Tree], u8_type(), body, vec![]);
        assert_ir_eq!(optimize(&p).unwrap().body, int(9));
        for (_, code) in modes(&p) {
            for env in [atom(&[]), atom(&[0]), pair(&atom(&[1]), &atom(&[2]))] {
                accepted(run(&code, &env, LIMIT), &uint(9));
            }
        }
    }
    let types = vec![u8_type(), Type::Bool, Type::Bytes { len: 2 }];
    for runtime in [false, true] {
        let functions = if runtime {
            vec![function(0, vec![], Type::Unit, Expr::Unit)]
        } else {
            vec![]
        };
        let p = program(
            Environment::Strict,
            types.clone(),
            u8_type(),
            sequence(field(arg(2), 0), int(9)),
            functions,
        );
        let folded = optimize(&p).unwrap();
        assert_ir_eq!(folded.body, int(9));
        assert_ir_eq!(folded.parameters, p.parameters);
        assert_eq!(folded.environment, Environment::Strict);
        for (_, code) in modes(&p) {
            accepted(
                run(&code, &list(&[uint(7), atom(&[]), atom(&[0, 255])]), LIMIT),
                &uint(9),
            );
            for malformed in [
                atom(&[]),
                list(&[uint(7)]),
                list(&[atom(&[0]), atom(&[]), atom(&[0, 255])]), // padded integer
                list(&[uint(7), atom(&[2]), atom(&[0, 255])]),   // noncanonical Bool
                list(&[uint(7), atom(&[]), atom(&[0])]),         // wrong fixed width
                list(&[uint(7), atom(&[]), pair(&atom(&[]), &atom(&[]))]),
                list(&[uint(7), atom(&[]), atom(&[0, 255]), atom(&[])]),
            ] {
                abi_rejected(run(&code, &malformed, LIMIT));
            }
        }
    }
}

#[test]
fn unused_overflow_projections_and_calls_retain_rejection_divergence_and_operand_order() {
    let overflow = add(int(255), int(1));
    let reject_value = sequence(
        Expr::Reject {
            message: "first explicit failure".into(),
        },
        int(1),
    );
    for (effect, expected) in [
        (overflow, vec![FailureCode::Raise]),
        (
            sequence(first(tree_atom("")), int(1)),
            // A literal first operation and a fused path report different
            // typed projection errors; neither resource class is admissible.
            vec![FailureCode::InvalidOpArg, FailureCode::PathIntoAtom],
        ),
        (reject_value, vec![FailureCode::Raise]),
    ] {
        let p = ignored(u8_type(), binding(0, u8_type(), effect, int(9)));
        assert!(matches!(optimize(&p).unwrap().body, Expr::Let { .. }));
        for (_, code) in modes(&p) {
            match run(&code, &atom(&[]), LIMIT) {
                Outcome::Failed(code) => assert!(expected.contains(&code)),
                other => panic!("required initializer did not reject: {other:?}"),
            }
        }
    }
    let functions = vec![
        spin(&u8_type()),
        function(
            1,
            vec![],
            u8_type(),
            Expr::Reject {
                message: "called rejection".into(),
            },
        ),
    ];
    for (left, right, expected) in [
        (call(1, vec![]), call(0, vec![]), FailureCode::Raise),
        (call(0, vec![]), call(1, vec![]), FailureCode::CostExceeded),
    ] {
        for body in [
            binary(BinaryOp::Add, &u8_type(), left.clone(), right.clone()),
            sequence(left.clone(), right.clone()),
            field(
                Expr::Tuple {
                    items: vec![left, right],
                },
                0,
            ),
        ] {
            let p = program(
                Environment::Ignored,
                vec![],
                u8_type(),
                body,
                functions.clone(),
            );
            let folded = optimize(&p).unwrap();
            assert!(contains(&folded.body, |expr| matches!(
                expr,
                Expr::Call { function_id: 0, .. }
            )));
            for (_, code) in modes(&p) {
                failed(run(&code, &atom(&[]), LIMIT), expected);
            }
        }
    }
    // Constant folding must not turn eager Bool bit operations into &&/||.
    for kind in [BinaryOp::Bitand, BinaryOp::Bitor] {
        let body = binary(
            kind,
            &Type::Bool,
            boolean(kind == BinaryOp::Bitor),
            Expr::Reject {
                message: "eager RHS".into(),
            },
        );
        let p = ignored(Type::Bool, body);
        for (_, code) in modes(&p) {
            failed(run(&code, &atom(&[]), LIMIT), FailureCode::Raise);
        }
    }
}

#[test]
fn literal_lazy_branches_skip_only_unreachable_effects_and_hash_initializers_stay_strict() {
    let functions = vec![spin(&u8_type())];
    for body in [
        branch(boolean(true), int(7), call(0, vec![])),
        branch(boolean(false), call(0, vec![]), int(7)),
    ] {
        let p = program(
            Environment::Ignored,
            vec![],
            u8_type(),
            body,
            functions.clone(),
        );
        assert_ir_eq!(optimize(&p).unwrap().body, int(7));
        for (_, code) in modes(&p) {
            accepted(run(&code, &atom(&[]), LIMIT), &uint(7));
        }
    }
    let hash = Expr::Sha256 {
        parts: vec![literal(&vec![0x5a; 4096])],
    };
    for body in [
        binding(0, Type::Bytes { len: 32 }, hash.clone(), int(7)),
        sequence(hash, int(7)),
    ] {
        let p = ignored(u8_type(), body);
        let folded = optimize(&p).unwrap();
        assert!(contains(&folded.body, |expr| matches!(
            expr,
            Expr::Sha256 { .. }
        )));
        for (_, code) in modes(&p) {
            failed(run(&code, &atom(&[]), 1_000), FailureCode::CostExceeded);
            accepted(run(&code, &atom(&[]), 1_000_000), &uint(7));
        }
    }
}

#[test]
fn known_bytes_and_constructor_fields_preserve_padding_high_bits_and_strict_siblings() {
    for bytes in [
        vec![],
        vec![0],
        vec![0, 0],
        vec![0, 1],
        vec![128],
        vec![255],
    ] {
        let tree = Expr::TreeAtom {
            value: Box::new(literal(&bytes)),
        };
        let p = ignored(
            Type::Atom,
            Expr::TreeAtomValue {
                value: Box::new(tree.clone()),
            },
        );
        assert_ir_eq!(optimize(&p).unwrap().body, literal(&bytes));
        for (_, code) in modes(&p) {
            accepted(run(&code, &atom(&[]), LIMIT), &atom(&bytes));
        }
        let p = ignored(
            Type::Bool,
            Expr::AtomIsEmpty {
                value: Box::new(literal(&bytes)),
            },
        );
        assert_ir_eq!(optimize(&p).unwrap().body, boolean(bytes.is_empty()));
        for (_, code) in modes(&p) {
            accepted(
                run(&code, &atom(&[]), LIMIT),
                &atom(if bytes.is_empty() { &[1] } else { &[] }),
            );
        }
    }
    for (index, wanted) in [(0, 0), (1, 128), (2, 255)] {
        let p = ignored(
            u8_type(),
            field(
                Expr::Bytes {
                    hex: "0080ff".into(),
                },
                index,
            ),
        );
        assert_ir_eq!(optimize(&p).unwrap().body, int(wanted));
        for (_, code) in modes(&p) {
            accepted(run(&code, &atom(&[]), LIMIT), &uint(wanted));
        }
    }
    let pair = Expr::TreePair {
        first: Box::new(tree_atom("80")),
        rest: Box::new(tree_atom("0000")),
    };
    let p = ignored(
        Type::Atom,
        Expr::TreeAtomValue {
            value: Box::new(rest(pair)),
        },
    );
    assert_ir_eq!(optimize(&p).unwrap().body, literal(&[0, 0]));
    for (_, code) in modes(&p) {
        accepted(run(&code, &atom(&[]), LIMIT), &atom(&[0, 0]));
    }
    let functions = vec![function(
        0,
        vec![],
        Type::Tree,
        Expr::Reject {
            message: "discarded pair sibling".into(),
        },
    )];
    let p = program(
        Environment::Ignored,
        vec![],
        Type::Tree,
        first(Expr::TreePair {
            first: Box::new(tree_atom("01")),
            rest: Box::new(call(0, vec![])),
        }),
        functions,
    );
    assert!(contains(&optimize(&p).unwrap().body, |expr| matches!(
        expr,
        Expr::Call { .. }
    )));
    for (_, code) in modes(&p) {
        failed(run(&code, &atom(&[]), LIMIT), FailureCode::Raise);
    }
}

#[test]
fn whole_input_validation_precedes_dead_code_folding() {
    for invalid in [
        binary(BinaryOp::Div, &u8_type(), int(1), int(0)),
        local(999),
        binding(0, Type::Bool, int(1), int(2)),
        binary(BinaryOp::Add, &u8_type(), boolean(false), int(1)),
    ] {
        let p = ignored(u8_type(), branch(boolean(true), int(7), invalid));
        assert!(
            optimize(&p).is_err(),
            "dead code never exempts type/operation validation"
        );
        assert!(compile(&p, true).is_err());
    }
}

#[test]
fn shared_known_values_do_not_materialize_exponential_trees_or_large_literal_aliases() {
    let mut body = is_pair(local(39));
    for id in (0..40).rev() {
        let value = if id == 0 {
            tree_atom("01")
        } else {
            Expr::TreePair {
                first: Box::new(local(id - 1)),
                rest: Box::new(local(id - 1)),
            }
        };
        body = binding(id, Type::Tree, value, body);
    }
    let p = ignored(Type::Bool, body);
    assert_ir_eq!(optimize(&p).unwrap().body, boolean(true));
    let body = binding(
        0,
        Type::Bytes { len: 4096 },
        Expr::Bytes {
            hex: "ab".repeat(4096),
        },
        Expr::Tuple {
            items: (0..256).map(|_| field(local(0), 0)).collect(),
        },
    );
    let p = ignored(
        Type::Tuple {
            items: vec![u8_type(); 256],
        },
        body,
    );
    let folded = optimize(&p).unwrap();
    let json = serde_json::to_vec(&folded).unwrap();
    assert!(
        json.len() < 30_000,
        "large payloads never duplicate into each Local: {}",
        json.len()
    );
    assert!(!contains(
        &folded.body,
        |expr| matches!(expr, Expr::Bytes { hex } if hex.len() > 128)
    ));
    folded.validate().unwrap();

    let inner_type = Type::Tuple {
        items: vec![Type::Atom; 256],
    };
    let wide = || Expr::Tuple {
        items: (0..256).map(|_| literal(&vec![0; 4096])).collect(),
    };
    let p = ignored(
        Type::Tuple {
            items: vec![inner_type; 2],
        },
        Expr::Tuple {
            items: vec![wide(), wide()],
        },
    );
    p.validate().unwrap();
    assert!(
        optimize(&p)
            .unwrap_err()
            .to_string()
            .contains("constant optimization literal-copy budget")
    );
}
