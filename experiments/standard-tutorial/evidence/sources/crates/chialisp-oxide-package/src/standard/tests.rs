//! Public-binding resource controls. These execute SDK codecs and local
//! admission only; no CLVM, signature, authorization or coin-spend execution.
use super::*;
use crate::verify_package;
use clvmr_sdk::{
    Allocator,
    serde::{node_from_bytes, node_to_bytes_limit},
};
use serde_json::{Value, json};

fn fixture_binding() -> (StandardModule, SyntheticPublicKey, [u8; 48]) {
    let expected: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/mainnet-standard/expected.json"
    ))
    .unwrap();
    let module = hex::decode(
        include_str!("../../tests/fixtures/mainnet-standard/pinned-standard-module-hex.body")
            .trim(),
    )
    .unwrap();
    let artifact = inspect_artifact(&module, Encoding::Plain, &Limits::default()).unwrap();
    let envelope = json!({
        "schema_version":1, "kind":"clvm_artifact",
        "artifact":{"identity":artifact.identity(),"clvm_hex":hex::encode(&module)},
        "rust":null,"source_scope":"not_recorded","sources":[],"provenance":null,
    });
    let package =
        verify_package(&serde_json::to_vec(&envelope).unwrap(), &Limits::default()).unwrap();
    let binding = StandardModule::from_package(&package, &Limits::default()).unwrap();
    assert_eq!(binding.object_id(), package.object_id());
    assert_eq!(binding.artifact().bytes(), module);
    let key_bytes: [u8; 48] = hex::decode(expected["arguments"][0]["hex"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let key = SyntheticPublicKey::from_bytes(key_bytes).unwrap();
    assert!(!key.public_key().is_inf());
    (binding, key, key_bytes)
}

fn admitted(raw: &[u8]) -> VerifiedArtifact {
    inspect_artifact(raw, Encoding::Plain, &Limits::default()).unwrap()
}

fn output_limits(output: &VerifiedArtifact) -> Limits {
    let stats = output.stats();
    Limits {
        max_blob_bytes: output.bytes().len(),
        max_expanded_bytes: usize::try_from(stats.expanded_bytes).unwrap(),
        max_nodes: usize::try_from(stats.nodes).unwrap(),
        max_depth: usize::try_from(stats.depth).unwrap(),
        // The binding conservatively budgets every atom payload plus the
        // initial ghost byte. This is not the smaller allocator charge, which
        // can reuse nil/one nodes, or a measurement of process memory.
        max_atom_heap_bytes: usize::try_from(stats.atom_bytes).unwrap() + 1,
        ..Limits::default()
    }
}

fn one_under(exact: &Limits) -> [(&'static str, Limits); 5] {
    [
        (
            "serialized bytes",
            Limits {
                max_blob_bytes: exact.max_blob_bytes - 1,
                ..exact.clone()
            },
        ),
        (
            "expanded bytes",
            Limits {
                max_expanded_bytes: exact.max_expanded_bytes - 1,
                ..exact.clone()
            },
        ),
        (
            "logical nodes",
            Limits {
                max_nodes: exact.max_nodes - 1,
                ..exact.clone()
            },
        ),
        (
            "depth",
            Limits {
                max_depth: exact.max_depth - 1,
                ..exact.clone()
            },
        ),
        (
            "conservative atom payload heap",
            Limits {
                max_atom_heap_bytes: exact.max_atom_heap_bytes - 1,
                ..exact.clone()
            },
        ),
    ]
}

fn resource_error<T>(result: Result<T>, label: &str) {
    match result {
        Err(error) => assert_eq!(error.kind(), ErrorKind::ResourceLimit, "{label}: {error}"),
        Ok(_) => panic!("{label}: expected a host resource limit"),
    }
}

#[test]
fn curry_exact_sdk_output_bounds_succeed_and_each_one_under_bound_rejects() {
    let (binding, key, key_bytes) = fixture_binding();
    let original = binding.artifact().identity().clone();
    let mut sdk = Allocator::new();
    let module = node_from_bytes(&mut sdk, binding.artifact().bytes()).unwrap();
    let independent_key = PublicKey::from_bytes(&key_bytes).unwrap();
    let expected = CurriedProgram {
        program: module,
        args: StandardArgs::new(independent_key),
    }
    .to_clvm(&mut sdk)
    .unwrap();
    let expected_bytes =
        node_to_bytes_limit(&sdk, expected, Limits::default().max_blob_bytes).unwrap();
    let expected = admitted(&expected_bytes);
    let exact = output_limits(&expected);
    let actual = binding.curry(&key, &exact).unwrap();
    assert_eq!(actual.artifact().identity(), expected.identity());
    assert_eq!(actual.artifact().bytes(), expected.bytes());
    assert_eq!(actual.module_object_id(), binding.object_id());
    for (label, limits) in one_under(&exact) {
        resource_error(binding.curry(&key, &limits), label);
        assert_eq!(binding.artifact().identity(), &original);
    }
    let narrow_atom = Limits {
        max_atom_bytes: key_bytes.len() - 1,
        ..exact
    };
    resource_error(
        binding.curry(&key, &narrow_atom),
        "synthetic key atom width",
    );
    assert_eq!(binding.artifact().identity(), &original);
}

#[test]
fn delegated_solution_exact_sdk_bounds_preserve_both_opaque_operands() {
    let (binding, key, _) = fixture_binding();
    let instance = binding.curry(&key, &Limits::default()).unwrap();
    // Opaque quote program and an improper, binary-containing solution tree.
    // Neither the binding nor this resource test assigns them a nominal ABI.
    let program = admitted(&hex::decode("ff0184feedbeef").unwrap());
    let solution = admitted(&hex::decode("ff86fe00ff010203ff0102").unwrap());
    let original_program = program.identity().clone();
    let original_solution = solution.identity().clone();
    let original_instance = instance.artifact().identity().clone();
    let mut sdk = Allocator::new();
    let delegated_puzzle = node_from_bytes(&mut sdk, program.bytes()).unwrap();
    let inner_solution = node_from_bytes(&mut sdk, solution.bytes()).unwrap();
    let expected = StandardSolution {
        original_public_key: None,
        delegated_puzzle,
        solution: inner_solution,
    }
    .to_clvm(&mut sdk)
    .unwrap();
    let expected_bytes =
        node_to_bytes_limit(&sdk, expected, Limits::default().max_blob_bytes).unwrap();
    let expected = admitted(&expected_bytes);
    let exact = output_limits(&expected);
    let actual = instance
        .delegated_solution(&program, &solution, &exact)
        .unwrap();
    assert_eq!(actual.identity(), expected.identity());
    assert_eq!(actual.bytes(), expected.bytes());
    for (label, limits) in one_under(&exact) {
        resource_error(
            instance.delegated_solution(&program, &solution, &limits),
            label,
        );
        assert_eq!(program.identity(), &original_program);
        assert_eq!(solution.identity(), &original_solution);
        assert_eq!(instance.artifact().identity(), &original_instance);
    }
    // Both operands were admitted earlier under a broader profile. Each must
    // still satisfy the new caller's per-atom ceiling before SDK construction.
    for (limit, label) in [(3, "program atom"), (5, "solution atom")] {
        let limits = Limits {
            max_atom_bytes: limit,
            ..exact.clone()
        };
        resource_error(
            instance.delegated_solution(&program, &solution, &limits),
            label,
        );
    }
    assert_eq!(program.identity(), &original_program);
    assert_eq!(solution.identity(), &original_solution);
    assert_eq!(instance.artifact().identity(), &original_instance);
}

#[test]
fn zero_oversized_and_exhausted_profiles_are_typed_resources_without_mutation() {
    let (binding, key, _) = fixture_binding();
    let instance = binding.curry(&key, &Limits::default()).unwrap();
    let program = admitted(&[1]);
    let solution = admitted(&[0x80]);
    let source = binding.artifact().identity().clone();
    let output = instance.artifact().identity().clone();
    let check = |limits: &Limits| {
        resource_error(binding.curry(&key, limits), "invalid curry profile");
        resource_error(
            instance.delegated_solution(&program, &solution, limits),
            "invalid solution profile",
        );
        assert_eq!(binding.artifact().identity(), &source);
        assert_eq!(instance.artifact().identity(), &output);
    };
    for limits in [
        Limits {
            max_atom_heap_bytes: 0,
            ..Limits::default()
        },
        Limits {
            max_work: 0,
            ..Limits::default()
        },
    ] {
        check(&limits);
    }
    #[cfg(target_pointer_width = "64")]
    check(&Limits {
        max_atom_heap_bytes: u32::MAX as usize + 1,
        ..Limits::default()
    });
}
