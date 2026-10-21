//! Complete coin-spend validation around serialized compiler outputs.
//!
//! The ledger simulator uses chia-sdk-test 0.36.0, chia-consensus 0.36.1,
//! clvmr 0.16.2, and TESTNET11 constants. This is deliberately separate from the
//! primary differential executor's clvmr 0.21.0. A passing spend here establishes
//! the tested behavior under these simulator rules, not mainnet admission at
//! every historical or future fork height.

use anyhow::{Context, Result, bail, ensure};
use chia_sdk_signer::{AggSigConstants, RequiredSignature};
use chia_sdk_types::TESTNET11_CONSTANTS;
use clvmr::serde::serialized_length_from_bytes;

pub use chia_bls::{SecretKey, Signature};
pub use chia_consensus::validation_error::ErrorCode;
pub use chia_protocol::{Bytes32, Coin, CoinSpend, Program, SpendBundle};
pub use chia_sdk_test::{Simulator, SimulatorConfig, SimulatorError, sign_transaction};

pub const SIMULATOR_VERSION: &str = "chia-sdk-test 0.36.0";
pub const CONSENSUS_VERSION: &str = "chia-consensus 0.36.1";
pub const VM_VERSION: &str = "clvmr 0.16.2";
pub const NETWORK: &str = "TESTNET11";
pub const REJECTION_PROTOCOL: &str = "chia-consensus-0.36.1/v1";

/// Exact semantic consensus failures used by the ledger evidence. This is a
/// deliberately small expectation set, not a classifier for every consensus or
/// clvmr 0.16.2 failure. In particular, GeneratorRuntimeError conflates semantic,
/// resource, and codec failures upstream and must not confirm these claims.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsensusRejection {
    BadAggregateSignature,
    AssertSecondsAbsoluteFailed,
    AssertMyAmountFailed,
    DoubleSpend,
    MintingCoin,
    AssertCoinAnnouncementFailed,
    MessageNotSentOrReceived,
    InvalidPuzzleHash,
    InvalidCoinAmount,
    AssertBeforeSecondsAbsoluteFailed,
}

impl ConsensusRejection {
    pub const ALL: &'static [Self] = &[
        Self::BadAggregateSignature,
        Self::AssertSecondsAbsoluteFailed,
        Self::AssertMyAmountFailed,
        Self::DoubleSpend,
        Self::MintingCoin,
        Self::AssertCoinAnnouncementFailed,
        Self::MessageNotSentOrReceived,
        Self::InvalidPuzzleHash,
        Self::InvalidCoinAmount,
        Self::AssertBeforeSecondsAbsoluteFailed,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BadAggregateSignature => "BadAggregateSignature",
            Self::AssertSecondsAbsoluteFailed => "AssertSecondsAbsoluteFailed",
            Self::AssertMyAmountFailed => "AssertMyAmountFailed",
            Self::DoubleSpend => "DoubleSpend",
            Self::MintingCoin => "MintingCoin",
            Self::AssertCoinAnnouncementFailed => "AssertCoinAnnouncementFailed",
            Self::MessageNotSentOrReceived => "MessageNotSentOrReceived",
            Self::InvalidPuzzleHash => "InvalidPuzzleHash",
            Self::InvalidCoinAmount => "InvalidCoinAmount",
            Self::AssertBeforeSecondsAbsoluteFailed => "AssertBeforeSecondsAbsoluteFailed",
        }
    }

    pub const fn error_code(self) -> ErrorCode {
        match self {
            Self::BadAggregateSignature => ErrorCode::BadAggregateSignature,
            Self::AssertSecondsAbsoluteFailed => ErrorCode::AssertSecondsAbsoluteFailed,
            Self::AssertMyAmountFailed => ErrorCode::AssertMyAmountFailed,
            Self::DoubleSpend => ErrorCode::DoubleSpend,
            Self::MintingCoin => ErrorCode::MintingCoin,
            Self::AssertCoinAnnouncementFailed => ErrorCode::AssertCoinAnnouncementFailed,
            Self::MessageNotSentOrReceived => ErrorCode::MessageNotSentOrReceived,
            Self::InvalidPuzzleHash => ErrorCode::InvalidPuzzleHash,
            Self::InvalidCoinAmount => ErrorCode::InvalidCoinAmount,
            Self::AssertBeforeSecondsAbsoluteFailed => ErrorCode::AssertBeforeSecondsAbsoluteFailed,
        }
    }
}

/// Require the actual consensus code, not any signing, runtime, or host error.
pub fn require_consensus_rejection<T>(
    result: std::result::Result<T, SimulatorError>,
    expected: ConsensusRejection,
) -> Result<ConsensusRejection> {
    match result {
        Err(SimulatorError::Validation(code)) if code == expected.error_code() => Ok(expected),
        Err(error) => bail!(
            "expected consensus {} rejection, got {error:?}",
            expected.as_str()
        ),
        Ok(_) => bail!(
            "expected consensus {} rejection, transaction succeeded",
            expected.as_str()
        ),
    }
}

/// Produce an intentionally incomplete BLS aggregate for negative TESTNET11
/// ledger tests. Messages and public keys come from execution of these actual
/// spends, including AGG_SIG_ME coin/domain binding. Only matching supplied
/// keys contribute; missing keys remain absent. Secp conditions are unsupported.
pub fn partial_aggregate_signature(spends: &[CoinSpend], keys: &[SecretKey]) -> Result<Signature> {
    let mut allocator = clvmr::Allocator::new();
    let constants = AggSigConstants::new(TESTNET11_CONSTANTS.agg_sig_me_additional_data);
    let required = RequiredSignature::from_coin_spends(&mut allocator, spends, &constants)?;
    let mut signature = Signature::default();
    for required in required {
        let RequiredSignature::Bls(required) = required else {
            bail!("partial BLS signature fixture does not support secp conditions");
        };
        if let Some(key) = keys
            .iter()
            .find(|key| key.public_key() == required.public_key)
        {
            signature += &chia_bls::sign(key, required.message());
        }
    }
    Ok(signature)
}

/// Sign one actual TESTNET11 BLS requirement with a deliberately different key.
/// This negative-test helper retains the required message's coin/domain binding;
/// it does not omit the signature or alter the spend. Submit the returned
/// signature through `new_transaction` to test consensus rejection directly.
pub fn sign_single_requirement_with_wrong_key(
    spends: &[CoinSpend],
    wrong_key: &SecretKey,
) -> Result<Signature> {
    let mut allocator = clvmr::Allocator::new();
    let constants = AggSigConstants::new(TESTNET11_CONSTANTS.agg_sig_me_additional_data);
    let required = RequiredSignature::from_coin_spends(&mut allocator, spends, &constants)?;
    let [RequiredSignature::Bls(required)] = required.as_slice() else {
        bail!(
            "wrong-key signature control requires exactly one BLS requirement and no secp requirements"
        );
    };
    ensure!(
        wrong_key.public_key() != required.public_key,
        "wrong-key signature control requires a key different from the required public key"
    );
    let signature = chia_bls::sign(wrong_key, required.message());
    ensure!(
        signature != Signature::default(),
        "wrong-key signature control must produce a nonempty signature"
    );
    Ok(signature)
}

/// Decode exactly one serialized CLVM object, rejecting truncation and trailing
/// bytes before passing it to upstream APIs that assume well-formed programs.
pub fn program(bytes: &[u8]) -> Result<Program> {
    let length = serialized_length_from_bytes(bytes).context("invalid CLVM serialization")?;
    ensure!(
        length == bytes.len() as u64,
        "trailing bytes after CLVM object"
    );
    Ok(Program::from(bytes))
}

/// Hash the actual compiled reveal, rather than a source text or its byte hash.
pub fn puzzle_hash(bytes: &[u8]) -> Result<Bytes32> {
    program(bytes)?;
    let hash = clvm_utils::tree_hash_from_bytes(bytes).context("hashing puzzle reveal")?;
    Ok(hash.into())
}

/// Mint a deterministic fixture coin locked to the actual compiled program.
pub fn create_coin(simulator: &mut Simulator, reveal: &[u8], amount: u64) -> Result<Coin> {
    Ok(simulator.new_coin(puzzle_hash(reveal)?, amount))
}

/// Build a spend from serialized compiler output and solution, checking that
/// the reveal is the one committed to by the coin before transaction validation.
pub fn coin_spend(coin: Coin, reveal: &[u8], solution: &[u8]) -> Result<CoinSpend> {
    ensure!(
        puzzle_hash(reveal)? == coin.puzzle_hash,
        "puzzle reveal does not match coin"
    );
    Ok(CoinSpend::new(coin, program(reveal)?, program(solution)?))
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;

    const DESTINATION: [u8; 32] = [0x42; 32];

    fn compile(body: &str) -> Vec<u8> {
        chialisp_oxide_reference::compile(&format!("(mod () {body})")).unwrap()
    }

    fn fixed_payment(amount: u64, assertion: u64) -> Vec<u8> {
        compile(&format!(
            "(list (list 73 {assertion}) (list 51 0x{} {amount}))",
            hex::encode(DESTINATION)
        ))
    }

    fn unsigned(spends: Vec<CoinSpend>) -> SpendBundle {
        SpendBundle::new(spends, Signature::default())
    }

    #[test]
    fn ledger_protocol_matches_the_exact_typed_expectations() {
        let protocol: serde_json::Value =
            serde_json::from_str(include_str!("../rejection-protocol.json")).unwrap();
        assert_eq!(protocol["schema_version"], 1);
        assert_eq!(protocol["protocol"], REJECTION_PROTOCOL);
        assert_eq!(protocol["stage"], "consensus");
        let codes = ConsensusRejection::ALL
            .iter()
            .map(|code| code.as_str())
            .collect::<Vec<_>>();
        assert_eq!(protocol["codes"], serde_json::json!(codes));
        for expected in ConsensusRejection::ALL {
            assert_eq!(format!("{:?}", expected.error_code()), expected.as_str());
            let actual = require_consensus_rejection::<()>(
                Err(SimulatorError::Validation(expected.error_code())),
                *expected,
            )
            .unwrap();
            assert_eq!(actual, *expected);
        }
    }

    #[test]
    fn consensus_expectations_reject_resource_host_signer_and_unexpected_errors() {
        // Explicit boundary controls; this does not claim to exhaust the old
        // VM's memory/stack through consensus. Its generic runtime code has
        // already discarded the underlying EvalErr and cannot prove rejection.
        for error in [
            SimulatorError::Validation(ErrorCode::CostExceeded),
            SimulatorError::Validation(ErrorCode::GeneratorRuntimeError),
            SimulatorError::Validation(ErrorCode::Unknown),
            SimulatorError::Validation(ErrorCode::AssertMyAmountFailed),
            SimulatorError::MissingKey,
            SimulatorError::Io(std::io::Error::other("injected infrastructure failure")),
        ] {
            assert!(
                require_consensus_rejection::<()>(
                    Err(error),
                    ConsensusRejection::BadAggregateSignature,
                )
                .is_err()
            );
        }
        assert!(
            require_consensus_rejection(Ok(()), ConsensusRejection::BadAggregateSignature).is_err()
        );
    }

    #[test]
    fn one_real_signature_cannot_satisfy_a_two_signature_ledger_spend() {
        let keys = [
            SecretKey::from_seed(&[0x17; 32]),
            SecretKey::from_seed(&[0x18; 32]),
        ];
        let reveal = compile(&format!(
            "(list (list 50 0x{} 0x010203) (list 50 0x{} 0x010203))",
            hex::encode(keys[0].public_key().to_bytes()),
            hex::encode(keys[1].public_key().to_bytes()),
        ));
        let mut simulator = Simulator::new();
        let coin = create_coin(&mut simulator, &reveal, 1).unwrap();
        let spend = coin_spend(coin, &reveal, &[0x80]).unwrap();
        let partial =
            partial_aggregate_signature(std::slice::from_ref(&spend), &keys[..1]).unwrap();
        let second_partial =
            partial_aggregate_signature(std::slice::from_ref(&spend), &keys[1..]).unwrap();
        let complete = sign_transaction(std::slice::from_ref(&spend), &keys).unwrap();
        assert_ne!(
            partial,
            Signature::default(),
            "the partial aggregate must really contain a signature"
        );
        assert_ne!(partial, complete);
        assert_ne!(second_partial, Signature::default());
        assert_eq!(
            &partial + &second_partial,
            complete,
            "independent real signatures combine to the SDK's full aggregate"
        );
        assert_eq!(
            partial_aggregate_signature(std::slice::from_ref(&spend), &keys).unwrap(),
            complete,
            "actual condition messages agree with the SDK's full signer",
        );
        let before = simulator.clone();
        require_consensus_rejection(
            simulator.new_transaction(SpendBundle::new(vec![spend.clone()], partial)),
            ConsensusRejection::BadAggregateSignature,
        )
        .unwrap();
        assert_eq!(simulator, before);
        simulator
            .new_transaction(SpendBundle::new(vec![spend], complete))
            .unwrap();
    }

    #[test]
    fn fixed_destination_and_amount_produce_the_expected_child() {
        let mut simulator = Simulator::new();
        let reveal = fixed_payment(42, 42);
        let coin = create_coin(&mut simulator, &reveal, 42).unwrap();
        let spend = coin_spend(coin, &reveal, &[0x80]).unwrap();
        simulator.new_transaction(unsigned(vec![spend])).unwrap();
        let children = simulator.children(coin.coin_id());
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].coin.amount, 42);
        assert_eq!(children[0].coin.puzzle_hash, Bytes32::new(DESTINATION));
        assert!(
            simulator
                .coin_state(coin.coin_id())
                .unwrap()
                .spent_height
                .is_some()
        );
    }

    #[test]
    fn my_amount_assertion_rejects_the_wrong_parent_amount() {
        let mut simulator = Simulator::new();
        let reveal = fixed_payment(42, 42);
        let coin = create_coin(&mut simulator, &reveal, 43).unwrap();
        let spend = coin_spend(coin, &reveal, &[0x80]).unwrap();
        let before = simulator.clone();
        require_consensus_rejection(
            simulator.new_transaction(unsigned(vec![spend])),
            ConsensusRejection::AssertMyAmountFailed,
        )
        .unwrap();
        assert_eq!(simulator, before);
    }

    #[test]
    fn absolute_timelock_rejects_early_and_accepts_after_time_advances() {
        let mut simulator = Simulator::new();
        let reveal = compile("(list (list 81 100))");
        let coin = create_coin(&mut simulator, &reveal, 1).unwrap();
        let spend = coin_spend(coin, &reveal, &[0x80]).unwrap();
        let before = simulator.clone();
        require_consensus_rejection(
            simulator.new_transaction(unsigned(vec![spend.clone()])),
            ConsensusRejection::AssertSecondsAbsoluteFailed,
        )
        .unwrap();
        assert_eq!(simulator, before);
        simulator.pass_time(100);
        simulator.new_transaction(unsigned(vec![spend])).unwrap();
    }

    #[test]
    fn signature_is_required_and_is_bound_to_the_spent_coin() {
        let mut simulator = Simulator::new();
        let key = SecretKey::from_seed(&[0x17; 32]);
        let reveal = compile(&format!(
            "(list (list 50 0x{} 0x010203))",
            hex::encode(key.public_key().to_bytes())
        ));
        let coin = create_coin(&mut simulator, &reveal, 1).unwrap();
        let spend = coin_spend(coin, &reveal, &[0x80]).unwrap();
        let before = simulator.clone();
        require_consensus_rejection(
            simulator.new_transaction(unsigned(vec![spend.clone()])),
            ConsensusRejection::BadAggregateSignature,
        )
        .unwrap();
        assert_eq!(simulator, before);

        simulator.spend_coins(vec![spend.clone()], &[key]).unwrap();

        // Replay the successfully signed bundle against another otherwise
        // identical coin: AGG_SIG_ME includes the original coin ID.
        let signature =
            chia_sdk_test::sign_transaction(&[spend], &[SecretKey::from_seed(&[0x17; 32])])
                .unwrap();
        let other = create_coin(&mut simulator, &reveal, 1).unwrap();
        let other_spend = coin_spend(other, &reveal, &[0x80]).unwrap();
        let before = simulator.clone();
        require_consensus_rejection(
            simulator.new_transaction(SpendBundle::new(vec![other_spend], signature)),
            ConsensusRejection::BadAggregateSignature,
        )
        .unwrap();
        assert_eq!(simulator, before);
    }

    #[test]
    fn creating_more_value_than_the_inputs_is_rejected_atomically() {
        let mut simulator = Simulator::new();
        let reveal = fixed_payment(43, 42);
        let coin = create_coin(&mut simulator, &reveal, 42).unwrap();
        let spend = coin_spend(coin, &reveal, &[0x80]).unwrap();
        let before = simulator.clone();
        require_consensus_rejection(
            simulator.new_transaction(unsigned(vec![spend])),
            ConsensusRejection::MintingCoin,
        )
        .unwrap();
        assert_eq!(simulator, before);
        assert!(simulator.children(coin.coin_id()).is_empty());
    }

    #[test]
    fn announcements_require_the_matching_spend_in_the_same_bundle() {
        let mut simulator = Simulator::new();
        let creator_reveal = compile("(list (list 60 0x010203))");
        let creator = create_coin(&mut simulator, &creator_reveal, 1).unwrap();
        let announcement = Sha256::digest([creator.coin_id().as_slice(), &[1, 2, 3]].concat());
        let assertion_reveal =
            compile(&format!("(list (list 61 0x{}))", hex::encode(announcement)));
        let asserter = create_coin(&mut simulator, &assertion_reveal, 1).unwrap();
        let assertion = coin_spend(asserter, &assertion_reveal, &[0x80]).unwrap();
        let before = simulator.clone();
        require_consensus_rejection(
            simulator.new_transaction(unsigned(vec![assertion.clone()])),
            ConsensusRejection::AssertCoinAnnouncementFailed,
        )
        .unwrap();
        assert_eq!(simulator, before);
        let creation = coin_spend(creator, &creator_reveal, &[0x80]).unwrap();
        simulator
            .new_transaction(unsigned(vec![creation, assertion]))
            .unwrap();
    }

    #[test]
    fn message_conditions_bind_both_coin_ids_and_reject_a_different_sender() {
        let mut simulator = Simulator::new();
        let sender_reveal = chialisp_oxide_reference::compile(
            "(mod (receiver_id) (list (list 66 63 0x010203 receiver_id)))",
        )
        .unwrap();
        let receiver_reveal = chialisp_oxide_reference::compile(
            "(mod (sender_id) (list (list 67 63 0x010203 sender_id)))",
        )
        .unwrap();
        let sender = create_coin(&mut simulator, &sender_reveal, 1).unwrap();
        let receiver = create_coin(&mut simulator, &receiver_reveal, 1).unwrap();
        let id_solution = |id: Bytes32| {
            let mut bytes = vec![0xff, 0xa0];
            bytes.extend_from_slice(id.as_slice());
            bytes.push(0x80);
            bytes
        };
        let send = coin_spend(sender, &sender_reveal, &id_solution(receiver.coin_id())).unwrap();
        let wrong_receive = coin_spend(
            receiver,
            &receiver_reveal,
            &id_solution(Bytes32::new([0x99; 32])),
        )
        .unwrap();
        let before = simulator.clone();
        require_consensus_rejection(
            simulator.new_transaction(unsigned(vec![send.clone(), wrong_receive])),
            ConsensusRejection::MessageNotSentOrReceived,
        )
        .unwrap();
        assert_eq!(simulator, before);
        let receive =
            coin_spend(receiver, &receiver_reveal, &id_solution(sender.coin_id())).unwrap();
        simulator
            .new_transaction(unsigned(vec![send, receive]))
            .unwrap();
    }

    #[test]
    fn a_successfully_spent_coin_cannot_be_spent_again() {
        let mut simulator = Simulator::new();
        let reveal = fixed_payment(42, 42);
        let coin = create_coin(&mut simulator, &reveal, 42).unwrap();
        let spend = coin_spend(coin, &reveal, &[0x80]).unwrap();
        simulator
            .new_transaction(unsigned(vec![spend.clone()]))
            .unwrap();
        let before = simulator.clone();
        require_consensus_rejection(
            simulator.new_transaction(unsigned(vec![spend])),
            ConsensusRejection::DoubleSpend,
        )
        .unwrap();
        assert_eq!(simulator, before);
    }

    #[test]
    fn serialized_bridge_rejects_malformed_inputs_and_wrong_reveals() {
        for malformed in [&[][..], &[0xff][..], &[0x80, 0x80][..], &[0x82, 1][..]] {
            assert!(program(malformed).is_err());
        }
        let mut simulator = Simulator::new();
        let reveal = fixed_payment(42, 42);
        let coin = create_coin(&mut simulator, &reveal, 42).unwrap();
        assert!(coin_spend(coin, &compile("()"), &[0x80]).is_err());
        assert!(coin_spend(coin, &reveal, &[0xff]).is_err());
    }
}
