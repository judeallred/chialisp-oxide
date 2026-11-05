//! Independent wire-format fixtures for complementary absolute time conditions.
//! These exercise the pinned TESTNET11 simulator, not compiler preservation.
use chialisp_oxide_simulator::{
    Bytes32, ConsensusRejection, Signature, Simulator, SpendBundle, coin_spend, create_coin,
    require_consensus_rejection,
};

const DEADLINE: u64 = 100;
const AMOUNT: u64 = 42;
const DESTINATION: [u8; 32] = [0x42; 32];

fn reveal(time_opcode: u64) -> Vec<u8> {
    // Literal wire opcodes here are independent of the SDK constructors.
    chialisp_oxide_reference::compile(&format!(
        "(mod () (list (list {time_opcode} {DEADLINE}) (list 73 {AMOUNT}) (list 51 0x{} {AMOUNT})))",
        hex::encode(DESTINATION)
    ))
    .unwrap()
}

fn check_at_timestamp(reveal: &[u8], timestamp: u64, expected: Option<ConsensusRejection>) {
    let mut simulator = Simulator::new();
    let coin = create_coin(&mut simulator, reveal, AMOUNT).unwrap();
    simulator.set_next_timestamp(timestamp).unwrap();
    assert_eq!(simulator.next_timestamp(), timestamp);
    assert!(simulator.children(coin.coin_id()).is_empty());
    assert!(
        simulator
            .coin_state(coin.coin_id())
            .unwrap()
            .spent_height
            .is_none()
    );
    let spend = coin_spend(coin, reveal, &[0x80]).unwrap();
    let before = simulator.clone();
    let result = simulator.new_transaction(SpendBundle::new(vec![spend], Signature::default()));
    if let Some(expected) = expected {
        require_consensus_rejection(result, expected).unwrap();
        assert_eq!(
            simulator, before,
            "rejection must roll back the full ledger"
        );
    } else {
        result.unwrap();
        assert_ne!(simulator, before);
        assert!(
            simulator
                .coin_state(coin.coin_id())
                .unwrap()
                .spent_height
                .is_some()
        );
        let children = simulator.children(coin.coin_id());
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].coin.parent_coin_info, coin.coin_id());
        assert_eq!(children[0].coin.puzzle_hash, Bytes32::new(DESTINATION));
        assert_eq!(children[0].coin.amount, AMOUNT);
        assert!(children[0].spent_height.is_none());
    }
}

#[test]
fn before_deadline_accepts_only_the_timestamp_strictly_before_it() {
    let expected = ConsensusRejection::AssertBeforeSecondsAbsoluteFailed;
    assert_eq!(u32::from(expected.error_code()), 128);
    let reveal = reveal(85);
    for (timestamp, outcome) in [
        (DEADLINE - 1, None),
        (DEADLINE, Some(expected)),
        (DEADLINE + 1, Some(expected)),
    ] {
        check_at_timestamp(&reveal, timestamp, outcome);
    }
}

#[test]
fn at_or_after_deadline_accepts_the_deadline_and_later_timestamp() {
    let reveal = reveal(81);
    for (timestamp, outcome) in [
        (
            DEADLINE - 1,
            Some(ConsensusRejection::AssertSecondsAbsoluteFailed),
        ),
        (DEADLINE, None),
        (DEADLINE + 1, None),
    ] {
        check_at_timestamp(&reveal, timestamp, outcome);
    }
}
