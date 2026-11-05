//! Finite ABI and evaluation-order regressions for the emitted integer decoder.
//! Expected canonical bytes and bounds come from BigInt, independently of the
//! compiler's substring checks. Cost exhaustion is diagnostic, never rejection.
#[allow(dead_code)]
#[path = "runtime_evaluation/support.rs"]
mod support;

use chialisp_oxide_ir::{Environment, Expr, Type};
use chialisp_oxide_vm::FailureCode;
use num_bigint::BigInt;
use support::*;

fn integer(bits: u16, signed: bool) -> Type {
    Type::Int { bits, signed }
}

fn canonical(value: &BigInt) -> Vec<u8> {
    if *value == BigInt::from(0) {
        vec![]
    } else {
        value.to_signed_bytes_be()
    }
}

fn identity(ty: &Type, runtime: bool) -> chialisp_oxide_ir::Program {
    let body = if runtime {
        binding(0, ty.clone(), arg(0), local(0))
    } else {
        arg(0)
    };
    program(
        Environment::Strict,
        vec![ty.clone()],
        ty.clone(),
        body,
        vec![],
    )
}

#[test]
fn integer_decoder_accepts_exact_canonical_widths_in_both_lowering_modes() {
    for bits in [8, 16, 32, 64, 128] {
        for signed in [false, true] {
            let magnitude = BigInt::from(1) << usize::from(bits - u16::from(signed));
            let min = if signed { -&magnitude } else { 0.into() };
            let max: BigInt = &magnitude - 1;
            let mut values = (0..=255).map(|x| vec![x]).collect::<Vec<_>>();
            values.push(vec![]);
            values.extend([
                vec![0, 0x7f, 0],
                vec![0, 0x80, 0],
                vec![0xff, 0x7f, 0],
                vec![0xff, 0x80, 0],
            ]);
            // In particular: 0080, 0001, 0000, ffff, ff7f and ff80.
            for first in [0, 1, 0x7f, 0x80, 0xff] {
                for second in [0, 1, 0x7f, 0x80, 0xfe, 0xff] {
                    values.push(vec![first, second]);
                }
            }
            for value in [
                &min - 1,
                min.clone(),
                &min + 1,
                &max - 1,
                max.clone(),
                &max + 1,
            ] {
                let bytes = canonical(&value);
                values.push(bytes.clone());
                let mut padded = vec![if value < 0.into() { 0xff } else { 0 }];
                padded.extend(bytes);
                values.push(padded);
            }
            values.push(vec![0; usize::from(bits / 8) + 2]);
            values.push(vec![0xff; usize::from(bits / 8) + 2]);
            values.sort();
            values.dedup();
            for runtime in [false, true] {
                for (optimize, code) in modes(&identity(&integer(bits, signed), runtime)) {
                    for bytes in &values {
                        let value = BigInt::from_signed_bytes_be(bytes);
                        let valid = canonical(&value) == *bytes && value >= min && value <= max;
                        let observation = run(&code, &list(&[atom(bytes)]), LIMIT);
                        if valid {
                            accepted(observation, &atom(bytes));
                        } else {
                            failed(observation, FailureCode::Raise);
                        }
                    }
                    for (malformed, failure) in [
                        (atom(&[]), FailureCode::Raise),
                        (atom(&[1]), FailureCode::Raise),
                        (list(&[pair(&atom(&[]), &atom(&[]))]), FailureCode::Raise),
                        (
                            list(&[atom(&[1]), atom(&[2])]),
                            // The unoptimized exact-arity equality sees a
                            // pair. Rue's existing nil truthiness optimization
                            // instead reaches the explicit reject.
                            if optimize {
                                FailureCode::Raise
                            } else {
                                FailureCode::InvalidOpArg
                            },
                        ),
                        (pair(&atom(&[1]), &atom(&[2])), FailureCode::Raise),
                    ] {
                        failed(run(&code, &malformed, LIMIT), failure);
                    }
                }
            }
        }
    }
}

#[test]
fn integer_decoder_exhausts_short_atoms_and_samples_long_sign_extensions() {
    let ty = integer(128, true);
    let lower = -(BigInt::from(1) << 127usize);
    let upper = (BigInt::from(1) << 127usize) - 1;
    let mut cases = vec![vec![]];
    cases.extend((0..=255).map(|value| vec![value]));
    cases.extend((0..=u16::MAX).map(|value| value.to_be_bytes().to_vec()));
    // A two-byte lower boundary such as 007f must reject every extension;
    // comparing strictly with 007f would accidentally accept 007f00.
    for prefix in [[0, 0x7f], [0, 0x80], [0xff, 0x7f], [0xff, 0x80]] {
        for suffix in [0, 1, 0x7f, 0x80, 0xff] {
            for length in [3, 8, 16, 17, 18, 32] {
                let mut bytes = prefix.to_vec();
                bytes.resize(length, suffix);
                cases.push(bytes);
            }
        }
    }
    // Reproducible samples; these are finite executed tests, not a general
    // optimizer or cryptographic randomness claim.
    let mut state = 0x7aa4_529b_7813_c6d5u64;
    for sample in 0..1024 {
        let mut bytes = Vec::new();
        for _ in 0..3 + sample % 30 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            bytes.push(state as u8);
        }
        if sample % 3 != 0 {
            bytes[0] = if sample % 3 == 1 { 0 } else { 0xff };
        }
        cases.push(bytes);
    }
    for runtime in [false, true] {
        for (_, code) in modes(&identity(&ty, runtime)) {
            for bytes in &cases {
                let value = BigInt::from_signed_bytes_be(bytes);
                let valid = canonical(&value) == *bytes && value >= lower && value <= upper;
                let actual = run(&code, &list(&[atom(bytes)]), LIMIT);
                if valid {
                    accepted(actual, &atom(bytes));
                } else {
                    failed(actual, FailureCode::Raise);
                }
            }
        }
    }
}

#[test]
fn integer_decoder_rejects_before_a_divergent_or_failing_body() {
    let ty = integer(8, true);
    let source = program(
        Environment::Strict,
        vec![ty.clone()],
        ty.clone(),
        call(0, vec![]),
        vec![spin(&ty)],
    );
    for (_, code) in modes(&source) {
        // Nil and a single ff are canonical and must reach the spin. Substrings
        // requiring byte 1 must remain lazy, rather than failing on these atoms.
        for bytes in [vec![], vec![1], vec![0xff], vec![0x80], vec![0x7f]] {
            failed(
                run(&code, &list(&[atom(&bytes)]), LIMIT),
                FailureCode::CostExceeded,
            );
        }
        for bytes in [
            vec![0],
            vec![0, 0],
            vec![0, 1],
            vec![0xff, 0xff],
            vec![0, 0x80],
            vec![0xff, 0x7f],
            vec![0; 4],
        ] {
            failed(
                run(&code, &list(&[atom(&bytes)]), LIMIT),
                FailureCode::Raise,
            );
        }
    }
    let mut failing = source;
    failing.functions.clear();
    failing.body = Expr::Sequence {
        first: Box::new(Expr::TreeFirst {
            value: Box::new(tree_atom("")),
        }),
        then_value: Box::new(Expr::Int {
            value: "1".into(),
            ty,
        }),
    };
    for (_, code) in modes(&failing) {
        failed(
            run(&code, &list(&[atom(&[1])]), LIMIT),
            FailureCode::InvalidOpArg,
        );
        failed(run(&code, &list(&[atom(&[0])]), LIMIT), FailureCode::Raise);
    }
}
