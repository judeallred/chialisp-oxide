//! Representation regressions for typed bitvector inputs, mathematical checked
//! arithmetic, tuple scopes and conditional forwarding. Model checks plus
//! finite VM replay do not establish compiler preservation.
use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Parameter, Program, Type};
use chialisp_oxide_verifier::{
    Comparison, Obligation, Predicate, ReplayOutcome, SolverConfig, Specification, Term,
    Verification, Witness, WitnessValue, emit, replay, verify,
};
use chialisp_oxide_vm::FailureCode;
use num_bigint::BigInt;
use std::sync::Mutex;

static SOLVER: Mutex<()> = Mutex::new(());
fn arg(index: usize) -> Expr {
    Expr::Arg { index }
}
fn input(argument: usize, fields: Vec<usize>) -> Term {
    Term::Input { argument, fields }
}
fn number(value: impl ToString) -> Term {
    Term::Int {
        value: value.to_string(),
    }
}
fn literal(ty: &Type, value: impl ToString) -> Expr {
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
fn eq(left: Term, right: Term) -> Predicate {
    Predicate::Compare {
        relation: Comparison::Eq,
        left,
        right,
    }
}
fn program(types: Vec<Type>, result: Type, body: Expr) -> Program {
    Program {
        name: "bitwise-inputs".into(),
        environment: Environment::Strict,
        parameters: types
            .into_iter()
            .enumerate()
            .map(|(i, ty)| Parameter {
                name: format!("p{i}"),
                ty,
            })
            .collect(),
        result,
        body,
        functions: vec![],
    }
}
fn spec(assumptions: Predicate, property: Predicate) -> Specification {
    Specification {
        assumptions,
        obligation: Obligation::OnSuccess { property },
    }
}
fn prove(p: &Program, spec: &Specification) -> Witness {
    let result = verify(p, spec, &SolverConfig::default())
        .unwrap_or_else(|error| panic!("{error}; program={p:?}; spec={spec:?}"));
    let Verification::Proven { nonvacuity } = result else {
        panic!("{result:?}; program={p:?}; spec={spec:?}")
    };
    nonvacuity
}
fn expected(value: impl ToString) -> String {
    let n: BigInt = value.to_string().parse().unwrap();
    let bytes = if n == BigInt::from(0) {
        vec![]
    } else {
        n.to_signed_bytes_be()
    };
    if bytes.is_empty() {
        return "80".into();
    }
    let prefix = if bytes.len() == 1 && bytes[0] < 128 {
        String::new()
    } else {
        format!("{:02x}", 128 + bytes.len())
    };
    prefix
        + &bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
}
fn replay_equal(p: &Program, witness: &Witness, value: impl ToString) {
    let wanted = expected(value);
    for row in replay(p, witness, 1_000_000).unwrap() {
        let ReplayOutcome::Accepted { output_hex, .. } = row.outcome else {
            panic!("typed nonvacuity replay rejected")
        };
        assert_eq!(output_hex, wanted);
    }
}

#[test]
fn every_width_admits_both_extremes_and_excludes_only_out_of_range_inputs() {
    let _guard = SOLVER.lock().unwrap();
    for bits in [8, 16, 32, 64, 128] {
        for signed in [false, true] {
            let ty = Type::Int { bits, signed };
            let magnitude = BigInt::from(1u8) << usize::from(bits - u16::from(signed));
            let min = if signed { -&magnitude } else { BigInt::from(0) };
            let max = &magnitude - 1u8;
            let p = program(
                vec![ty.clone()],
                ty.clone(),
                binary(BinaryOp::Bitxor, &ty, arg(0), literal(&ty, 0)),
            );
            for limit in [&min, &max] {
                let witness = prove(
                    &p,
                    &spec(
                        eq(input(0, vec![]), number(limit)),
                        eq(Term::Output { fields: vec![] }, number(limit)),
                    ),
                );
                assert_eq!(
                    witness.arguments,
                    vec![WitnessValue::Int {
                        value: limit.to_string()
                    }]
                );
                replay_equal(&p, &witness, limit);
            }
            for outside in [&min - 1u8, &max + 1u8] {
                let excluded = spec(eq(input(0, vec![]), number(outside)), Predicate::default());
                assert_eq!(
                    verify(&p, &excluded, &SolverConfig::default()).unwrap(),
                    Verification::Vacuous
                );
            }
        }
    }
}

#[test]
fn checked_arithmetic_keeps_its_overflow_domain_before_bitvector_conversion() {
    let _guard = SOLVER.lock().unwrap();
    let ty = Type::Int {
        bits: 8,
        signed: false,
    };
    let sum = binary(BinaryOp::Add, &ty, arg(0), literal(&ty, 1));
    let p = program(
        vec![ty.clone()],
        ty.clone(),
        binary(BinaryOp::Bitand, &ty, sum, literal(&ty, 255)),
    );
    let relation = eq(
        Term::Output { fields: vec![] },
        Term::Add {
            left: Box::new(input(0, vec![])),
            right: Box::new(number(1)),
        },
    );
    let witness = prove(&p, &spec(Predicate::default(), relation));
    let [WitnessValue::Int { value }] = witness.arguments.as_slice() else {
        panic!("integer witness")
    };
    replay_equal(
        &p,
        &witness,
        value.parse::<u8>().unwrap().checked_add(1).unwrap(),
    );
    let acceptance = Specification {
        assumptions: Predicate::default(),
        obligation: Obligation::AcceptanceIff {
            predicate: Predicate::Compare {
                relation: Comparison::Lt,
                left: input(0, vec![]),
                right: number(255),
            },
        },
    };
    prove(&p, &acceptance);
    for row in replay(
        &p,
        &Witness {
            arguments: vec![WitnessValue::Int {
                value: "255".into(),
            }],
        },
        1_000_000,
    )
    .unwrap()
    {
        let ReplayOutcome::Rejected { failure, .. } = row.outcome else {
            panic!("overflow must reject before wrapping")
        };
        assert_eq!(failure.code(), FailureCode::Raise);
    }
    // Unsupported byte inputs remain outside scalar proofs even when unused;
    // the BV representation must not silently remove unsupported parameters.
    let mut bytes = p.clone();
    bytes.parameters.push(Parameter {
        name: "unused_bytes".into(),
        ty: Type::Bytes { len: 1 },
    });
    bytes.validate().unwrap();
    assert!(emit(&bytes, &acceptance).is_err());
}

#[test]
fn tuple_input_words_and_conditional_results_keep_signed_interpretation_and_scope() {
    let _guard = SOLVER.lock().unwrap();
    let signed = Type::Int {
        bits: 8,
        signed: true,
    };
    let unsigned = Type::Int {
        bits: 16,
        signed: false,
    };
    let types = vec![
        Type::Tuple {
            items: vec![signed.clone(), unsigned],
        },
        Type::Bool,
    ];
    let left = Expr::Field {
        value: Box::new(arg(0)),
        index: 0,
    };
    let selected = Expr::If {
        condition: Box::new(arg(1)),
        then_value: Box::new(binary(
            BinaryOp::Bitand,
            &signed,
            left.clone(),
            literal(&signed, 3),
        )),
        else_value: Box::new(binary(BinaryOp::Bitor, &signed, left, literal(&signed, 4))),
    };
    let p = program(
        types,
        signed.clone(),
        binary(BinaryOp::Bitxor, &signed, selected, literal(&signed, -1)),
    );
    for value in [i8::MIN, -1, 0, i8::MAX] {
        for flag in [false, true] {
            let wanted: i8 = !(if flag { value & 3 } else { value | 4 });
            let assumptions = Predicate::All {
                items: vec![
                    eq(input(0, vec![0]), number(value)),
                    eq(input(0, vec![1]), number(u16::MAX)),
                    eq(input(1, vec![]), Term::Bool { value: flag }),
                ],
            };
            let witness = prove(
                &p,
                &spec(
                    assumptions,
                    eq(Term::Output { fields: vec![] }, number(wanted)),
                ),
            );
            assert_eq!(
                witness.arguments,
                vec![
                    WitnessValue::Tuple {
                        items: vec![
                            WitnessValue::Int {
                                value: value.to_string()
                            },
                            WitnessValue::Int {
                                value: u16::MAX.to_string()
                            }
                        ]
                    },
                    WitnessValue::Bool { value: flag }
                ]
            );
            replay_equal(&p, &witness, wanted);
        }
    }
}
