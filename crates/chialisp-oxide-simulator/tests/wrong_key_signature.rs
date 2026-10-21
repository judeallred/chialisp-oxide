//! Wrong-key signatures reach consensus with the correct coin/domain/message.
//! Public fixture keys here must never control real funds.
use chia_sdk_signer::{AggSigConstants, RequiredSignature};
use chia_sdk_types::TESTNET11_CONSTANTS;
use chialisp_oxide_simulator::{
    Bytes32, CoinSpend, ConsensusRejection, SecretKey, Signature, Simulator, SpendBundle,
    coin_spend, create_coin, partial_aggregate_signature, require_consensus_rejection,
    sign_single_requirement_with_wrong_key, sign_transaction,
};

const AMOUNT: u64 = 42;
const DESTINATION: [u8; 32] = [0x42; 32];
const MESSAGE: [u8; 32] = [0xab; 32];

fn payment(owner: &SecretKey, requirements: usize) -> Vec<u8> {
    // Independent wire-format fixture, not an invocation of SDK constructors.
    let signature = format!(
        "(list 50 0x{} 0x{})",
        hex::encode(owner.public_key().to_bytes()),
        hex::encode(MESSAGE)
    );
    let mut conditions = vec![signature; requirements];
    conditions.push(format!("(list 73 {AMOUNT})"));
    conditions.push(format!("(list 51 0x{} {AMOUNT})", hex::encode(DESTINATION)));
    chialisp_oxide_reference::compile(&format!("(mod () (list {}))", conditions.join(" "))).unwrap()
}

fn spend_for(reveal: &[u8]) -> CoinSpend {
    let mut simulator = Simulator::new();
    let coin = create_coin(&mut simulator, reveal, AMOUNT).unwrap();
    coin_spend(coin, reveal, &[0x80]).unwrap()
}

#[test]
fn nonempty_signature_from_wrong_key_is_a_consensus_failure_with_full_rollback() {
    let owner = SecretKey::from_seed(&[0x17; 32]);
    let wrong_key = SecretKey::from_seed(&[0x18; 32]);
    assert_ne!(owner.public_key(), wrong_key.public_key());
    let reveal = payment(&owner, 1);
    let mut simulator = Simulator::new();
    let coin = create_coin(&mut simulator, &reveal, AMOUNT).unwrap();
    let spend = coin_spend(coin, &reveal, &[0x80]).unwrap();
    let spends = std::slice::from_ref(&spend);
    let signature = sign_single_requirement_with_wrong_key(spends, &wrong_key).unwrap();
    assert_ne!(signature, Signature::default());
    // Spell AGG_SIG_ME's final message independently of RequiredSignature.
    let mut message = MESSAGE.to_vec();
    message.extend_from_slice(coin.coin_id().as_ref());
    message.extend_from_slice(TESTNET11_CONSTANTS.agg_sig_me_additional_data.as_ref());
    assert_eq!(signature, chia_bls::sign(&wrong_key, &message));
    assert!(chia_bls::verify(
        &signature,
        &wrong_key.public_key(),
        &message
    ));
    assert!(!chia_bls::verify(&signature, &owner.public_key(), &message));
    // Supplying a wrong key to the partial helper would only omit it. Keep
    // this genuine nonempty wrong-key control distinct from missing-signature.
    assert_eq!(
        partial_aggregate_signature(spends, std::slice::from_ref(&wrong_key)).unwrap(),
        Signature::default()
    );

    let before = simulator.clone();
    require_consensus_rejection(
        simulator.new_transaction(SpendBundle::new(vec![spend.clone()], signature)),
        ConsensusRejection::BadAggregateSignature,
    )
    .unwrap();
    assert_eq!(simulator, before);

    // An independently submitted positive case starts from the identical
    // unspent fixture and retains the original reveal and solution.
    let mut positive = before;
    let matching = sign_transaction(spends, std::slice::from_ref(&owner)).unwrap();
    assert_ne!(matching, Signature::default());
    assert_eq!(matching, chia_bls::sign(&owner, &message));
    positive
        .new_transaction(SpendBundle::new(vec![spend], matching))
        .unwrap();
    assert!(
        positive
            .coin_state(coin.coin_id())
            .unwrap()
            .spent_height
            .is_some()
    );
    let children = positive.children(coin.coin_id());
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].coin.parent_coin_info, coin.coin_id());
    assert_eq!(children[0].coin.puzzle_hash, Bytes32::new(DESTINATION));
    assert_eq!(children[0].coin.amount, AMOUNT);
}

#[test]
fn wrong_key_control_requires_one_bls_pair_a_different_key_and_a_nonempty_signature() {
    let owner = SecretKey::from_seed(&[0x17; 32]);
    let wrong_key = SecretKey::from_seed(&[0x18; 32]);
    for spends in [
        vec![],
        vec![spend_for(&payment(&owner, 0))],
        vec![spend_for(&payment(&owner, 2))],
        vec![
            spend_for(&payment(&owner, 1)),
            spend_for(&payment(&owner, 1)),
        ],
    ] {
        let error = sign_single_requirement_with_wrong_key(&spends, &wrong_key).unwrap_err();
        assert!(error.to_string().contains("exactly one BLS requirement"));
    }
    let spend = spend_for(&payment(&owner, 1));
    let spends = std::slice::from_ref(&spend);
    let error = sign_single_requirement_with_wrong_key(spends, &owner).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("different from the required public key")
    );
    let zero_key = SecretKey::from_bytes(&[0; 32]).unwrap();
    let error = sign_single_requirement_with_wrong_key(spends, &zero_key).unwrap_err();
    assert!(error.to_string().contains("nonempty signature"));
}

fn secp_reveal() -> Vec<u8> {
    use clvmr::{Allocator, NodePtr, serde::node_to_bytes};

    fn list(allocator: &mut Allocator, items: &[NodePtr]) -> NodePtr {
        items.iter().rev().fold(NodePtr::NIL, |rest, first| {
            allocator.new_pair(*first, rest).unwrap()
        })
    }
    fn quote(allocator: &mut Allocator, bytes: &[u8]) -> NodePtr {
        let one = allocator.new_atom(&[1]).unwrap();
        let value = allocator.new_atom(bytes).unwrap();
        allocator.new_pair(one, value).unwrap()
    }

    let mut allocator = Allocator::new();
    // Compressed secp256k1 generator point. The SDK signer dialect collects
    // this requirement without requiring its placeholder signature to verify.
    let key =
        hex::decode("0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798").unwrap();
    let opcode = allocator.new_atom(&[0x13, 0xd6, 0x1f, 0x00]).unwrap();
    let key = quote(&mut allocator, &key);
    let message = quote(&mut allocator, &[0x2a; 32]);
    let signature = quote(&mut allocator, &[1, 2, 3]);
    let verify = list(&mut allocator, &[opcode, key, message, signature]);
    let select = allocator.new_atom(&[3]).unwrap();
    let nil = quote(&mut allocator, &[]);
    // (i VERIFY (q . ()) (q . ())) runs VERIFY and returns valid empty
    // conditions. Construct directly so no optimizer can erase the control.
    let program = list(&mut allocator, &[select, verify, nil, nil]);
    node_to_bytes(&allocator, program).unwrap()
}

#[test]
fn a_real_collected_secp_requirement_is_not_accepted_as_bls() {
    let spend = spend_for(&secp_reveal());
    let spends = std::slice::from_ref(&spend);
    let mut allocator = clvmr::Allocator::new();
    let constants = AggSigConstants::new(TESTNET11_CONSTANTS.agg_sig_me_additional_data);
    let required = RequiredSignature::from_coin_spends(&mut allocator, spends, &constants).unwrap();
    assert!(matches!(required.as_slice(), [RequiredSignature::Secp(_)]));
    let wrong_key = SecretKey::from_seed(&[0x18; 32]);
    let error = sign_single_requirement_with_wrong_key(spends, &wrong_key).unwrap_err();
    assert!(error.to_string().contains("exactly one BLS requirement"));
}
