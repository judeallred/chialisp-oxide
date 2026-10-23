//! Retrieved-byte canonical binding controls, independent source compilation
//! and typed VM observations. No signatures, SDK spends or chain claims.
use chialisp_oxide_package::{
    Digest32, ErrorKind, Limits, VerifiedPackage,
    artifact::{Encoding, VerifiedArtifact, inspect_artifact},
    digest,
    inspect::{PuzzleHashArgumentError, PuzzleHashArguments, RevealForm, inspect_reveal},
    p2_puzzle_hash::{InnerPuzzleHash, P2PuzzleHashModule},
    store::Registry,
    verify_package,
};
use chialisp_oxide_vm::{FailureCode, classify_failure};
use clvmr::{
    Allocator, ChiaDialect, MEMPOOL_MODE, NodePtr, run_program,
    serde::{node_from_bytes, node_to_bytes},
};
use serde_json::{Value, json};

const SOURCE: &str = include_str!("fixtures/p2-puzzle-hash/p2_puzzle_hash.clsp");
const MODULE_HEX: &str = include_str!("fixtures/p2-puzzle-hash/p2_puzzle_hash.clsp.hex");
const PROVENANCE: &str = include_str!("fixtures/p2-puzzle-hash/provenance.json");
const LICENSE: &[u8] = include_bytes!("fixtures/p2-puzzle-hash/UPSTREAM-LICENSE-APACHE-2.0");
const SOURCE_REVISION: &str = "d122456feeef55708d354a065623037bfa010714";

fn admitted(bytes: &[u8]) -> VerifiedArtifact {
    inspect_artifact(bytes, Encoding::Plain, &Limits::default()).unwrap()
}
fn package(bytes: &[u8]) -> VerifiedPackage {
    let artifact = admitted(bytes);
    let envelope = json!({
        "schema_version":1, "kind":"clvm_artifact",
        "artifact":{"identity":artifact.identity(),"clvm_hex":hex::encode(bytes)},
        "rust":null,"source_scope":"not_recorded","sources":[],"provenance":null,
    });
    verify_package(&serde_json::to_vec(&envelope).unwrap(), &Limits::default()).unwrap()
}
fn published_module() -> Vec<u8> {
    hex::decode(MODULE_HEX.trim()).unwrap()
}
fn binding() -> P2PuzzleHashModule {
    P2PuzzleHashModule::from_package(&package(&published_module()), &Limits::default()).unwrap()
}
fn list(allocator: &mut Allocator, values: &[NodePtr]) -> NodePtr {
    values.iter().rev().fold(NodePtr::NIL, |tail, value| {
        allocator.new_pair(*value, tail).unwrap()
    })
}
// Current-VM manual trees are independent of the binding's older SDK derive codecs.
fn raw_curry(module: &[u8], arguments: &[Vec<u8>]) -> Vec<u8> {
    let mut allocator = Allocator::new();
    let module = node_from_bytes(&mut allocator, module).unwrap();
    let quote = allocator.new_atom(&[1]).unwrap();
    let apply = allocator.new_atom(&[2]).unwrap();
    let cons = allocator.new_atom(&[4]).unwrap();
    let mut environment = quote;
    for argument in arguments.iter().rev() {
        let argument = node_from_bytes(&mut allocator, argument).unwrap();
        let quoted = allocator.new_pair(quote, argument).unwrap();
        environment = list(&mut allocator, &[cons, quoted, environment]);
    }
    let quoted_module = allocator.new_pair(quote, module).unwrap();
    let program = list(&mut allocator, &[apply, quoted_module, environment]);
    node_to_bytes(&allocator, program).unwrap()
}
fn raw_solution(program: &[u8], solution: &[u8]) -> Vec<u8> {
    let mut allocator = Allocator::new();
    let program = node_from_bytes(&mut allocator, program).unwrap();
    let solution = node_from_bytes(&mut allocator, solution).unwrap();
    let node = list(&mut allocator, &[program, solution]);
    node_to_bytes(&allocator, node).unwrap()
}
fn hash_atom(hash: Digest32) -> Vec<u8> {
    let mut bytes = vec![0xa0];
    bytes.extend_from_slice(hash.as_bytes());
    bytes
}
fn run(program: &[u8], environment: &[u8], max_cost: u64) -> Result<Vec<u8>, FailureCode> {
    let mut allocator = Allocator::new_limited(8 * 1024 * 1024);
    let program = node_from_bytes(&mut allocator, program).unwrap();
    let environment = node_from_bytes(&mut allocator, environment).unwrap();
    run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        environment,
        max_cost,
    )
    .map(|result| node_to_bytes(&allocator, result.1).unwrap())
    .map_err(|error| classify_failure(&error).code())
}

#[test]
fn retained_pins_compile_to_the_published_module_and_sdk_bytes() {
    let provenance: Value = serde_json::from_str(PROVENANCE).unwrap();
    assert_eq!(provenance["commit"], SOURCE_REVISION);
    assert_eq!(
        provenance["tree"],
        "89e5f55c5afdde2c436b9a2c08b8e30ecf11eb62"
    );
    assert_eq!(
        digest(SOURCE.as_bytes()).to_string(),
        "ae756f5268f5dc1b1dde949e9dcbd9d388d5308d09c73353e05a1380b84f4088"
    );
    assert_eq!(provenance["license"], "Apache-2.0");
    let fixtures: &[(&str, &[u8])] = &[
        ("p2_puzzle_hash.clsp", SOURCE.as_bytes()),
        ("p2_puzzle_hash.clsp.hex", MODULE_HEX.as_bytes()),
        ("UPSTREAM-LICENSE-APACHE-2.0", LICENSE),
    ];
    assert_eq!(
        provenance["files"].as_array().unwrap().len(),
        fixtures.len()
    );
    for (name, bytes) in fixtures {
        let entries: Vec<_> = provenance["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["path"] == *name)
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["bytes"], bytes.len());
        assert_eq!(entries[0]["sha256"], digest(bytes).to_string());
        assert!(
            entries[0]["url"]
                .as_str()
                .unwrap()
                .contains(SOURCE_REVISION)
        );
    }
    let compiled = chialisp_oxide_reference::compile(SOURCE).unwrap();
    assert_eq!(compiled, published_module());
    assert_eq!(compiled.as_slice(), chia_puzzles::P2_PUZZLE_HASH);
    let artifact = admitted(&compiled);
    assert_eq!(artifact.identity().bytes, 143);
    assert_eq!(
        artifact.identity().blob_sha256.to_string(),
        "678bc94e1a0fa439ba21eb11ccfb7d1cea5ac9ee6e59dcf51175f3ab642aca64"
    );
    assert_eq!(
        artifact.identity().module_tree_hash.as_bytes(),
        &chia_puzzles::P2_PUZZLE_HASH_HASH
    );
}

#[test]
fn actual_retrieved_module_curries_the_commitment_without_substituting_bytes() {
    let package = package(&published_module());
    let directory = tempfile::tempdir().unwrap();
    let registry = Registry::create(
        &directory.path().canonicalize().unwrap().join("registry"),
        Limits::default(),
    )
    .unwrap();
    registry.publish(&package).unwrap();
    let retrieved = registry.get(package.object_id()).unwrap();
    let module = P2PuzzleHashModule::from_package(&retrieved, &Limits::default()).unwrap();
    assert_eq!(module.object_id(), retrieved.object_id());
    assert_eq!(module.artifact().bytes(), retrieved.artifact().bytes());
    let program = admitted(&[1]); // identity: return the entire supplied environment
    let commitment = InnerPuzzleHash::from_artifact(&program);
    assert_eq!(
        commitment,
        InnerPuzzleHash::from_tree_hash(program.identity().module_tree_hash)
    );
    let instance = module.curry(commitment, &Limits::default()).unwrap();
    let compiled = chialisp_oxide_reference::compile(SOURCE).unwrap();
    let expected = raw_curry(&compiled, &[hash_atom(commitment.tree_hash())]);
    assert_eq!(instance.artifact().bytes(), expected);
    assert_eq!(instance.module_object_id(), retrieved.object_id());
    assert_eq!(instance.commitment(), commitment);
    assert_ne!(
        instance.artifact().identity().module_tree_hash,
        module.artifact().identity().module_tree_hash
    );

    let inspection = inspect_reveal(
        &expected,
        &Limits::default(),
        Some(instance.artifact().identity().module_tree_hash),
    )
    .unwrap();
    let known = inspection.report().known_module.as_ref().unwrap();
    assert_eq!(known.name, "p2_puzzle_hash");
    assert_eq!(known.source_revision, SOURCE_REVISION);
    assert_eq!(known.license, "Apache-2.0");
    assert_eq!(
        inspection.module().unwrap().bytes(),
        retrieved.artifact().bytes()
    );
    assert!(matches!(inspection.report().p2_puzzle_hash_arguments,
        Some(PuzzleHashArguments::Decoded { inner_puzzle_hash, role: "inner_puzzle_tree_hash" }) if inner_puzzle_hash == commitment.tree_hash()));
    let report = serde_json::to_value(inspection.report()).unwrap();
    assert_eq!(report["standard_arguments"]["status"], "not_applicable");
}

#[test]
fn explicit_binding_rejects_other_modules_despite_valid_package_admission() {
    for blob in [
        vec![1],
        vec![0x80],
        chia_puzzles::P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE.to_vec(),
    ] {
        let package = package(&blob);
        assert_eq!(
            P2PuzzleHashModule::from_package(&package, &Limits::default())
                .unwrap_err()
                .kind(),
            ErrorKind::DigestMismatch
        );
    }
    let package = package(&published_module());
    for limits in [
        Limits {
            max_blob_bytes: 142,
            ..Limits::default()
        },
        Limits {
            max_work: 0,
            ..Limits::default()
        },
    ] {
        assert_eq!(
            P2PuzzleHashModule::from_package(&package, &limits)
                .unwrap_err()
                .kind(),
            ErrorKind::ResourceLimit
        );
    }
}

#[test]
fn opaque_raw_atoms_and_improper_tails_are_preserved_in_both_codec_and_execution() {
    let module = binding();
    let program = admitted(&[1]);
    let instance = module
        .curry(InnerPuzzleHash::from_artifact(&program), &Limits::default())
        .unwrap();
    let reference = raw_curry(
        &chialisp_oxide_reference::compile(SOURCE).unwrap(),
        &[hash_atom(instance.commitment().tree_hash())],
    );
    for bytes in [
        vec![0x80],
        vec![0x82, 0, 0],
        vec![0x83, 0xff, 0x80, 0],
        vec![0xff, 1, 2],
        vec![0xff, 0x82, 0, 0, 0xff, 0x80, 0x81, 0xff],
    ] {
        let environment = admitted(&bytes);
        let solution = instance
            .solution(&program, &environment, &Limits::default())
            .unwrap();
        assert_eq!(
            solution.bytes(),
            raw_solution(program.bytes(), environment.bytes())
        );
        assert_eq!(
            run(instance.artifact().bytes(), solution.bytes(), 1_000_000),
            Ok(bytes.clone())
        );
        assert_eq!(run(&reference, solution.bytes(), 1_000_000), Ok(bytes));
    }
}

#[test]
fn wrong_commitments_are_host_errors_and_separately_executed_typed_vm_rejections() {
    let module = binding();
    let expected = admitted(&[1]);
    let instance = module
        .curry(
            InnerPuzzleHash::from_artifact(&expected),
            &Limits::default(),
        )
        .unwrap();
    let solution = admitted(&[0x80]);
    // nil succeeds on direct application; path 2 into nil fails with a distinct
    // code. Both must hit the commitment guard's Raise in the outer module.
    for bytes in [vec![0x80], vec![2]] {
        let wrong = admitted(&bytes);
        assert_eq!(
            instance
                .solution(&wrong, &solution, &Limits::default())
                .unwrap_err()
                .kind(),
            ErrorKind::DigestMismatch
        );
        let unsafe_solution = raw_solution(wrong.bytes(), solution.bytes());
        assert_eq!(
            run(instance.artifact().bytes(), &unsafe_solution, 1_000_000),
            Err(FailureCode::Raise)
        );
    }
    assert_eq!(run(&[0x80], solution.bytes(), 1_000_000), Ok(vec![0x80]));
    assert_eq!(
        run(&[2], solution.bytes(), 1_000_000),
        Err(FailureCode::PathIntoAtom)
    );
}

#[test]
fn matching_program_rejection_and_resource_exhaustion_remain_distinct() {
    let module = binding();
    let raising = admitted(&hex::decode("ff0880").unwrap());
    let instance = module
        .curry(InnerPuzzleHash::from_artifact(&raising), &Limits::default())
        .unwrap();
    let solution = instance
        .solution(&raising, &admitted(&[0x80]), &Limits::default())
        .unwrap();
    assert_eq!(
        run(instance.artifact().bytes(), solution.bytes(), 1_000_000),
        Err(FailureCode::Raise)
    );
    assert_eq!(
        run(instance.artifact().bytes(), solution.bytes(), 1),
        Err(FailureCode::CostExceeded)
    );
    let identity = admitted(&[1]);
    let instance = module
        .curry(
            InnerPuzzleHash::from_artifact(&identity),
            &Limits::default(),
        )
        .unwrap();
    let solution = instance
        .solution(&identity, &admitted(&[0x80]), &Limits::default())
        .unwrap();
    assert_eq!(
        run(instance.artifact().bytes(), solution.bytes(), 1),
        Err(FailureCode::CostExceeded)
    );
}

#[test]
fn catalog_recognizes_the_raw_module_and_rejects_invalid_commitment_shapes() {
    let raw = inspect_reveal(&published_module(), &Limits::default(), None).unwrap();
    assert_eq!(raw.report().form, RevealForm::RecognizedRawModule);
    assert!(matches!(
        raw.report().p2_puzzle_hash_arguments,
        Some(PuzzleHashArguments::NotCurried)
    ));
    let valid_hash = hash_atom(Digest32::new([7; 32]));
    for (arguments, reason) in [
        (vec![], PuzzleHashArgumentError::WrongArity),
        (
            vec![valid_hash.clone(), valid_hash],
            PuzzleHashArgumentError::WrongArity,
        ),
        (vec![vec![0x80]], PuzzleHashArgumentError::WrongHashLength),
        (
            vec![vec![0xff, 1, 2]],
            PuzzleHashArgumentError::ExpectedAtom,
        ),
    ] {
        let bytes = raw_curry(&published_module(), &arguments);
        let result = inspect_reveal(&bytes, &Limits::default(), None).unwrap();
        assert!(result.report().module_recognized);
        assert!(
            matches!(result.report().p2_puzzle_hash_arguments, Some(PuzzleHashArguments::Invalid { reason: actual }) if actual == reason)
        );
    }
}

#[path = "p2_puzzle_hash/bounds.rs"]
mod bounds;
