//! Finite backend tests for a constructed atom's checked view. This does not
//! admit unchecked source operations or prove general compiler/cost preservation.
#[path = "runtime_evaluation/support.rs"]
#[allow(
    dead_code,
    reason = "Shared IR/VM fixtures cover other independent operations."
)]
mod support;
use chialisp_oxide_ir::{Environment, Expr, Program, Type, compile, optimize};
use chialisp_oxide_vm::FailureCode;
use support::*;

const EMPTY_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const ABC_HASH: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
fn atom_expr(bytes: &[u8]) -> Expr {
    Expr::Atom {
        hex: hex::encode(bytes),
    }
}
fn roundtrip(value: Expr) -> Expr {
    Expr::TreeAtomValue {
        value: Box::new(Expr::TreeAtom {
            value: Box::new(value),
        }),
    }
}
fn cast(value: Expr) -> Expr {
    Expr::TreeAtomValue {
        value: Box::new(value),
    }
}
fn hash(value: Expr) -> Expr {
    Expr::Sha256 { parts: vec![value] }
}
fn bottom_atom() -> Expr {
    sequence(
        Expr::Reject {
            message: "required atom operand".into(),
        },
        atom_expr(&[0, 1]),
    )
}
fn digest_type() -> Type {
    Type::Bytes { len: 32 }
}
fn fixture(parameter: Type, body: Expr) -> Program {
    // A table makes this genuinely runtime lowering even for a literal/leaf
    // operand. The unused total function is never invoked or a policy oracle.
    program(
        Environment::Strict,
        vec![parameter],
        Type::Atom,
        body,
        vec![function(0, vec![], Type::Unit, Expr::Unit)],
    )
}
fn blob(payload: &[u8]) -> Vec<u8> {
    if payload.len() < 64 {
        return atom(payload);
    }
    let size = payload.len();
    assert!(size < 0x10_0000);
    let mut output = if size < 8192 {
        vec![0xc0 | (size >> 8) as u8, size as u8]
    } else {
        vec![0xe0 | (size >> 16) as u8, (size >> 8) as u8, size as u8]
    };
    output.extend(payload);
    output
}

#[test]
fn constructed_atom_and_byte_array_views_preserve_nil_padding_and_non_utf8() {
    for payload in [
        vec![],
        vec![0],
        vec![0, 0],
        vec![0, 1],
        vec![0x80],
        vec![0xff, 0x80],
        vec![0; 32],
    ] {
        for ty in [Type::Atom, Type::Bytes { len: payload.len() }] {
            let source = fixture(ty, roundtrip(arg(0)));
            assert_eq!(source.type_of(&source.body).unwrap(), Some(Type::Atom));
            let rewritten = optimize(&source).unwrap();
            assert_eq!(
                rewritten.type_of(&rewritten.body).unwrap(),
                Some(Type::Atom)
            );
            let input = list(&[blob(&payload)]);
            for (_, code) in modes(&source) {
                accepted(run(&code, &input, FINITE_LIMIT), &blob(&payload));
            }
        }
    }
    // Constants and decoded byte-array values are not integer atoms. All32 zero
    // bytes remain present rather than being normalized to CLVM numeric nil.
    let source = fixture(
        Type::Unit,
        roundtrip(Expr::Bytes {
            hex: "00".repeat(32),
        }),
    );
    for (_, code) in modes(&source) {
        accepted(
            run(&code, &list(&[atom(&[])]), FINITE_LIMIT),
            &atom(&[0; 32]),
        );
    }
}

#[test]
fn computed_hash_view_matches_fixed_known_bytes_and_independent_chialisp() {
    // FIPS/SHAVS and independently checked exact byte vectors, including a
    // leading-zero digest. No tested VM supplies the expected digest value.
    let vectors: &[(&[u8], &str)] = &[
        (b"", EMPTY_HASH),
        (b"abc", ABC_HASH),
        (
            b"\0",
            "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
        ),
        (
            b"\0\0",
            "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7",
        ),
        (
            b"\0\\",
            "00bc4b668ccce3ae825dbc18d6ff1dcdd9ec14b284c4f161fa5cfbb2d5069534",
        ),
    ];
    let source = program(
        Environment::Strict,
        vec![Type::Atom],
        Type::Atom,
        roundtrip(hash(arg(0))),
        vec![],
    );
    let code = modes(&source);
    let reference = chialisp_oxide_reference::compile("(mod (PAYLOAD) (sha256 PAYLOAD))").unwrap();
    for (payload, digest) in vectors {
        let expected = atom(&hex::decode(digest).unwrap());
        let input = list(&[blob(payload)]);
        accepted(run(&reference, &input, FINITE_LIMIT), &expected);
        for (_, code) in &code {
            accepted(run(code, &input, FINITE_LIMIT), &expected);
        }
    }
}

#[test]
fn unused_constructed_digest_still_hashes_and_resource_failure_is_not_rejection() {
    let source = program(
        Environment::Strict,
        vec![Type::Atom],
        u8_type(),
        sequence(roundtrip(hash(arg(0))), int(7)),
        vec![],
    );
    let code = modes(&source);
    for (_, code) in &code {
        accepted(run(code, &list(&[atom(b"abc")]), FINITE_LIMIT), &uint(7));
        // SHA's byte cost alone exceeds this finite budget; a totality error
        // that drops the unused digest would incorrectly return7 here.
        failed(
            run(code, &list(&[blob(&vec![0x78; 65_536])]), LIMIT),
            FailureCode::CostExceeded,
        );
        abi_rejected(run(code, &list(&[pair(&uint(1), &uint(2))]), FINITE_LIMIT));
    }
}

#[test]
fn constructor_omission_retains_reject_spin_and_discarded_sibling_order() {
    let call_spin = call(0, vec![]);
    let raise_then_spin = sequence(bottom_atom(), call_spin.clone());
    let spin_then_raise = sequence(call_spin.clone(), bottom_atom());
    let rejecting_sibling = field(
        Expr::Tuple {
            items: vec![atom_expr(&[0, 1]), bottom_atom()],
        },
        0,
    );
    let early_reject_sibling = field(
        Expr::Tuple {
            items: vec![bottom_atom(), call_spin.clone()],
        },
        0,
    );
    let early_spin_sibling = field(
        Expr::Tuple {
            items: vec![call_spin, bottom_atom()],
        },
        1,
    );
    for (value, expected) in [
        (raise_then_spin, FailureCode::Raise),
        (spin_then_raise, FailureCode::CostExceeded),
        (rejecting_sibling, FailureCode::Raise),
        (early_reject_sibling, FailureCode::Raise),
        (early_spin_sibling, FailureCode::CostExceeded),
    ] {
        let source = program(
            Environment::Ignored,
            vec![],
            Type::Atom,
            roundtrip(value),
            vec![spin(&Type::Atom)],
        );
        for (_, code) in modes(&source) {
            failed(run(&code, &atom(&[]), LIMIT), expected);
        }
    }
}

#[test]
fn computed_callee_runs_once_and_matches_a_direct_unwrapped_control() {
    let helper = function(0, vec![Type::Atom], digest_type(), hash(arg(0)));
    let invocation = call(0, vec![arg(0)]);
    let source = program(
        Environment::Strict,
        vec![Type::Atom],
        Type::Atom,
        roundtrip(invocation.clone()),
        vec![helper.clone()],
    );
    let direct = program(
        Environment::Strict,
        vec![Type::Atom],
        digest_type(),
        invocation.clone(),
        vec![helper.clone()],
    );
    // The controls share their actual function body, table and input. Changing
    // only the transparent atom view must generate the same optimized program,
    // not a scalar guard that evaluates the expensive callee twice.
    assert_eq!(
        compile(&source, true).unwrap(),
        compile(&direct, true).unwrap()
    );
    let twice = program(
        Environment::Strict,
        vec![Type::Atom],
        Type::Atom,
        binding(
            0,
            digest_type(),
            invocation.clone(),
            sequence(invocation, roundtrip(local(0))),
        ),
        vec![helper],
    );
    let payload: Vec<u8> = (0..8192).map(|index| (17 * index + 3) as u8).collect();
    let input = list(&[blob(&payload)]);
    let expected = atom(
        &hex::decode("0a44a320571de174033d2e1153da8fa45b6a5ddd7fc091ccf5c90e06db556c02").unwrap(),
    );
    for ((mode, once), (_, twice)) in modes(&source).into_iter().zip(modes(&twice)) {
        let one_cost = accepted(run(&once, &input, FINITE_LIMIT), &expected);
        let two_cost = accepted(run(&twice, &input, FINITE_LIMIT), &expected);
        assert!(
            one_cost < two_cost,
            "mode={mode}: once={one_cost}, twice={two_cost}"
        );
        accepted(run(&once, &input, two_cost - 1), &expected);
        failed(run(&twice, &input, two_cost - 1), FailureCode::CostExceeded);
    }
}

#[test]
fn plain_tree_locals_and_malformed_external_abis_keep_their_checks() {
    let source = program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Atom,
        binding(0, Type::Tree, arg(0), cast(local(0))),
        vec![],
    );
    for (_, code) in modes(&source) {
        for payload in [&[][..], &[0], &[0, 0], &[0, 1], &[0xff, 0x80]] {
            accepted(run(&code, &blob(payload), FINITE_LIMIT), &blob(payload));
        }
        // No actual TreeAtom constructor was evaluated here: this cast remains
        // checked even when the input has been bound in an immutable local.
        failed(
            run(&code, &pair(&uint(1), &uint(2)), FINITE_LIMIT),
            FailureCode::Raise,
        );
    }
    for ty in [Type::Atom, Type::Bytes { len: 32 }] {
        let source = fixture(ty, roundtrip(arg(0)));
        for (_, code) in modes(&source) {
            for malformed in [
                atom(&[]),
                atom(&[0]),
                pair(&atom(&[1]), &atom(&[0])),
                list(&[pair(&uint(1), &uint(2))]),
                list(&[atom(&[]), atom(&[])]),
            ] {
                abi_rejected(run(&code, &malformed, FINITE_LIMIT));
            }
        }
    }
    // No new unchecked operation or dead invalid TreeAtom operand is admitted.
    assert!(
        serde_json::from_value::<Expr>(
            serde_json::json!({"op":"unchecked_atom","value":{"op":"arg","index":0}})
        )
        .is_err()
    );
    let mut invalid = fixture(Type::Unit, roundtrip(Expr::Bool { value: true }));
    assert!(invalid.validate().is_err());
    invalid.body = branch(Expr::Bool { value: false }, invalid.body, atom_expr(&[]));
    assert!(compile(&invalid, true).is_err());
}
