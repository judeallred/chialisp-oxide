use super::*;
use clvmr::serde::{node_from_bytes_backrefs, node_to_bytes_limit};
use std::str::FromStr;

fn bytes(value: &str) -> Vec<u8> {
    hex::decode(value).unwrap()
}
fn hash(value: &str) -> Digest32 {
    Digest32::from_str(value).unwrap()
}
fn inspect(value: &str) -> VerifiedArtifact {
    inspect_artifact(&bytes(value), Encoding::Plain, &Limits::default()).unwrap()
}
fn fails(value: &[u8], limits: &Limits, kind: ErrorKind, reason: &str) {
    let error = inspect_artifact(value, Encoding::Plain, limits).unwrap_err();
    assert_eq!(error.kind(), kind, "{error}");
    assert!(error.to_string().contains(reason), "{error}");
}

#[test]
fn independent_known_atom_pair_and_program_hashes_are_exact() {
    // Independent SHA256(prefix || bytes/child hashes), never expectations
    // obtained from the implementation under test or an assembled program.
    for (encoded, blob_hash, tree_hash) in [
        (
            "80",
            "76be8b528d0075f7aae98d6fa57a6d3c83ae480a8469e668d7b0af968995ac71",
            "4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a",
        ),
        (
            "00",
            "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
            "47dc540c94ceb704a23875c11273e16bb0b8a87aed84de911f2133568115f254",
        ),
        (
            "01",
            "4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a",
            "9dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2",
        ),
        (
            "8180",
            "73dafe1a9ee2a5640d38482df68ee062b7a5b524fa08bc1a382dacb7f2dfe3ee",
            "3be90d393f91241448d7dceadad32d91c1c94f307805937b46ed01ea669c17c3",
        ),
        (
            "ff8080",
            "25b9fe337804b7cb4d3c30ed55c3f0ab0982915c0670cc0cc4b6e36527df914d",
            "52db9ef97986e7382ef78b8eae2dacdbb2ce823ed1396a0fb2f7f120a2b40a63",
        ),
        (
            "ff0180",
            "2a94811ae46f4cc43946f4c7c2b61a1b18b471a9963a50943360e2659052812a",
            "ba4484b961b7a2369d948d06c55b64bdbfaffb326bc13b490ab1215dd33d8d46",
        ),
    ] {
        let raw = bytes(encoded);
        let expected = ArtifactIdentity {
            encoding: Encoding::Plain,
            bytes: raw.len() as u64,
            blob_sha256: hash(blob_hash),
            module_tree_hash: hash(tree_hash),
        };
        let result = verify_artifact(&raw, &expected, &Limits::default()).unwrap();
        assert_eq!(result.identity(), &expected);
        assert_eq!(result.bytes(), raw);
        assert_eq!(result.stats().expanded_bytes, raw.len() as u64);
    }
    assert_ne!(
        inspect("80").identity().module_tree_hash,
        inspect("00").identity().module_tree_hash
    );
}

#[test]
fn real_published_standard_module_matches_independently_pinned_bytes_and_hashes() {
    // chia-puzzles 0.20.3 src/programs.rs:3777, generated standard module.
    // Crate checksum 553363ce9550cfde0ae1306a0a79bf9afd36ea09e0222b874b6287b44ad9d178.
    // This is artifact identity, not a mainnet reveal observation or a spend.
    let raw = bytes(concat!(
        "ff02ffff01ff02ffff03ff0bffff01ff02ffff03ffff09ff05ffff1dff0bffff1effff0bff0bffff02ff06ffff04ff02ffff04ff17ff8080808080808080",
        "ffff01ff02ff17ff2f80ffff01ff088080ff0180ffff01ff04ffff04ff04ffff04ff05ffff04ffff02ff06ffff04ff02ffff04ff17ff80808080ff80808080",
        "ffff02ff17ff2f808080ff0180ffff04ffff01ff32ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff06ffff04ff02ffff04ff09ff80808080",
        "ffff02ff06ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080"
    ));
    assert_eq!(raw.len(), 227);
    let expected = ArtifactIdentity {
        encoding: Encoding::Plain,
        bytes: 227,
        blob_sha256: hash("21747ef070376012f22f7b426edaa36a244a77f8825d13d5529963b921ad1da7"),
        module_tree_hash: hash("e9aaa49f45bad5c889b86ee3341550c155cfdd10c3a6757de618d20612fffd52"),
    };
    assert_eq!(
        verify_artifact(&raw, &expected, &Limits::default())
            .unwrap()
            .identity(),
        &expected
    );
}

#[test]
fn alternate_serialization_same_tree_does_not_become_a_blob_alias_or_silent_fallback() {
    // Exact paired upstream clvmr0.21 serde/de_br.rs test_deserialize_with_backrefs
    // fixtures: ("foobar" "foobar"). Only the plain form is admitted here.
    let plain = bytes("ff86666f6f626172ff86666f6f62617280");
    let compressed = bytes("ff86666f6f626172fe01");
    let expected_tree = hash("9148834131750904c023598bed28db269bdb29012514579e723d63e27829bcba");
    let result = inspect_artifact(&plain, Encoding::Plain, &Limits::default()).unwrap();
    assert_eq!(result.identity().module_tree_hash, expected_tree);
    assert_eq!(
        result.identity().blob_sha256,
        hash("272ff3cc2ad77c6fce87434c4046919e4897dd97d745e814ac4c4c5340feec01")
    );
    assert_eq!(
        digest(&compressed),
        hash("18012b867b5256f8d11087fdd65a268a9ad6adaa4fde3ae12b21261cd0798f41")
    );
    assert_ne!(digest(&compressed), result.identity().blob_sha256);
    // Establish this really is a valid equivalent serialization with the pinned
    // independent decoder, rather than accepting an arbitrary unsupported blob.
    let mut allocator = Allocator::new_limited(1024);
    let node = node_from_bytes_backrefs(&mut allocator, &compressed).unwrap();
    assert_eq!(node_to_bytes_limit(&allocator, node, 1024).unwrap(), plain);
    fails(
        &compressed,
        &Limits::default(),
        ErrorKind::UnsupportedEncoding,
        "backreferences",
    );
    let mut identity = result.identity().clone();
    identity.bytes = compressed.len() as u64;
    identity.blob_sha256 = digest(&compressed);
    assert_eq!(
        verify_artifact(&compressed, &identity, &Limits::default())
            .unwrap_err()
            .kind(),
        ErrorKind::UnsupportedEncoding
    );
}

#[test]
fn marker_bytes_and_padded_integer_atoms_are_preserved_as_opaque_payloads() {
    for raw in [
        "82feff",
        "86fdff32303236",
        "820001",
        "82ffff",
        "820000",
        "ff0102",
    ] {
        let result = inspect(raw);
        assert_eq!(result.bytes(), bytes(raw));
        assert_eq!(result.identity().blob_sha256, digest(&bytes(raw)));
    }
    assert_ne!(
        inspect("820001").identity().module_tree_hash,
        inspect("01").identity().module_tree_hash
    );
    let improper = inspect("ff0102");
    assert_eq!(
        (
            improper.stats().nodes,
            improper.stats().pairs,
            improper.stats().atoms,
            improper.stats().depth
        ),
        (3, 1, 2, 2)
    );
}

#[test]
fn noncanonical_atom_headers_are_rejected_with_their_own_kind() {
    for raw in [
        "8100",
        "8101",
        "817f",
        "c000",
        "c00101",
        "c03f",
        "e00000",
        "e01fff",
        "f0000000",
        "f800000000",
    ] {
        fails(
            &bytes(raw),
            &Limits::default(),
            ErrorKind::NonCanonicalSerialization,
            "CLVM atom",
        );
    }
}

#[test]
fn empty_truncated_oversized_and_trailing_serializations_are_malformed() {
    for raw in [
        "",
        "ff",
        "ff80",
        "81",
        "8180ff",
        "c0",
        "c040",
        "e0",
        "fc0400000000",
        "fd0000",
        "8080",
        "ff808080",
        "ff018000",
        "83aabb",
    ] {
        fails(
            &bytes(raw),
            &Limits::default(),
            ErrorKind::MalformedSerialization,
            "CLVM",
        );
    }
    // A correctly supplied SHA of a malicious suffix does not authorize parsing
    // only the first object, even when its tree commitment names that prefix.
    let mut expected = inspect("80").identity().clone();
    let raw = bytes("8080");
    expected.bytes = 2;
    expected.blob_sha256 = digest(&raw);
    assert_eq!(
        verify_artifact(&raw, &expected, &Limits::default())
            .unwrap_err()
            .kind(),
        ErrorKind::MalformedSerialization
    );
}

#[test]
fn unsupported_formats_at_tokens_remain_explicit_even_when_nested() {
    for raw in [
        "fe",
        "fe01",
        "ff01fe02",
        "ff01fffe1080",
        "fdff32303236",
        "fffdff3230323680",
    ] {
        fails(
            &bytes(raw),
            &Limits::default(),
            ErrorKind::UnsupportedEncoding,
            "unsupported",
        );
    }
}

#[test]
fn exact_blob_node_depth_atom_and_expansion_boundaries_are_not_raised() {
    let pair = bytes("ff8080");
    let mut limits = Limits {
        max_blob_bytes: 3,
        ..Limits::default()
    };
    inspect_artifact(&pair, Encoding::Plain, &limits).unwrap();
    limits.max_blob_bytes = 2;
    fails(&pair, &limits, ErrorKind::ResourceLimit, "blob bytes");
    limits = Limits {
        max_nodes: 3,
        ..Limits::default()
    };
    inspect_artifact(&pair, Encoding::Plain, &limits).unwrap();
    limits.max_nodes = 2;
    fails(&pair, &limits, ErrorKind::ResourceLimit, "nodes");
    limits = Limits {
        max_depth: 2,
        ..Limits::default()
    };
    inspect_artifact(&pair, Encoding::Plain, &limits).unwrap();
    limits.max_depth = 1;
    fails(&pair, &limits, ErrorKind::ResourceLimit, "depth");
    limits = Limits {
        max_atom_bytes: 2,
        ..Limits::default()
    };
    inspect_artifact(&bytes("82feff"), Encoding::Plain, &limits).unwrap();
    limits.max_atom_bytes = 1;
    fails(
        &bytes("82feff"),
        &limits,
        ErrorKind::ResourceLimit,
        "atom bytes",
    );
    limits = Limits {
        max_expanded_bytes: 3,
        ..Limits::default()
    };
    inspect_artifact(&pair, Encoding::Plain, &limits).unwrap();
    limits.max_expanded_bytes = 2;
    fails(&pair, &limits, ErrorKind::ResourceLimit, "expanded bytes");
}

#[test]
fn heap_accounting_includes_all_nonfast_atoms_and_initial_ghost_byte() {
    let one_byte_heap = Limits {
        max_atom_heap_bytes: 1,
        ..Limits::default()
    };
    for raw in ["80", "01", "ff01ff8080"] {
        let value = inspect_artifact(&bytes(raw), Encoding::Plain, &one_byte_heap).unwrap();
        assert_eq!(value.stats().allocator_atom_heap_bytes, 1);
    }
    for raw in ["00", "02"] {
        let two_byte_heap = Limits {
            max_atom_heap_bytes: 2,
            ..Limits::default()
        };
        assert_eq!(
            inspect_artifact(&bytes(raw), Encoding::Plain, &two_byte_heap)
                .unwrap()
                .stats()
                .allocator_atom_heap_bytes,
            2
        );
        fails(
            &bytes(raw),
            &one_byte_heap,
            ErrorKind::ResourceLimit,
            "atom heap bytes",
        );
    }
    // Repeated equal small atoms each charge heap, unlike nil/one fast paths.
    let raw = bytes("ff02ff0280");
    let three_byte_heap = Limits {
        max_atom_heap_bytes: 3,
        ..Limits::default()
    };
    assert_eq!(
        inspect_artifact(&raw, Encoding::Plain, &three_byte_heap)
            .unwrap()
            .stats()
            .allocator_atom_heap_bytes,
        3
    );
    let two_byte_heap = Limits {
        max_atom_heap_bytes: 2,
        ..Limits::default()
    };
    fails(
        &raw,
        &two_byte_heap,
        ErrorKind::ResourceLimit,
        "atom heap bytes",
    );
    fails(
        &[0x80],
        &Limits {
            max_atom_heap_bytes: 0,
            ..Limits::default()
        },
        ErrorKind::ResourceLimit,
        "atom heap bytes",
    );
    #[cfg(target_pointer_width = "64")]
    fails(
        &[0x80],
        &Limits {
            max_atom_heap_bytes: u32::MAX as usize + 1,
            ..Limits::default()
        },
        ErrorKind::ResourceLimit,
        "configuration",
    );
}

#[test]
fn exact_recorded_work_is_bounded_through_decoder_hash_and_owned_copy() {
    let raw = bytes("ffff82feff01ff82000180");
    let expected = inspect_artifact(&raw, Encoding::Plain, &Limits::default()).unwrap();
    let exact = Limits {
        max_work: expected.stats().work as usize,
        ..Limits::default()
    };
    let actual = inspect_artifact(&raw, Encoding::Plain, &exact).unwrap();
    assert_eq!(actual.stats(), expected.stats());
    let short = Limits {
        max_work: exact.max_work - 1,
        ..Limits::default()
    };
    fails(&raw, &short, ErrorKind::ResourceLimit, "work");
    for limits in [
        Limits {
            max_nodes: 0,
            ..Limits::default()
        },
        Limits {
            max_depth: 0,
            ..Limits::default()
        },
        Limits {
            max_work: 0,
            ..Limits::default()
        },
    ] {
        fails(&[0x80], &limits, ErrorKind::ResourceLimit, "limit exceeded");
    }
}

#[test]
fn declared_byte_blob_and_tree_commitments_are_checked_separately() {
    let raw = bytes("ff0180");
    let expected = inspect_artifact(&raw, Encoding::Plain, &Limits::default())
        .unwrap()
        .identity()
        .clone();
    for field in ["bytes", "blob", "tree"] {
        let mut changed = expected.clone();
        match field {
            "bytes" => changed.bytes += 1,
            "blob" => changed.blob_sha256 = Digest32::new([0; 32]),
            "tree" => changed.module_tree_hash = Digest32::new([0; 32]),
            _ => unreachable!(),
        }
        let error = verify_artifact(&raw, &changed, &Limits::default()).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::DigestMismatch, "{error}");
    }
    // The tree hash of nil coincides with SHA256 of byte 01; these digest types
    // do not turn equal-looking hashes across domains into interchangeable claims.
    assert_eq!(
        inspect("80").identity().module_tree_hash,
        inspect("01").identity().blob_sha256
    );
    assert_ne!(
        inspect("80").identity().blob_sha256,
        inspect("01").identity().blob_sha256
    );
}

fn spine(pairs: usize, left: bool) -> Vec<u8> {
    let mut result = Vec::with_capacity(2 * pairs + 1);
    if left {
        result.extend(std::iter::repeat_n(0xff, pairs));
        result.extend(std::iter::repeat_n(0x80, pairs + 1));
    } else {
        for _ in 0..pairs {
            result.extend_from_slice(&[0xff, 0x80]);
        }
        result.push(0x80);
    }
    result
}

#[test]
fn deep_left_and_right_inputs_use_iterative_scans_decoder_and_hashing() {
    let limits = Limits::default();
    for left in [false, true] {
        let raw = spine(limits.max_depth - 1, left);
        let result = inspect_artifact(&raw, Encoding::Plain, &limits).unwrap();
        assert_eq!(result.stats().depth, limits.max_depth as u64);
        assert_eq!(result.stats().nodes, (2 * limits.max_depth - 1) as u64);
        let beyond = spine(limits.max_depth, left);
        fails(&beyond, &limits, ErrorKind::ResourceLimit, "depth");
    }
}

#[test]
fn unfinished_prefix_and_many_nil_nodes_fail_resource_preflight_before_decoder() {
    let depth = Limits {
        max_depth: 32,
        ..Limits::default()
    };
    fails(&[0xff; 4096], &depth, ErrorKind::ResourceLimit, "depth");
    let nodes = Limits {
        max_nodes: 64,
        ..Limits::default()
    };
    fails(&spine(64, false), &nodes, ErrorKind::ResourceLimit, "nodes");
    // Even an otherwise malformed oversized input is not read or hashed past
    // the caller's input cap merely to produce a more specific parser error.
    let blob = Limits {
        max_blob_bytes: 8,
        ..Limits::default()
    };
    fails(&[0xff; 9], &blob, ErrorKind::ResourceLimit, "blob bytes");
}

#[test]
fn canonical_atom_length_transitions_and_noncanonical_padded_headers() {
    // Independent explicit shortest-serialization length boundaries.
    for (length, header) in [(63, "bf"), (64, "c040"), (8191, "dfff"), (8192, "e02000")] {
        let mut raw = bytes(header);
        raw.extend(std::iter::repeat_n(0xfe, length));
        let result = inspect_artifact(&raw, Encoding::Plain, &Limits::default()).unwrap();
        assert_eq!(result.stats().atom_bytes, length as u64);
        assert_eq!(result.stats().nodes, 1);
        assert_eq!(result.bytes(), raw);
        assert_eq!(result.stats().allocator_atom_heap_bytes, length as u64 + 1);
    }
    let mut overlong = bytes("c03f");
    overlong.extend([0xfe; 63]);
    fails(
        &overlong,
        &Limits::default(),
        ErrorKind::NonCanonicalSerialization,
        "nonminimal",
    );
}

#[test]
fn identity_wire_encoding_is_closed_and_preserves_separate_hash_fields() {
    let expected = inspect("ff0180").identity().clone();
    let mut value = serde_json::to_value(&expected).unwrap();
    assert_eq!(value["encoding"], "clvm-legacy-plain/v1");
    assert_eq!(
        serde_json::from_value::<ArtifactIdentity>(value.clone()).unwrap(),
        expected
    );
    value["trusted"] = true.into();
    assert!(serde_json::from_value::<ArtifactIdentity>(value).is_err());
    for invalid in [
        "clvm-legacy-backrefs/v1",
        "serde2026",
        "plain",
        "clvm-legacy-plain/v2",
    ] {
        let mut value = serde_json::to_value(&expected).unwrap();
        value["encoding"] = invalid.into();
        assert!(serde_json::from_value::<ArtifactIdentity>(value).is_err());
    }
}
