//! Public caller-limit controls; host resource errors are not puzzle rejection.
use super::*;

fn exact_shape(output: &VerifiedArtifact) -> Limits {
    Limits {
        max_blob_bytes: output.bytes().len(),
        max_expanded_bytes: output.bytes().len(),
        max_nodes: usize::try_from(output.stats().nodes).unwrap(),
        max_depth: usize::try_from(output.stats().depth).unwrap(),
        max_atom_heap_bytes: usize::try_from(output.stats().atom_bytes).unwrap() + 1,
        ..Limits::default()
    }
}
fn one_under(exact: &Limits) -> [Limits; 5] {
    [
        Limits {
            max_blob_bytes: exact.max_blob_bytes - 1,
            ..exact.clone()
        },
        Limits {
            max_expanded_bytes: exact.max_expanded_bytes - 1,
            ..exact.clone()
        },
        Limits {
            max_nodes: exact.max_nodes - 1,
            ..exact.clone()
        },
        Limits {
            max_depth: exact.max_depth - 1,
            ..exact.clone()
        },
        Limits {
            max_atom_heap_bytes: exact.max_atom_heap_bytes - 1,
            ..exact.clone()
        },
    ]
}
fn resource<T>(result: chialisp_oxide_package::Result<T>) {
    match result {
        Ok(_) => panic!("expected typed host resource limit"),
        Err(error) => assert_eq!(error.kind(), ErrorKind::ResourceLimit, "{error}"),
    }
}

#[test]
fn exact_curry_shape_passes_and_one_under_bounds_fail_without_mutating_module() {
    let module = binding();
    let original = module.artifact().identity().clone();
    let commitment = InnerPuzzleHash::from_tree_hash(Digest32::new([0xff; 32]));
    let independent = admitted(&raw_curry(
        &published_module(),
        &[hash_atom(commitment.tree_hash())],
    ));
    let exact = exact_shape(&independent);
    let instance = module.curry(commitment, &exact).unwrap();
    assert_eq!(instance.artifact().identity(), independent.identity());
    for limits in one_under(&exact) {
        resource(module.curry(commitment, &limits));
        assert_eq!(module.artifact().identity(), &original);
    }
    resource(module.curry(
        commitment,
        &Limits {
            max_atom_bytes: 31,
            ..exact
        },
    ));
    assert_eq!(module.artifact().identity(), &original);
}

#[test]
fn exact_solution_shape_passes_and_both_inputs_are_readmitted_under_caller_limits() {
    let module = binding();
    let program = admitted(&hex::decode("ff0184feedbeef").unwrap());
    let environment = admitted(&hex::decode("ff86fe00ff010203ff0102").unwrap());
    let instance = module
        .curry(InnerPuzzleHash::from_artifact(&program), &Limits::default())
        .unwrap();
    let original = (
        program.identity().clone(),
        environment.identity().clone(),
        instance.artifact().identity().clone(),
    );
    let expected = admitted(&raw_solution(program.bytes(), environment.bytes()));
    let exact = exact_shape(&expected);
    assert_eq!(
        instance
            .solution(&program, &environment, &exact)
            .unwrap()
            .identity(),
        expected.identity()
    );
    for limits in one_under(&exact).into_iter().chain([
        Limits {
            max_atom_bytes: 3,
            ..exact.clone()
        },
        Limits {
            max_atom_bytes: 5,
            ..exact.clone()
        },
    ]) {
        resource(instance.solution(&program, &environment, &limits));
        assert_eq!(program.identity(), &original.0);
        assert_eq!(environment.identity(), &original.1);
        assert_eq!(instance.artifact().identity(), &original.2);
    }
}

#[test]
fn invalid_sdk_heap_and_exhausted_work_profiles_cannot_construct_outputs() {
    let module = binding();
    let program = admitted(&[1]);
    let environment = admitted(&[0x80]);
    let commitment = InnerPuzzleHash::from_artifact(&program);
    let instance = module.curry(commitment, &Limits::default()).unwrap();
    let mut profiles = vec![
        Limits {
            max_atom_heap_bytes: 0,
            ..Limits::default()
        },
        Limits {
            max_work: 0,
            ..Limits::default()
        },
        Limits {
            max_work: 1,
            ..Limits::default()
        },
    ];
    #[cfg(target_pointer_width = "64")]
    profiles.push(Limits {
        max_atom_heap_bytes: u32::MAX as usize + 1,
        ..Limits::default()
    });
    for limits in profiles {
        resource(module.curry(commitment, &limits));
        resource(instance.solution(&program, &environment, &limits));
    }
}

#[test]
fn inspection_whole_pipeline_work_remains_exactly_bounded_for_new_schema() {
    let commitment = Digest32::new([3; 32]);
    let bytes = raw_curry(&published_module(), &[hash_atom(commitment)]);
    let baseline = inspect_reveal(&bytes, &Limits::default(), None).unwrap();
    let exact = Limits {
        max_work: baseline.work(),
        ..Limits::default()
    };
    assert_eq!(
        inspect_reveal(&bytes, &exact, None).unwrap().work(),
        baseline.work()
    );
    resource(inspect_reveal(
        &bytes,
        &Limits {
            max_work: exact.max_work - 1,
            ..exact
        },
        None,
    ));
}
