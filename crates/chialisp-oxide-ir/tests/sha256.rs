//! Finite backend-IR regressions for SHA256, not Rust source admission or a
//! compiler/cryptographic preservation proof. Hash expectations are fixed known
//! vectors or independent Python/OpenSSL SHA256 results, never the tested VM.
#[path = "runtime_evaluation/support.rs"]
#[allow(
    dead_code,
    reason = "Shared fixtures also exercise unrelated IR operations."
)]
mod support;

use chialisp_oxide_ir::{Environment, Expr, Program, Type, compile};
use chialisp_oxide_vm::{FailureCode, FailureKind};
use support::*;

const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
fn digest_type() -> Type {
    Type::Bytes { len: 32 }
}
fn hash(parts: Vec<Expr>) -> Expr {
    Expr::Sha256 { parts }
}
fn bytes(value: &[u8]) -> Expr {
    Expr::Bytes {
        hex: hex::encode(value),
    }
}
fn arbitrary(value: &[u8]) -> Expr {
    Expr::Atom {
        hex: hex::encode(value),
    }
}
fn bottom() -> Expr {
    Expr::Reject {
        message: "required hash operand".into(),
    }
}
fn fixture(types: Vec<Type>, body: Expr) -> Program {
    program(Environment::Strict, types, digest_type(), body, vec![])
}
fn expected(hex: &str) -> Vec<u8> {
    let value = hex::decode(hex).unwrap();
    assert_eq!(value.len(), 32);
    atom(&value)
}
// Independent serializer with long-atom support; expected digest serialization
// still uses the pre-existing small-atom constructor.
fn blob(value: &[u8]) -> Vec<u8> {
    let n = value.len();
    if n < 64 {
        return atom(value);
    }
    let mut out = if n < 0x2000 {
        vec![0xc0 | (n >> 8) as u8, n as u8]
    } else {
        assert!(n < 0x10_0000);
        vec![0xe0 | (n >> 16) as u8, (n >> 8) as u8, n as u8]
    };
    out.extend(value);
    out
}
fn reference(arity: usize) -> Vec<u8> {
    let names = (0..arity)
        .map(|i| format!("A{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    chialisp_oxide_reference::compile(&format!("(mod ({names}) (sha256 {names}))")).unwrap()
}
fn output(code: &[u8], environment: &[u8]) -> Vec<u8> {
    match run(code, environment, FINITE_LIMIT) {
        Outcome::Accepted { bytes, .. } => bytes,
        other => panic!("expected conclusive exact bytes, got {other:?}"),
    }
}

#[test]
fn known_vectors_and_zero_to_many_multipart_partitions_match_independent_chialisp() {
    // SHA-256 examples from FIPS 180-4/SHAVS. No text/hex/numeric conversion is
    // part of the hashing contract; each part contributes its exact payload.
    let long = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
    let longest = b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu";
    let cases: Vec<(Vec<&[u8]>, &str)> = vec![
        (vec![], EMPTY),
        (vec![b""], EMPTY),
        (vec![b"abc"], ABC),
        (vec![b"a", b"bc"], ABC),
        (vec![b"a", b"b", b"c"], ABC),
        (vec![b"", b"a", b"", b"b", b"", b"c", b""], ABC),
        (
            vec![long],
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
        (
            vec![longest],
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
        ),
        (
            vec![b"\0"],
            "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
        ),
        (
            vec![b"\0\0"],
            "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7",
        ),
        (
            vec![b"\0\x01"],
            "b413f47d13ee2fe6c845b2ee141af81de858df4ec549a58b7970bb96645bc8d2",
        ),
        (
            vec![b"\xff\x80\xfe\0"],
            "02d5067db6f13e15941cdec9c05203c06094964bdf5aee1827267826de541cfb",
        ),
        // Leading zero output must remain a 32-byte atom, not an integer.
        (
            vec![b"\0\\"],
            "00bc4b668ccce3ae825dbc18d6ff1dcdd9ec14b284c4f161fa5cfbb2d5069534",
        ),
    ];
    for (parts, digest) in cases {
        let p = fixture(
            vec![Type::Atom; parts.len()],
            hash((0..parts.len()).map(arg).collect()),
        );
        let environment = list(&parts.iter().map(|part| blob(part)).collect::<Vec<_>>());
        let wanted = expected(digest);
        accepted(
            run(&reference(parts.len()), &environment, FINITE_LIMIT),
            &wanted,
        );
        for (_, code) in modes(&p) {
            accepted(run(&code, &environment, FINITE_LIMIT), &wanted);
        }
    }
    // The IR accepts 64 parts, but the current combination of 64 decoded
    // parameters and 64 strict hash bindings exceeds codegen's independent
    // global nesting budget. Nominal IR arity is not a codegen guarantee.
    let largest = fixture(vec![Type::Atom; 64], hash((0..64).map(arg).collect()));
    largest.validate().unwrap();
    let environment = list(&vec![atom(&[]); 64]);
    accepted(
        run(&reference(64), &environment, FINITE_LIMIT),
        &expected(EMPTY),
    );
    for optimize in [false, true] {
        assert_eq!(
            compile(&largest, optimize).unwrap_err().to_string(),
            "CLVM expansion nesting limit"
        );
    }
}

#[test]
fn sha_padding_boundaries_and_large_atoms_match_fixed_independent_hashes() {
    // Generated once with Python hashlib/OpenSSL for bytes (17*i+3) mod 256.
    // These constants keep the expected algorithm independent of CLVM/Rue.
    let vectors = [
        (0, EMPTY),
        (
            1,
            "084fed08b978af4d7d196a7446a86b58009e636b611db16211b65a9aadff29c5",
        ),
        (
            2,
            "44224029cde5cc30babca408bbe4ad68aa5a92f2f1337f66d2ad313ecabcd242",
        ),
        (
            31,
            "64cb82bc3829555059e7e7d0cbc070bb370ca3cefc671e919a51086294f842ff",
        ),
        (
            32,
            "ba6cd1a2ce6f9e35b43c2b1df39404331d3c3bbececef613d02fcfacec8a0dee",
        ),
        (
            54,
            "b049958c18299b973766328ea0198d5a131af151d2e799f2a646ebd5de3604ba",
        ),
        (
            55,
            "c40dbcd18dfdb34a5c52a791f09286986b5e9d61f867278c03adda227684e897",
        ),
        (
            56,
            "ca6c4804e2170c368f94d2da606adf4552eb0b1d35bc2ad1dc12887627a028eb",
        ),
        (
            57,
            "6f622dcd0a7f01cc588f376ac71fd2d8e787dba4b101d0006a9c512212ecbe7c",
        ),
        (
            63,
            "7f963102afccacb03458e0e3ee8f51a234359aed99cb7ebfd33d5e47a095fcba",
        ),
        (
            64,
            "30a1b375b49e0f1d5e00b6868bbfd471cbe5387806456cc11fd153c689bfe747",
        ),
        (
            65,
            "ce4c5c9097270aaca5630ec6ca4b48c145a1bb24462589a0ec3f69cd5c9b2a5c",
        ),
        (
            111,
            "6ca3914f516aa855512bc3b6b3f99e7696a4b820717aa64dc547df0d6cb22f70",
        ),
        (
            112,
            "de87a637b8ab2f08031246ec0d0a7b45dc3efef30e55629b20a251b29b70110f",
        ),
        (
            119,
            "c9b7b15a882bf470dc6a27daf5486f9222f0ec5193069462d1ae5d729bbcc3eb",
        ),
        (
            120,
            "53416ddca73fa04063bbedf24baf10637cde9f47c447586231115c0cc944645d",
        ),
        (
            127,
            "fc7d260501b641289fe7df7d943df65d93c782f2f3a1e2a4e69c6f763d88f8c5",
        ),
        (
            128,
            "5a878aab98a988c5479b48c00d8e2bef988c946dc648583ffe7ac598d76bacd5",
        ),
        (
            129,
            "a1253e220a0a60177ba22d6f1ae079f6b8f0b8fa598b0adc1390b0d7c81756fa",
        ),
        (
            255,
            "d990656c560f084c02cff84fb818505172b37d27a9c5c5531c26dc453199df82",
        ),
        (
            256,
            "a15c31ac66d64c42626322653083309abb46d605813c04494a1835c6d9170c76",
        ),
        (
            1024,
            "f1305624e9a6d905a63dc5ec25bee729df52db7ce7ff2c25716dd91303e8a699",
        ),
        (
            8192,
            "0a44a320571de174033d2e1153da8fa45b6a5ddd7fc091ccf5c90e06db556c02",
        ),
    ];
    let single = modes(&fixture(vec![Type::Atom], hash(vec![arg(0)])));
    let three = modes(&fixture(
        vec![Type::Atom; 3],
        hash(vec![arg(0), arg(1), arg(2)]),
    ));
    let reference_single = reference(1);
    let reference_three = reference(3);
    for (n, digest) in vectors {
        let data: Vec<u8> = (0..n).map(|i| (i * 17 + 3) as u8).collect();
        let wanted = expected(digest);
        let input = list(&[blob(&data)]);
        for code in [&single[0].1, &single[1].1, &reference_single] {
            accepted(run(code, &input, FINITE_LIMIT), &wanted);
        }
        // Move both split positions across the SHA padding/block boundary.
        for first in [0, 1.min(n), 55.min(n), 64.min(n), n] {
            let second = first + (n - first) / 2;
            let input = list(&[
                blob(&data[..first]),
                blob(&data[first..second]),
                blob(&data[second..]),
            ]);
            for code in [&three[0].1, &three[1].1, &reference_three] {
                accepted(run(code, &input, FINITE_LIMIT), &wanted);
            }
        }
    }
}

#[test]
fn byte_arrays_hash_as_raw_bytes_and_all_declared_abi_inputs_are_checked() {
    for (types, parts, good) in [
        (
            vec![Type::Bytes { len: 3 }],
            vec![arg(0)],
            list(&[blob(b"abc")]),
        ),
        (
            vec![Type::Bytes { len: 0 }, Type::Bytes { len: 3 }],
            vec![arg(0), arg(1)],
            list(&[blob(b""), blob(b"abc")]),
        ),
    ] {
        for (_, code) in modes(&fixture(types, hash(parts))) {
            accepted(run(&code, &good, LIMIT), &expected(ABC));
        }
    }
    for parts in [vec![], vec![arbitrary(b"")]] {
        let p = fixture(vec![Type::Bytes { len: 3 }, Type::Bool], hash(parts));
        for (_, code) in modes(&p) {
            accepted(
                run(&code, &list(&[blob(b"abc"), uint(0)]), LIMIT),
                &expected(EMPTY),
            );
            for malformed in [
                atom(&[]),
                uint(1),
                list(&[blob(b"abc")]),
                list(&[blob(b"abc"), uint(0), uint(1)]),
                pair(&blob(b"abc"), &pair(&uint(0), &blob(b"\0"))),
                list(&[blob(b"ab"), uint(0)]),
                list(&[blob(b"abcd"), uint(0)]),
                list(&[pair(&uint(1), &uint(2)), uint(0)]),
                list(&[blob(b"abc"), blob(b"\0")]),
                list(&[blob(b"abc"), blob(b"\0\x01")]),
                list(&[blob(b"abc"), uint(2)]),
            ] {
                abi_rejected(run(&code, &malformed, LIMIT));
            }
        }
    }
    let atom_input = fixture(vec![Type::Atom], hash(vec![arg(0)]));
    for (_, code) in modes(&atom_input) {
        abi_rejected(run(&code, &list(&[pair(&uint(1), &uint(2))]), LIMIT));
        // The same bytes that are invalid Bool/Int encodings remain valid atoms.
        accepted(
            run(&code, &list(&[blob(b"\0")]), LIMIT),
            &expected("6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d"),
        );
    }
}

#[test]
fn hash_keeps_strict_nullary_and_ignored_environment_policies_distinct() {
    let ignored = program(
        Environment::Ignored,
        vec![],
        digest_type(),
        hash(vec![arbitrary(b"abc")]),
        vec![],
    );
    let strict = fixture(vec![], hash(vec![arbitrary(b"abc")]));
    for (_, code) in modes(&ignored) {
        for environment in [
            atom(&[]),
            blob(b"\0"),
            pair(&uint(1), &uint(2)),
            list(&[uint(8)]),
        ] {
            accepted(run(&code, &environment, LIMIT), &expected(ABC));
        }
    }
    for (_, code) in modes(&strict) {
        accepted(run(&code, &atom(&[]), LIMIT), &expected(ABC));
        for environment in [blob(b"\0"), uint(1), list(&[uint(8)])] {
            abi_rejected(run(&code, &environment, LIMIT));
        }
    }
    let nested = fixture(vec![], hash(vec![hash(vec![arbitrary(b"abc")])]));
    for (_, code) in modes(&nested) {
        accepted(
            run(&code, &atom(&[]), LIMIT),
            &expected("4f8b42c22dd3729b519ba6f68d2da7cc5b2d606d05daed5ad5128cc03e6c6358"),
        );
    }
}

#[test]
fn hash_parts_evaluate_once_in_source_order_before_calls_discarding_and_lazy_branches() {
    for (parts, wanted) in [
        (vec![bottom(), call(0, vec![])], FailureCode::Raise),
        (vec![call(0, vec![]), bottom()], FailureCode::CostExceeded),
    ] {
        let p = program(
            Environment::Ignored,
            vec![],
            digest_type(),
            hash(parts),
            vec![spin(&Type::Atom)],
        );
        for (_, code) in modes(&p) {
            failed(run(&code, &atom(&[]), LIMIT), wanted);
        }
    }
    let failing_hash = hash(vec![bottom()]);
    for body in [
        sequence(failing_hash.clone(), bytes(&[0; 32])),
        binding(0, digest_type(), failing_hash.clone(), bytes(&[0; 32])),
        call(0, vec![failing_hash]),
        hash(vec![field(
            Expr::Tuple {
                items: vec![
                    arbitrary(b"abc"),
                    // Aggregate fields need a declared returned type; this
                    // initializer still rejects before Local can be read.
                    binding(7, Type::Atom, bottom(), local(7)),
                ],
            },
            0,
        )]),
    ] {
        let p = program(
            Environment::Ignored,
            vec![],
            digest_type(),
            body,
            vec![function(
                0,
                vec![digest_type()],
                digest_type(),
                bytes(&[0; 32]),
            )],
        );
        for (_, code) in modes(&p) {
            failed(run(&code, &atom(&[]), LIMIT), FailureCode::Raise);
        }
    }
    for condition in [false, true] {
        let active = hash(vec![arbitrary(b"abc")]);
        let dead = hash(vec![call(0, vec![])]);
        let body = if condition {
            branch(Expr::Bool { value: true }, active, dead)
        } else {
            branch(Expr::Bool { value: false }, dead, active)
        };
        let p = program(
            Environment::Ignored,
            vec![],
            digest_type(),
            body,
            vec![spin(&Type::Atom)],
        );
        for (_, code) in modes(&p) {
            accepted(run(&code, &atom(&[]), LIMIT), &expected(ABC));
        }
    }
}

#[test]
fn hash_lexical_bindings_and_callee_arguments_keep_their_own_frames() {
    let helper = function(
        0,
        vec![Type::Atom],
        digest_type(),
        binding(0, Type::Atom, arg(0), hash(vec![local(0), arg(0)])),
    );
    let body = binding(
        0,
        digest_type(),
        hash(vec![]),
        binding(
            1,
            digest_type(),
            call(0, vec![arg(1)]),
            hash(vec![arg(0), local(1), local(0), arg(0)]),
        ),
    );
    let p = program(
        Environment::Strict,
        vec![Type::Atom; 2],
        digest_type(),
        body,
        vec![helper],
    );
    for (_, code) in modes(&p) {
        accepted(
            run(&code, &list(&[blob(b"left"), blob(b"right")]), FINITE_LIMIT),
            &expected("6b1ca1fa9e45a5b675043ba690596b0c561abc34d66ffe9f66115a2fa9f378fb"),
        );
    }
    // A deliberately duplicated source call computes the same digest twice.
    // The Let-bound program must succeed below that measured cost in both modes.
    let helper = function(0, vec![Type::Atom], digest_type(), hash(vec![arg(0)]));
    let once = program(
        Environment::Strict,
        vec![Type::Atom],
        digest_type(),
        binding(
            0,
            digest_type(),
            call(0, vec![arg(0)]),
            hash(vec![local(0), local(0)]),
        ),
        vec![helper.clone()],
    );
    let twice = program(
        Environment::Strict,
        vec![Type::Atom],
        digest_type(),
        hash(vec![call(0, vec![arg(0)]), call(0, vec![arg(0)])]),
        vec![helper],
    );
    let environment = list(&[blob(&vec![0x5a; 8192])]);
    let reference =
        chialisp_oxide_reference::compile("(mod (A) (sha256 (sha256 A) (sha256 A)))").unwrap();
    let wanted = output(&reference, &environment);
    for ((_, one), (_, two)) in modes(&once).into_iter().zip(modes(&twice)) {
        let one_cost = accepted(run(&one, &environment, FINITE_LIMIT), &wanted);
        let two_cost = accepted(run(&two, &environment, FINITE_LIMIT), &wanted);
        assert!(one_cost < two_cost, "one={one_cost}, two={two_cost}");
        accepted(run(&one, &environment, two_cost - 1), &wanted);
        failed(
            run(&two, &environment, two_cost - 1),
            FailureCode::CostExceeded,
        );
    }
}

#[test]
fn unsupported_parts_result_types_dead_code_and_arity_fail_closed() {
    for value in [
        int(1),
        Expr::Bool { value: true },
        tree_atom("00"),
        Expr::Unit,
        Expr::Tuple {
            items: vec![arbitrary(b"a")],
        },
    ] {
        for body in [
            hash(vec![value.clone()]),
            hash(vec![bottom(), value.clone()]),
            hash(vec![value.clone(), bottom()]),
            branch(Expr::Bool { value: false }, hash(vec![value]), hash(vec![])),
        ] {
            let p = fixture(vec![], body);
            assert!(p.validate().is_err());
            assert!(compile(&p, false).is_err());
            assert!(compile(&p, true).is_err());
        }
    }
    let too_many = fixture(vec![], hash(vec![arbitrary(b""); 65]));
    assert!(too_many.validate().is_err());
    for optimize in [false, true] {
        assert!(compile(&too_many, optimize).is_err());
    }
    let mut wrong_result = fixture(vec![], hash(vec![]));
    wrong_result.result = Type::Bytes { len: 31 };
    assert!(wrong_result.validate().is_err());
    let invalid_unused = program(
        Environment::Ignored,
        vec![],
        digest_type(),
        bytes(&[0; 32]),
        vec![function(
            0,
            vec![],
            digest_type(),
            hash(vec![Expr::Bool { value: false }]),
        )],
    );
    assert!(invalid_unused.validate().is_err());
}

#[test]
fn every_hash_uses_the_runtime_envelope_and_legacy_or_unknown_wire_is_rejected() {
    let p = fixture(vec![], hash(vec![]));
    assert!(p.uses_runtime());
    let wire = serde_json::to_value(&p).unwrap();
    assert_eq!(wire["ir_version"], 4);
    assert_eq!(wire["program"]["functions"], serde_json::json!([]));
    let restored: Program = serde_json::from_value(wire.clone()).unwrap();
    for (_, code) in modes(&restored) {
        accepted(run(&code, &atom(&[]), LIMIT), &expected(EMPTY));
    }
    let mut fields = wire["program"].clone();
    fields.as_object_mut().unwrap().remove("functions");
    assert!(serde_json::from_value::<Program>(fields.clone()).is_err());
    for (version, environment) in [(2, "ignored"), (3, "raw"), (5, "strict")] {
        let mut old =
            serde_json::json!({"ir_version":version,"environment":environment,"program":fields});
        if environment == "raw" {
            old["program"]["parameters"] = serde_json::json!([{"name":"env","ty":{"kind":"tree"}}]);
        }
        assert!(serde_json::from_value::<Program>(old).is_err());
    }
    let mut unknown = wire;
    unknown["program"]["body"]["op"] = serde_json::json!("sha256_unreviewed");
    assert!(serde_json::from_value::<Program>(unknown).is_err());
    let unused = program(
        Environment::Ignored,
        vec![],
        digest_type(),
        bytes(&[0; 32]),
        vec![function(0, vec![], digest_type(), hash(vec![]))],
    );
    assert!(unused.uses_runtime());
    assert_eq!(serde_json::to_value(unused).unwrap()["ir_version"], 4);
}

#[test]
fn cost_exhaustion_is_a_resource_observation_not_program_rejection() {
    let p = fixture(vec![Type::Atom], hash(vec![arg(0)]));
    for (_, code) in modes(&p) {
        let observed = run(&code, &list(&[blob(&vec![0x5a; 8192])]), 1);
        failed(observed, FailureCode::CostExceeded);
        assert_eq!(FailureCode::CostExceeded.kind(), FailureKind::Resource);
    }
}

#[test]
fn checked_atom_extraction_before_hash_divergence_preserves_its_guard() {
    for extraction_first in [false, true] {
        let extraction = Expr::TreeAtomValue {
            value: Box::new(arg(0)),
        };
        let divergent = call(0, vec![]);
        let parts = if extraction_first {
            vec![extraction, divergent]
        } else {
            vec![divergent, extraction]
        };
        let p = program(
            Environment::Raw,
            vec![Type::Tree],
            digest_type(),
            hash(parts),
            vec![spin(&Type::Atom)],
        );
        for (_, code) in modes(&p) {
            let pair = pair(&uint(1), &uint(2));
            failed(
                run(&code, &pair, LIMIT),
                if extraction_first {
                    FailureCode::Raise
                } else {
                    FailureCode::CostExceeded
                },
            );
            // Nonempty padded zero bytes are still valid atom payloads, so
            // extraction succeeds and the divergent sibling is reached.
            failed(run(&code, &blob(&[0, 0]), LIMIT), FailureCode::CostExceeded);
        }
    }
}

#[test]
fn recursive_raw_tree_hash_matches_exact_published_helper_over_improper_and_large_trees() {
    let atom_value = Expr::TreeAtomValue {
        value: Box::new(arg(0)),
    };
    let recursive = function(
        0,
        vec![Type::Tree],
        digest_type(),
        branch(
            is_pair(arg(0)),
            hash(vec![
                bytes(&[2]),
                call(0, vec![first(arg(0))]),
                call(0, vec![rest(arg(0))]),
            ]),
            hash(vec![bytes(&[1]), atom_value]),
        ),
    );
    let p = program(
        Environment::Raw,
        vec![Type::Tree],
        digest_type(),
        call(0, vec![arg(0)]),
        vec![recursive],
    );
    // Exact helper body from chia_puzzles@cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8,
    // puzzles/sha256tree.clib lines4–10 (Apache-2.0), in a visible raw adapter.
    // This IR regression is not a corpus promotion or a module-identity claim.
    let reference = chialisp_oxide_reference::compile(
        "(mod TREE
  (defun sha256tree
    (TREE)
    (if (l TREE)
        (sha256 2 (sha256tree (f TREE)) (sha256tree (r TREE)))
        (sha256 1 TREE)
    )
  )
  (sha256tree TREE))",
    )
    .unwrap();
    let mut environments = vec![
        atom(&[]),
        blob(b"\0"),
        blob(b"\0\0"),
        blob(b"\0\x01"),
        blob(b"\xff"),
        pair(&blob(b"\0"), &blob(b"\0\x01")),
        list(&[uint(1), uint(2), uint(3)]),
        list(&[uint(8)]),
        pair(&uint(2), &uint(3)),
        blob(&vec![0x80; 8192]),
    ];
    for depth in [0, 1, 2, 8, 32, 128] {
        environments.push(right_spine(depth, &blob(b"\0\x01")));
    }
    let mut layer = vec![atom(&[]), blob(b"\0"), blob(b"\xff")];
    for _ in 0..3 {
        let next: Vec<_> = layer
            .iter()
            .take(3)
            .flat_map(|left| layer.iter().take(3).map(move |right| pair(left, right)))
            .collect();
        environments.extend(next.clone());
        layer = next;
    }
    for (_, code) in modes(&p) {
        accepted(
            run(&code, &atom(&[]), FINITE_LIMIT),
            &expected("4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"),
        );
        // A leading-zero digest must retain its full byte width here too.
        accepted(
            run(&code, &blob(&[0, 0xd9]), FINITE_LIMIT),
            &expected("00e146297686edb230b0e85eb3bbbef74d745dcd89d98469db35aaad8d2e08db"),
        );
        for environment in &environments {
            accepted(
                run(&code, environment, FINITE_LIMIT),
                &output(&reference, environment),
            );
        }
    }
}
