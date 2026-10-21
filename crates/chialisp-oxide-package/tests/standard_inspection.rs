//! Independent pinned SDK codecs over a retained Coinset HTTP observation.
//! No VM, signature validation, transaction submission or chain authentication.
use chia_bls::PublicKey;
use chia_protocol::Coin;
use chia_puzzle_types::standard::{StandardArgs, StandardSolution};
use chia_puzzles::{
    P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE, P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE_HASH,
};
use chia_sdk_types::Mod;
use chialisp_oxide_package::{
    Digest32, ErrorKind, Limits, VerifiedPackage,
    artifact::{Encoding, inspect_artifact},
    digest,
    inspect::{Inspection, inspect_reveal},
    standard::{StandardModule, SyntheticPublicKey},
    store::Registry,
    verify_package,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, tree_hash};
use clvmr_sdk::{
    Allocator, NodePtr,
    serde::{node_from_bytes, node_to_bytes},
};
use serde_json::Value;

macro_rules! fixture {
    ($name:literal) => {
        (
            $name,
            include_bytes!(concat!("fixtures/mainnet-standard/", $name)).as_slice(),
        )
    };
}
const FIXTURES: &[(&str, &[u8])] = &[
    fixture!("expected.json"),
    fixture!("network-info.body"),
    fixture!("network-info.receipt.json"),
    fixture!("standard-coin-record.body"),
    fixture!("standard-coin-record.receipt.json"),
    fixture!("standard-coin-spend.body"),
    fixture!("standard-coin-spend.receipt.json"),
    fixture!("pinned-standard-module-hex.body"),
    fixture!("pinned-standard-module-hex.receipt.json"),
    fixture!("pinned-standard-source.body"),
    fixture!("pinned-standard-source.receipt.json"),
    fixture!("UPSTREAM-LICENSE-APACHE-2.0"),
];
const PROVENANCE: &[u8] = include_bytes!("fixtures/mainnet-standard/provenance.json");

fn bytes(name: &str) -> &'static [u8] {
    FIXTURES.iter().find(|(key, _)| *key == name).unwrap().1
}
fn document(name: &str) -> Value {
    serde_json::from_slice(bytes(name)).unwrap()
}
fn decode(value: &Value) -> Vec<u8> {
    hex::decode(value.as_str().unwrap().trim_start_matches("0x")).unwrap()
}
fn hash(value: &Value) -> Digest32 {
    value
        .as_str()
        .unwrap()
        .trim_start_matches("0x")
        .parse()
        .unwrap()
}
fn reveal() -> Vec<u8> {
    decode(&document("standard-coin-spend.body")["coin_solution"]["puzzle_reveal"])
}
fn expected() -> Value {
    document("expected.json")
}
fn report(inspection: &Inspection) -> Value {
    serde_json::to_value(inspection.report()).unwrap()
}
fn sdk_coin(value: &Value) -> Coin {
    Coin::new(
        (*hash(&value["parent_coin_info"]).as_bytes()).into(),
        (*hash(&value["puzzle_hash"]).as_bytes()).into(),
        value["amount"].as_u64().unwrap(),
    )
}

#[test]
fn retained_http_bodies_receipts_and_upstream_pins_are_exact() {
    let provenance: Value = serde_json::from_slice(PROVENANCE).unwrap();
    let declared = provenance["files"].as_array().unwrap();
    assert_eq!(declared.len(), FIXTURES.len());
    for (name, actual) in FIXTURES {
        let entries = declared
            .iter()
            .filter(|entry| entry["path"] == *name)
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1, "exactly one retained identity for {name}");
        assert_eq!(entries[0]["bytes"], actual.len());
        assert_eq!(entries[0]["sha256"], digest(actual).to_string());
    }
    for stem in [
        "network-info",
        "standard-coin-record",
        "standard-coin-spend",
        "pinned-standard-module-hex",
        "pinned-standard-source",
    ] {
        let receipt = document(&format!("{stem}.receipt.json"));
        let body = bytes(&format!("{stem}.body"));
        assert_eq!(receipt["status"], 200);
        assert_eq!(receipt["bytes"], body.len());
        assert_eq!(receipt["sha256"], digest(body).to_string());
        assert!(
            receipt["requested_at_utc"]
                .as_str()
                .unwrap()
                .starts_with("2026-10-21T")
        );
    }
    assert_eq!(document("network-info.body")["network_name"], "mainnet");
    assert_eq!(document("network-info.body")["network_prefix"], "xch");
    let request = document("standard-coin-spend.receipt.json");
    assert_eq!(request["request_json"]["height"], 6_318_254);
    assert_eq!(
        request["request_json"]["coin_id"],
        format!("0x{}", expected()["coin_id"].as_str().unwrap())
    );
    let published_hex = std::str::from_utf8(bytes("pinned-standard-module-hex.body"))
        .unwrap()
        .trim();
    assert_eq!(
        hex::decode(published_hex).unwrap(),
        P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE
    );
    assert_eq!(
        StandardArgs::mod_reveal().as_ref(),
        &P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE
    );
    assert_eq!(
        StandardArgs::mod_hash().to_bytes(),
        P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE_HASH
    );
    assert_eq!(
        provenance["upstream_source_pins"].as_array().unwrap().len(),
        6
    );
    assert!(
        provenance["upstream_source_pins"]
            .as_array()
            .unwrap()
            .iter()
            .all(|source| source["license"] == "Apache-2.0")
    );
}

#[test]
fn actual_observed_reveal_decodes_and_recurries_through_the_independent_sdk() {
    let expected = expected();
    let reveal = reveal();
    assert_eq!(reveal.len(), 291);
    assert_eq!(
        digest(&reveal),
        hash(&expected["puzzle_reveal_blob_sha256"])
    );
    let full_hash = hash(&expected["puzzle_tree_hash"]);
    let inspection = inspect_reveal(&reveal, &Limits::default(), Some(full_hash)).unwrap();
    let module = inspection.module().unwrap();
    assert_eq!(module.bytes().len(), 227);
    assert_eq!(module.bytes(), P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE);
    assert_eq!(
        module.identity().blob_sha256,
        hash(&expected["module_blob_sha256"])
    );
    assert_eq!(
        module.identity().module_tree_hash,
        hash(&expected["module_tree_hash"])
    );
    assert_ne!(module.identity().module_tree_hash, full_hash);
    assert_eq!(inspection.reveal().identity().module_tree_hash, full_hash);
    assert_eq!(inspection.arguments().len(), 1);

    // This is the SDK's older pinned allocator/derive-codec path, independent
    // of the inspector's current clvmr parser and canonical-curry walker.
    let mut allocator = Allocator::new();
    let node = node_from_bytes(&mut allocator, &reveal).unwrap();
    let decoded = CurriedProgram::<NodePtr, StandardArgs>::from_clvm(&allocator, node).unwrap();
    assert_eq!(
        node_to_bytes(&allocator, decoded.program).unwrap(),
        module.bytes()
    );
    assert_eq!(
        tree_hash(&allocator, decoded.program).to_bytes(),
        P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE_HASH
    );
    let key_bytes = decoded.args.synthetic_key.to_bytes();
    assert_eq!(
        key_bytes.as_slice(),
        decode(&expected["arguments"][0]["hex"])
    );
    assert!(decoded.args.synthetic_key.is_valid());
    assert_eq!(
        PublicKey::from_bytes(&key_bytes).unwrap(),
        decoded.args.synthetic_key
    );
    let argument_node = node_from_bytes(&mut allocator, inspection.arguments()[0].bytes()).unwrap();
    assert_eq!(
        PublicKey::from_clvm(&allocator, argument_node).unwrap(),
        decoded.args.synthetic_key
    );
    let recurry = decoded.to_clvm(&mut allocator).unwrap();
    assert_eq!(node_to_bytes(&allocator, recurry).unwrap(), reveal);
    assert_eq!(
        tree_hash(&allocator, recurry).to_bytes(),
        *full_hash.as_bytes()
    );
    assert_eq!(
        StandardArgs::curry_tree_hash(decoded.args.synthetic_key).to_bytes(),
        *full_hash.as_bytes()
    );
    let report = report(&inspection);
    assert_eq!(report["module_recognized"], true);
    assert_eq!(report["standard_arguments"]["status"], "decoded");
    assert_eq!(
        report["standard_arguments"]["synthetic_public_key_hex"],
        hex::encode(key_bytes)
    );
}

#[test]
fn actual_observed_solution_preserves_the_sdk_three_field_schema() {
    let expected = expected();
    let raw = decode(&document("standard-coin-spend.body")["coin_solution"]["solution"]);
    assert_eq!(raw.len(), 103);
    assert_eq!(digest(&raw), hash(&expected["solution_blob_sha256"]));
    let mut allocator = Allocator::new();
    let node = node_from_bytes(&mut allocator, &raw).unwrap();
    let decoded = StandardSolution::<NodePtr, NodePtr>::from_clvm(&allocator, node).unwrap();
    assert_eq!(decoded.original_public_key, None);
    assert_eq!(
        node_to_bytes(&allocator, decoded.delegated_puzzle).unwrap(),
        decode(&expected["solution_fields"]["delegated_puzzle_hex"])
    );
    assert_eq!(
        tree_hash(&allocator, decoded.delegated_puzzle).to_bytes(),
        *hash(&expected["solution_fields"]["delegated_puzzle_tree_hash"]).as_bytes()
    );
    assert_eq!(
        node_to_bytes(&allocator, decoded.solution).unwrap(),
        vec![0x80]
    );
    let rebuilt = decoded.to_clvm(&mut allocator).unwrap();
    assert_eq!(node_to_bytes(&allocator, rebuilt).unwrap(), raw);
    // Width/shape recognition is not evaluation of the delegated puzzle and
    // says nothing about the missing aggregate signature or chain validity.
}

fn observed_relations_match(record: &Value, spend: &Value, coin_id: Digest32) -> bool {
    let record_coin = &record["coin_record"]["coin"];
    let spend_coin = &spend["coin_solution"]["coin"];
    if record_coin != spend_coin
        || sdk_coin(record_coin).coin_id().to_bytes() != *coin_id.as_bytes()
    {
        return false;
    }
    let raw = decode(&spend["coin_solution"]["puzzle_reveal"]);
    inspect_reveal(
        &raw,
        &Limits::default(),
        Some(hash(&record_coin["puzzle_hash"])),
    )
    .is_ok()
}

#[test]
fn false_coin_tuple_or_reveal_cannot_pass_the_observed_local_relations() {
    let record = document("standard-coin-record.body");
    let spend = document("standard-coin-spend.body");
    let coin_id = hash(&expected()["coin_id"]);
    assert!(observed_relations_match(&record, &spend, coin_id));
    assert_eq!(record["coin_record"]["spent"], true);
    assert_eq!(record["coin_record"]["confirmed_block_index"], 6_318_250);
    assert_eq!(record["coin_record"]["spent_block_index"], 6_318_254);
    assert!(!observed_relations_match(
        &record,
        &spend,
        Digest32::new([0; 32])
    ));
    for field in ["parent_coin_info", "puzzle_hash", "amount"] {
        let mut false_record = record.clone();
        let mut false_spend = spend.clone();
        let changed = if field == "amount" {
            Value::from(1)
        } else {
            Value::from(format!("0x{}", "00".repeat(32)))
        };
        false_record["coin_record"]["coin"][field] = changed.clone();
        false_spend["coin_solution"]["coin"][field] = changed;
        assert!(
            !observed_relations_match(&false_record, &false_spend, coin_id),
            "false {field}"
        );
    }
    let mut false_reveal = spend.clone();
    false_reveal["coin_solution"]["puzzle_reveal"] = "0x80".into();
    assert!(!observed_relations_match(&record, &false_reveal, coin_id));
    assert_eq!(
        inspect_reveal(&reveal(), &Limits::default(), Some(Digest32::new([0; 32])))
            .unwrap_err()
            .kind(),
        ErrorKind::DigestMismatch
    );
}

fn sdk_list(allocator: &mut Allocator, items: &[NodePtr]) -> NodePtr {
    items.iter().rev().fold(NodePtr::NIL, |tail, item| {
        allocator.new_pair(*item, tail).unwrap()
    })
}
fn raw_standard_arguments(arguments: &[Vec<u8>], pair_argument: bool) -> Vec<u8> {
    let mut allocator = Allocator::new();
    let module = node_from_bytes(&mut allocator, &P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE).unwrap();
    let one = allocator.new_atom(&[1]).unwrap();
    let four = allocator.new_atom(&[4]).unwrap();
    let mut tail = one;
    for bytes in arguments.iter().rev() {
        let mut argument = allocator.new_atom(bytes).unwrap();
        if pair_argument {
            argument = allocator.new_pair(argument, NodePtr::NIL).unwrap();
        }
        let quoted = allocator.new_pair(one, argument).unwrap();
        tail = sdk_list(&mut allocator, &[four, quoted, tail]);
    }
    let node = CurriedProgram {
        program: module,
        args: tail,
    }
    .to_clvm(&mut allocator)
    .unwrap();
    node_to_bytes(&allocator, node).unwrap()
}

#[test]
fn recognized_module_does_not_grant_invalid_arguments_a_standard_schema() {
    let key = decode(&expected()["arguments"][0]["hex"]);
    let cases = [
        (vec![], false, "wrong_arity"),
        (vec![key.clone(), key.clone()], false, "wrong_arity"),
        (vec![key[..47].to_vec()], false, "wrong_public_key_length"),
        (vec![vec![0; 48]], false, "invalid_public_key"),
        (vec![key], true, "expected_atom"),
    ];
    for (arguments, pair, reason) in cases {
        let raw = raw_standard_arguments(&arguments, pair);
        let inspection = inspect_reveal(&raw, &Limits::default(), None).unwrap();
        let report = report(&inspection);
        assert_eq!(report["module_recognized"], true, "{reason}");
        assert_eq!(
            report["standard_arguments"]["status"], "invalid",
            "{reason}"
        );
        assert_eq!(report["standard_arguments"]["reason"], reason);
        let mut allocator = Allocator::new();
        let node = node_from_bytes(&mut allocator, &raw).unwrap();
        assert!(
            CurriedProgram::<NodePtr, StandardArgs>::from_clvm(&allocator, node).is_err(),
            "independent SDK must also reject {reason}"
        );
    }
    let raw_module = inspect_reveal(
        &P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE,
        &Limits::default(),
        None,
    )
    .unwrap();
    assert_eq!(
        report(&raw_module)["standard_arguments"]["status"],
        "not_curried"
    );
    assert_eq!(
        raw_module.reveal().identity(),
        raw_module.module().unwrap().identity()
    );
    // Canonical infinity is a valid group encoding, independent of whether a
    // new-instance builder permits it as an authorization key.
    let infinity = PublicKey::default().to_bytes();
    let raw = raw_standard_arguments(&[infinity.to_vec()], false);
    let inspection = inspect_reveal(&raw, &Limits::default(), None).unwrap();
    assert_eq!(
        report(&inspection)["standard_arguments"]["status"],
        "decoded"
    );
    let mut allocator = Allocator::new();
    let node = node_from_bytes(&mut allocator, &raw).unwrap();
    let decoded = CurriedProgram::<NodePtr, StandardArgs>::from_clvm(&allocator, node).unwrap();
    assert_eq!(decoded.args.synthetic_key, PublicKey::default());
}

fn package_for_blob(blob: &[u8]) -> VerifiedPackage {
    let artifact = inspect_artifact(blob, Encoding::Plain, &Limits::default()).unwrap();
    let declaration = serde_json::json!({
        "schema_version": 1,
        "kind": "clvm_artifact",
        "artifact": { "identity": artifact.identity(), "clvm_hex": hex::encode(blob) },
        "rust": null,
        "source_scope": "not_recorded",
        "sources": [],
        "provenance": null,
    });
    verify_package(
        &serde_json::to_vec(&declaration).unwrap(),
        &Limits::default(),
    )
    .unwrap()
}

#[test]
fn host_binding_uses_the_actual_retrieved_module_and_matches_the_observed_instance() {
    let observed = inspect_reveal(&reveal(), &Limits::default(), None).unwrap();
    let module_package = package_for_blob(observed.module().unwrap().bytes());
    let temporary = tempfile::tempdir().unwrap();
    let registry = Registry::create(
        &temporary.path().canonicalize().unwrap().join("registry"),
        Limits::default(),
    )
    .unwrap();
    registry.publish(&module_package).unwrap();
    let retrieved = registry.get(module_package.object_id()).unwrap();
    let binding = StandardModule::from_package(&retrieved, &Limits::default()).unwrap();
    assert_eq!(binding.object_id(), retrieved.object_id());
    assert_eq!(binding.artifact().bytes(), retrieved.artifact().bytes());
    let expected_key: [u8; 48] = decode(&expected()["arguments"][0]["hex"])
        .try_into()
        .unwrap();
    let key = SyntheticPublicKey::from_bytes(expected_key).unwrap();
    assert_eq!(key.to_bytes(), expected_key);
    assert_eq!(
        key.public_key(),
        PublicKey::from_bytes(&expected_key).unwrap()
    );
    let instance = binding.curry(&key, &Limits::default()).unwrap();
    assert_eq!(instance.module_object_id(), retrieved.object_id());
    assert_eq!(instance.synthetic_key().to_bytes(), expected_key);
    assert_eq!(instance.artifact().bytes(), reveal());
    assert_eq!(instance.artifact().identity(), observed.reveal().identity());

    let other = SyntheticPublicKey::from_bytes(PublicKey::generator().to_bytes()).unwrap();
    assert_ne!(other.to_bytes(), key.to_bytes());
    let other_instance = binding.curry(&other, &Limits::default()).unwrap();
    let other_inspection =
        inspect_reveal(other_instance.artifact().bytes(), &Limits::default(), None).unwrap();
    assert_eq!(
        other_inspection.module().unwrap().identity(),
        observed.module().unwrap().identity()
    );
    assert_ne!(
        other_instance.artifact().identity().module_tree_hash,
        instance.artifact().identity().module_tree_hash
    );
    let mut allocator = Allocator::new();
    let official_module =
        node_from_bytes(&mut allocator, &P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE).unwrap();
    let oracle = CurriedProgram {
        program: official_module,
        args: StandardArgs::new(other.public_key()),
    }
    .to_clvm(&mut allocator)
    .unwrap();
    assert_eq!(
        node_to_bytes(&allocator, oracle).unwrap(),
        other_instance.artifact().bytes()
    );

    assert!(StandardModule::from_package(&package_for_blob(&[0x80]), &Limits::default()).is_err());
    assert!(
        StandardModule::from_package(&package_for_blob(&reveal()), &Limits::default()).is_err()
    );
    assert!(SyntheticPublicKey::from_bytes([0; 48]).is_err());
    assert!(SyntheticPublicKey::from_bytes(PublicKey::default().to_bytes()).is_err());
    let too_small = Limits {
        max_blob_bytes: reveal().len() - 1,
        ..Limits::default()
    };
    assert_eq!(
        binding.curry(&key, &too_small).err().unwrap().kind(),
        ErrorKind::ResourceLimit
    );
}

#[test]
fn host_standard_solution_binding_matches_the_sdk_and_preserves_opaque_inner_trees() {
    let module_package = package_for_blob(&decode(&expected()["module_hex"]));
    let binding = StandardModule::from_package(&module_package, &Limits::default()).unwrap();
    let key = SyntheticPublicKey::from_bytes(
        decode(&expected()["arguments"][0]["hex"])
            .try_into()
            .unwrap(),
    )
    .unwrap();
    let instance = binding.curry(&key, &Limits::default()).unwrap();
    let inner_program_bytes = decode(&expected()["solution_fields"]["delegated_puzzle_hex"]);
    let inner_program =
        inspect_artifact(&inner_program_bytes, Encoding::Plain, &Limits::default()).unwrap();
    let nil = inspect_artifact(&[0x80], Encoding::Plain, &Limits::default()).unwrap();
    let complete = instance
        .delegated_solution(&inner_program, &nil, &Limits::default())
        .unwrap();
    assert_eq!(complete.bytes(), decode(&expected()["solution_hex"]));

    // The reviewed outer binding does not infer the inner program's ABI.
    // Preserve even an improper inner solution tree byte for byte.
    let opaque_bytes = [0xff, 1, 2];
    let opaque = inspect_artifact(&opaque_bytes, Encoding::Plain, &Limits::default()).unwrap();
    let complete = instance
        .delegated_solution(&inner_program, &opaque, &Limits::default())
        .unwrap();
    let mut allocator = Allocator::new();
    let inner_program_node = node_from_bytes(&mut allocator, &inner_program_bytes).unwrap();
    let inner_solution_node = node_from_bytes(&mut allocator, &opaque_bytes).unwrap();
    let independent = StandardSolution {
        original_public_key: None,
        delegated_puzzle: inner_program_node,
        solution: inner_solution_node,
    }
    .to_clvm(&mut allocator)
    .unwrap();
    assert_eq!(
        node_to_bytes(&allocator, independent).unwrap(),
        complete.bytes()
    );
    let decoded = StandardSolution::<NodePtr, NodePtr>::from_clvm(&allocator, independent).unwrap();
    assert_eq!(decoded.original_public_key, None);
    assert_eq!(
        node_to_bytes(&allocator, decoded.solution).unwrap(),
        opaque_bytes
    );
}
