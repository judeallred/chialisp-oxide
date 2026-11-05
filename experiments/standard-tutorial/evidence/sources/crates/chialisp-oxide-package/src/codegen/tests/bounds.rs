use super::*;

#[test]
fn every_fixed_configuration_cap_rejects_raising_and_overflowing_values() {
    let mut variants = Vec::new();
    macro_rules! raise {
        ($field:ident) => {{
            let mut limits = GenerationLimits::default();
            limits.$field += 1;
            variants.push(limits);
            let mut limits = GenerationLimits::default();
            limits.$field = usize::MAX;
            variants.push(limits);
        }};
    }
    raise!(max_artifact_bytes);
    raise!(max_expanded_nodes);
    raise!(max_pairs);
    raise!(max_data_depth_root_one);
    raise!(max_unique_atoms);
    raise!(max_atom_payload_bytes);
    raise!(max_total_unique_atom_payload_bytes);
    raise!(max_live_pair_references);
    raise!(pairs_per_helper);
    raise!(max_construction_helpers);
    raise!(max_source_bytes);
    raise!(max_generation_work_units);
    for limits in variants {
        assert_kind(limits.validate(), ErrorKind::InvalidManifest);
    }
    let limits = GenerationLimits {
        pairs_per_helper: 0,
        ..GenerationLimits::default()
    };
    assert_kind(limits.validate(), ErrorKind::InvalidManifest);
    assert_kind(
        super::super::limits::add(usize::MAX, 1, "test overflow"),
        ErrorKind::ResourceLimit,
    );
    let mut work = Work::new(usize::MAX);
    work.charge(usize::MAX).unwrap();
    assert_kind(work.charge(1), ErrorKind::ResourceLimit);
    let mut impossible: Vec<u8> = Vec::new();
    assert_kind(
        super::super::limits::reserve(&mut impossible, usize::MAX, "capacity control"),
        ErrorKind::ResourceLimit,
    );
    assert!(impossible.is_empty());
}

#[test]
fn exact_measured_shape_and_output_limits_pass_and_one_under_fails() {
    let bytes = pair(
        &pair(&atom(&[0, 0x80]), &atom(&[0xff, 0xfe, 0])),
        &pair(&[1], &[0x80]),
    );
    let baseline = generated(&bytes, &GenerationLimits::default()).unwrap();
    let m = &baseline.record().metrics;
    macro_rules! boundary {
        ($field:ident, $value:expr) => {{
            let value = usize::try_from($value).unwrap();
            assert!(value > 0);
            let limits = GenerationLimits {
                $field: value,
                ..GenerationLimits::default()
            };
            let exact = generated(&bytes, &limits).unwrap();
            assert_eq!(exact.source(), baseline.source());
            let limits = GenerationLimits {
                $field: value - 1,
                ..GenerationLimits::default()
            };
            assert_kind(generated(&bytes, &limits), ErrorKind::ResourceLimit);
        }};
    }
    boundary!(max_artifact_bytes, m.artifact_bytes);
    boundary!(max_expanded_nodes, m.expanded_nodes);
    boundary!(max_pairs, m.pairs);
    boundary!(max_data_depth_root_one, m.data_depth_root_one);
    boundary!(max_unique_atoms, m.unique_atoms);
    boundary!(max_atom_payload_bytes, m.max_atom_payload_bytes);
    boundary!(
        max_total_unique_atom_payload_bytes,
        m.total_unique_atom_payload_bytes
    );
    boundary!(max_live_pair_references, m.max_live_pair_references);
    boundary!(max_construction_helpers, m.construction_helpers);
    boundary!(max_source_bytes, baseline.record().source_bytes);
    boundary!(max_generation_work_units, m.generation_work_units);
}

#[test]
fn payloads_255_and_256_are_verbatim_and_257_is_not_split_or_normalized() {
    for length in [255, 256] {
        let payload: Vec<_> = (0..length).map(|i| (i % 256) as u8).collect();
        let generated = generated(&atom(&payload), &GenerationLimits::default()).unwrap();
        assert_eq!(
            generated.record().metrics.max_atom_payload_bytes,
            length as u64
        );
        assert_eq!(generated.source().matches("const ATOM_").count(), 1);
        assert!(generated.source().contains("0xff") || length == 255);
    }
    let error = generated(&atom(&vec![0; 257]), &GenerationLimits::default()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::ResourceLimit);
    assert!(
        error
            .to_string()
            .contains("256 bytes; atoms are never split")
    );
}

#[test]
fn frontier_peak_inside_one_chunk_is_checked_even_without_forwarded_parameters() {
    let leaf_pair = pair(&[0x80], &[0x80]);
    let bytes = pair(&leaf_pair, &pair(&leaf_pair, &leaf_pair));
    let generated = generated(&bytes, &GenerationLimits::default()).unwrap();
    assert_eq!(generated.record().metrics.construction_helpers, 1);
    assert_eq!(generated.record().metrics.max_live_pair_references, 3);
    assert!(!generated.source().contains("fn stage_"));
    let limits = GenerationLimits {
        max_live_pair_references: 2,
        ..GenerationLimits::default()
    };
    assert_kind(super::generated(&bytes, &limits), ErrorKind::ResourceLimit);
}

#[test]
fn fixed_pair_node_and_helper_caps_admit_a_bounded_128_pair_tree() {
    // Each of 32 outer links contributes one pair plus a three-pair balanced
    // datum. This hits 128 pairs/257 nodes without hiding a depth/frontier limit.
    let two = pair(&[0x80], &[0x80]);
    let datum = pair(&two, &two);
    let mut bytes = vec![0x80];
    for _ in 0..32 {
        bytes = pair(&bytes, &datum);
    }
    let generated = generated(&bytes, &GenerationLimits::default()).unwrap();
    assert_eq!(
        (
            generated.record().metrics.pairs,
            generated.record().metrics.expanded_nodes,
            generated.record().metrics.construction_helpers
        ),
        (128, 257, 16)
    );
    let too_many = pair(&[0x80], &bytes);
    assert_kind(
        super::generated(&too_many, &GenerationLimits::default()),
        ErrorKind::ResourceLimit,
    );
}

#[test]
fn tightening_pair_chunk_size_stays_bounded_and_can_exceed_helper_eligibility() {
    let mut bytes = vec![0x80];
    for _ in 0..17 {
        bytes = pair(&[0x80], &bytes);
    }
    let limits = GenerationLimits {
        pairs_per_helper: 2,
        ..GenerationLimits::default()
    };
    let generated = generated(&bytes, &limits).unwrap();
    assert_eq!(generated.record().metrics.construction_helpers, 9);
    let limits = GenerationLimits {
        pairs_per_helper: 1,
        ..GenerationLimits::default()
    };
    assert_kind(super::generated(&bytes, &limits), ErrorKind::ResourceLimit);
}

#[test]
fn nil_can_tighten_payload_and_pair_limits_to_zero_but_not_required_shape() {
    let limits = GenerationLimits {
        max_pairs: 0,
        max_live_pair_references: 0,
        max_atom_payload_bytes: 0,
        max_total_unique_atom_payload_bytes: 0,
        ..GenerationLimits::default()
    };
    generated(&[0x80], &limits).unwrap();
    assert_kind(generated(&[0], &limits), ErrorKind::ResourceLimit);
    assert_kind(
        generated(&[0xff, 0x80, 0x80], &limits),
        ErrorKind::ResourceLimit,
    );
    for limits in [
        GenerationLimits {
            max_expanded_nodes: 0,
            ..GenerationLimits::default()
        },
        GenerationLimits {
            max_unique_atoms: 0,
            ..GenerationLimits::default()
        },
        GenerationLimits {
            max_data_depth_root_one: 0,
            ..GenerationLimits::default()
        },
    ] {
        assert_kind(generated(&[0x80], &limits), ErrorKind::ResourceLimit);
    }
}
