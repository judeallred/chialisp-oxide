//! Independent SDK codecs and bounded offline admission; no VM, signature,
//! spend or chain-authentication claims.
use chia_bls::PublicKey;
use chia_puzzle_types::standard::StandardArgs;
use chia_puzzles::P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE;
use chialisp_oxide_package::{
    ErrorKind, Limits, digest,
    inspect::{RevealForm, StandardArguments, inspect_reveal},
};
use clvm_traits::ToClvm;
use clvm_utils::{CurriedProgram, tree_hash};
use clvmr_sdk::{
    Allocator, NodePtr,
    serde::{node_from_bytes, node_to_bytes_limit},
};

fn serialize(allocator: &Allocator, node: NodePtr) -> Vec<u8> {
    node_to_bytes_limit(allocator, node, Limits::default().max_blob_bytes).unwrap()
}

fn sdk_list(allocator: &mut Allocator, items: &[NodePtr]) -> NodePtr {
    items.iter().rev().fold(NodePtr::NIL, |tail, item| {
        allocator.new_pair(*item, tail).unwrap()
    })
}

/// Build argument quotations with the independent SDK allocator, then let its
/// actual CurriedProgram codec construct the outer canonical curry expression.
fn sdk_curry(allocator: &mut Allocator, module: NodePtr, arguments: &[NodePtr]) -> NodePtr {
    let one = allocator.new_atom(&[1]).unwrap();
    let four = allocator.new_atom(&[4]).unwrap();
    let mut environment = one;
    for argument in arguments.iter().rev() {
        let quoted = allocator.new_pair(one, *argument).unwrap();
        environment = sdk_list(allocator, &[four, quoted, environment]);
    }
    CurriedProgram {
        program: module,
        args: environment,
    }
    .to_clvm(allocator)
    .unwrap()
}

#[test]
fn sdk_atom_header_boundaries_keep_exact_argument_spans_order_and_improper_tails() {
    let mut sdk = Allocator::new();
    // An arbitrary improper module is intentionally outside the known catalog.
    let one = sdk.new_atom(&[1]).unwrap();
    let two = sdk.new_atom(&[2]).unwrap();
    let module = sdk.new_pair(one, two).unwrap();
    let module_bytes = serialize(&sdk, module);
    let mut arguments = Vec::new();
    let mut expected_trees = Vec::new();
    // Opaque payloads contain quote/cons/apply, pair/backreference/serde markers,
    // nil-looking bytes and padding. Only the SDK's atom headers frame them.
    let marker = [
        0xfd, 0xff, 0x32, 0x30, 0x32, 0x36, 0xfe, 0x80, 0x00, 1, 2, 4,
    ];
    let boundaries: &[(usize, &[u8])] = &[
        (63, &[0xbf]),
        (64, &[0xc0, 0x40]),
        (8191, &[0xdf, 0xff]),
        (8192, &[0xe0, 0x20, 0x00]),
    ];
    for &(length, expected_header) in boundaries {
        let payload: Vec<u8> = (0..length)
            .map(|index| marker[index % marker.len()])
            .collect();
        let atom = sdk.new_atom(&payload).unwrap();
        let atom_bytes = serialize(&sdk, atom);
        assert_eq!(&atom_bytes[..expected_header.len()], expected_header);
        assert_eq!(&atom_bytes[expected_header.len()..], payload);
        assert_eq!(atom_bytes.len(), expected_header.len() + length);
        arguments.push(atom);
        expected_trees.push(atom_bytes.clone());

        // Keep the same header-crossing atom inside an improper argument. The
        // tail is the literal padded numeric atom 0x0001, not nil or integer1.
        let tail = sdk.new_atom(&[0, 1]).unwrap();
        let improper = sdk.new_pair(atom, tail).unwrap();
        let improper_bytes = serialize(&sdk, improper);
        assert_eq!(improper_bytes.first(), Some(&0xff));
        assert_eq!(&improper_bytes[1..1 + atom_bytes.len()], atom_bytes);
        assert_eq!(&improper_bytes[1 + atom_bytes.len()..], &[0x82, 0, 1]);
        arguments.push(improper);
        expected_trees.push(improper_bytes);
    }
    let curried = sdk_curry(&mut sdk, module, &arguments);
    let source = serialize(&sdk, curried);
    let inspection = inspect_reveal(&source, &Limits::default(), None).unwrap();
    assert_eq!(inspection.report().form, RevealForm::CanonicalCurry);
    assert!(!inspection.report().module_recognized);
    assert!(matches!(
        inspection.report().standard_arguments,
        StandardArguments::NotApplicable
    ));
    assert_eq!(inspection.module().unwrap().bytes(), module_bytes);
    assert_eq!(inspection.arguments().len(), expected_trees.len());
    assert_eq!(inspection.report().arguments.len(), expected_trees.len());
    assert_eq!(
        inspection.reveal().identity().module_tree_hash.as_bytes(),
        &tree_hash(&sdk, curried).to_bytes()
    );
    for (index, expected) in expected_trees.iter().enumerate() {
        let actual = &inspection.arguments()[index];
        let report = &inspection.report().arguments[index];
        assert_eq!(actual.bytes(), expected);
        assert_eq!(report.clvm_hex, hex::encode(expected));
        assert_eq!(report.identity, *actual.identity());
        assert_eq!(actual.identity().bytes, expected.len() as u64);
        assert_eq!(actual.identity().blob_sha256, digest(expected));
        assert_eq!(
            actual.identity().module_tree_hash.as_bytes(),
            &tree_hash(&sdk, arguments[index]).to_bytes()
        );
    }

    // Independently decode the inspector's selected spans into a fresh SDK
    // allocator, then rebuild the curry in the exact reported argument order.
    let mut rebuilt = Allocator::new();
    let module = node_from_bytes(&mut rebuilt, inspection.module().unwrap().bytes()).unwrap();
    let arguments: Vec<_> = inspection
        .arguments()
        .iter()
        .map(|argument| node_from_bytes(&mut rebuilt, argument.bytes()).unwrap())
        .collect();
    let curried = sdk_curry(&mut rebuilt, module, &arguments);
    assert_eq!(serialize(&rebuilt, curried), source);
}

#[test]
fn recognized_valid_key_exact_work_succeeds_and_one_under_is_a_host_resource_error() {
    let key = PublicKey::generator();
    assert!(!key.is_inf());
    let mut sdk = Allocator::new();
    let module = node_from_bytes(&mut sdk, &P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE).unwrap();
    let curried = CurriedProgram {
        program: module,
        args: StandardArgs::new(key),
    }
    .to_clvm(&mut sdk)
    .unwrap();
    let source = serialize(&sdk, curried);
    let baseline = inspect_reveal(&source, &Limits::default(), None).unwrap();
    assert_eq!(baseline.report().form, RevealForm::CanonicalCurry);
    assert!(baseline.report().module_recognized);
    match &baseline.report().standard_arguments {
        StandardArguments::Decoded {
            synthetic_public_key_hex,
            role,
            key_is_infinity,
        } => {
            assert_eq!(synthetic_public_key_hex, &hex::encode(key.to_bytes()));
            assert_eq!(*role, "synthetic_public_key");
            assert!(!*key_is_infinity);
        }
        other => panic!("valid SDK standard key must decode: {other:?}"),
    }
    // Use the public complete measurement. No private cost constants, stage
    // offsets, group-decoder model or duplicated bookkeeping algorithm appear.
    let observed_work = baseline.work();
    let exact_limits = Limits {
        max_work: observed_work,
        ..Limits::default()
    };
    let exact = inspect_reveal(&source, &exact_limits, None).unwrap();
    assert_eq!(exact.work(), observed_work);
    assert_eq!(exact.reveal().identity(), baseline.reveal().identity());
    assert_eq!(
        serde_json::to_value(exact.report()).unwrap(),
        serde_json::to_value(baseline.report()).unwrap()
    );
    let one_under = Limits {
        max_work: observed_work.checked_sub(1).unwrap(),
        ..exact_limits
    };
    match inspect_reveal(&source, &one_under, None) {
        Err(error) => assert_eq!(error.kind(), ErrorKind::ResourceLimit),
        Ok(result) => {
            panic!("exhausted host budget must not become an inspection/schema status: {result:?}")
        }
    }
}
