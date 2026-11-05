//! Pinned chia-wallet-sdk condition objects are the wire oracle, not oxide opcodes.
use chia_sdk_types::{Condition, announcement_id};
use clvm_traits::{FromClvm, ToClvm};
use clvmr_sdk::{Allocator, NodePtr, serde::node_to_bytes};

pub const HEIGHT_CASES: [[u32; 4]; 6] = [
    [0, 0, 0, 0],
    [1, 2, 3, 4],
    [127, 128, 255, 256],
    [65_535, 65_536, 16_777_215, 16_777_216],
    [u32::MAX, u32::MAX, u32::MAX, u32::MAX],
    // Constructors emit even inconsistent windows; consensus semantics are a separate lane.
    [10, 20, 9, 19],
];

#[derive(Clone, Copy)]
pub struct AnnouncementCase {
    pub name: &'static str,
    pub origin: [u8; 32],
    pub message: [u8; 32],
    pub digest: [u8; 32],
}

pub fn announcement_cases() -> [AnnouncementCase; 4] {
    [
        AnnouncementCase {
            name: "64 zero bytes",
            origin: [0; 32],
            message: [0; 32],
            digest: bytes32("f5a5fd42d16a20302798ef6ed309979b43003d2320d9f0e8ea9831a92759fb4b"),
        },
        AnnouncementCase {
            name: "bytes 0 through 63",
            origin: std::array::from_fn(|index| index as u8),
            message: std::array::from_fn(|index| (index + 32) as u8),
            digest: bytes32("fdeab9acf3710362bd2658cdc9a29e8f9c757fcf9811603a8c447cd1d9151108"),
        },
        AnnouncementCase {
            name: "origin 11 then message 22",
            origin: [0x11; 32],
            message: [0x22; 32],
            digest: bytes32("5189c77d29fe5d546a045ec46986852785fea5c13ac7da9c115ff5fb6edf817c"),
        },
        AnnouncementCase {
            name: "origin 22 then message 11",
            origin: [0x22; 32],
            message: [0x11; 32],
            digest: bytes32("adfafc05aac733fe9509f43bd1d158c882890351c7f343634c8ef9ea42cdb505"),
        },
    ]
}

fn bytes32(hex: &str) -> [u8; 32] {
    let mut result = [0; 32];
    assert_eq!(hex.len(), result.len() * 2);
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap();
    }
    result
}

pub fn height_conditions([minimum_age, earliest, maximum_age, latest]: [u32; 4]) -> Vec<Condition> {
    vec![
        Condition::assert_height_relative(minimum_age),
        Condition::assert_height_absolute(earliest),
        Condition::assert_before_height_relative(maximum_age),
        Condition::assert_before_height_absolute(latest),
    ]
}

pub fn announcement_conditions(case: AnnouncementCase) -> Vec<Condition> {
    let independent = announcement_id(case.origin.into(), case.message);
    assert_eq!(
        independent.as_ref(),
        case.digest.as_slice(),
        "{}",
        case.name
    );
    vec![
        Condition::create_coin_announcement(case.message.to_vec().into()),
        Condition::assert_coin_announcement(independent),
        Condition::assert_my_coin_id(case.origin.into()),
    ]
}

pub fn supplied_announcement(id: [u8; 32]) -> Vec<Condition> {
    vec![Condition::assert_coin_announcement(id.into())]
}

/// Compare named decoded variants and then exact SDK encoding. The second check
/// also catches extra fields or padding a permissive SDK field parser may ignore.
pub fn assert_exact_conditions(a: &mut Allocator, actual: NodePtr, expected: Vec<Condition>) {
    let parsed =
        Vec::<Condition>::from_clvm(a, actual).expect("decode actual SDK condition variants");
    assert_eq!(parsed, expected);
    let wanted = expected.to_clvm(a).unwrap();
    assert_eq!(
        node_to_bytes(a, actual).unwrap(),
        node_to_bytes(a, wanted).unwrap()
    );
}
