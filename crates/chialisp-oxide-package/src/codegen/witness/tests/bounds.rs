use super::*;

#[test]
fn every_profile_cap_is_tighten_only_including_overflowing_values() {
    macro_rules! raise {
        ($field:ident) => {
            for value in [WitnessGenerationLimits::default().$field + 1, usize::MAX] {
                let limits = WitnessGenerationLimits {
                    $field: value,
                    ..WitnessGenerationLimits::default()
                };
                assert_kind(limits.validate(), ErrorKind::InvalidManifest);
                assert_kind(generated(&[0x80], &limits), ErrorKind::InvalidManifest);
            }
        };
    }
    raise!(max_artifact_bytes);
    raise!(max_expanded_nodes);
    raise!(max_data_depth_root_one);
    raise!(max_source_bytes);
    raise!(max_generation_work_units);
}

#[test]
fn exact_artifact_source_and_work_bounds_pass_and_one_under_fails() {
    let bytes = pair(
        &pair(&atom(&[0, 0x80]), &atom(&[0xff, 0xfe, 0])),
        &pair(&[1], &[0x80]),
    );
    let baseline = generated(&bytes, &WitnessGenerationLimits::default()).unwrap();
    let metrics = &baseline.record().metrics;
    macro_rules! boundary {
        ($field:ident, $value:expr) => {{
            let value = usize::try_from($value).unwrap();
            assert!(value > 0);
            let limits = WitnessGenerationLimits {
                $field: value,
                ..WitnessGenerationLimits::default()
            };
            let exact = generated(&bytes, &limits).unwrap();
            assert_eq!(exact.source(), baseline.source());
            assert_eq!(exact.record().limits, limits);
            assert_eq!(exact.record().metrics, baseline.record().metrics);
            let limits = WitnessGenerationLimits {
                $field: value - 1,
                ..WitnessGenerationLimits::default()
            };
            assert_kind(generated(&bytes, &limits), ErrorKind::ResourceLimit);
        }};
    }
    boundary!(max_artifact_bytes, metrics.artifact_bytes);
    boundary!(max_expanded_nodes, metrics.expanded_nodes);
    boundary!(max_data_depth_root_one, metrics.data_depth_root_one);
    boundary!(max_source_bytes, baseline.record().source_bytes);
    boundary!(max_generation_work_units, metrics.generation_work_units);
}

#[test]
fn zero_eligibility_limits_do_not_admit_even_a_nil_program() {
    macro_rules! zero {
        ($field:ident) => {{
            let limits = WitnessGenerationLimits {
                $field: 0,
                ..WitnessGenerationLimits::default()
            };
            limits.validate().unwrap();
            assert_kind(generated(&[0x80], &limits), ErrorKind::ResourceLimit);
        }};
    }
    zero!(max_artifact_bytes);
    zero!(max_expanded_nodes);
    zero!(max_data_depth_root_one);
    zero!(max_source_bytes);
    zero!(max_generation_work_units);
}

#[test]
fn admitted_large_atoms_have_constant_size_witness_source_while_static_rejects() {
    let mut lengths = Vec::new();
    let mut work = Vec::new();
    for length in [257, 8192] {
        let bytes = pair(&[1], &atom(&vec![0xa7; length]));
        let original = package(&bytes);
        let fixture = Fixture::new(Limits::default());
        fixture.publish(&original);
        let checked = lock(&[("module", &original)]);
        let generated = generate_locked_witness_rust(
            &checked,
            &fixture.registry,
            "module",
            &WitnessGenerationLimits::default(),
        )
        .unwrap();
        assert_kind(
            crate::codegen::generate_locked_rust(
                &checked,
                &fixture.registry,
                "module",
                &crate::codegen::GenerationLimits::default(),
            ),
            ErrorKind::ResourceLimit,
        );
        assert_eq!(generated.record().artifact, *original.artifact().identity());
        assert_eq!(generated.record().metrics.expanded_nodes, 3);
        assert_eq!(generated.record().metrics.data_depth_root_one, 2);
        lengths.push(generated.source().len());
        work.push(generated.record().metrics.generation_work_units);
    }
    // The differing decimal artifact length and raw-lock length are measured;
    // neither selected payload is traversed or copied by the source recipe.
    assert_eq!(lengths[1] - lengths[0], 1);
    assert!(work[1] - work[0] < 32);
}

#[test]
fn node_and_depth_profiles_are_independent_of_static_construction_limits() {
    let mut bytes = vec![0x80];
    for _ in 0..300 {
        bytes = pair(&[0x80], &bytes);
    }
    let witness = generated(&bytes, &WitnessGenerationLimits::default()).unwrap();
    assert_eq!(witness.record().metrics.expanded_nodes, 601);
    assert_eq!(witness.record().metrics.data_depth_root_one, 301);
    assert_eq!(witness.record().metrics.embedded_program_bytes, 0);
    assert!(witness.source().len() < 8192);
    let limits = WitnessGenerationLimits {
        max_data_depth_root_one: 300,
        ..WitnessGenerationLimits::default()
    };
    assert_kind(generated(&bytes, &limits), ErrorKind::ResourceLimit);
    let limits = WitnessGenerationLimits {
        max_expanded_nodes: 600,
        ..WitnessGenerationLimits::default()
    };
    assert_kind(generated(&bytes, &limits), ErrorKind::ResourceLimit);
}
