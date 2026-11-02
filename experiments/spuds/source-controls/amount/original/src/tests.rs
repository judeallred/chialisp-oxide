use super::*;
use crate::fixture_keys::{OWNER_PUBLIC_KEY, RECOVERY_PUBLIC_KEY};
use std::panic::catch_unwind;

const MODULE: PuzzleHash = PuzzleHash([0xaa; 32]);
const HOLDER: PublicKey = PublicKey(OWNER_PUBLIC_KEY);
const CHALLENGER: PublicKey = PublicKey(RECOVERY_PUBLIC_KEY);
const PAYOUT: PuzzleHash = PuzzleHash([0x11; 32]);
const NEXT_PAYOUT: PuzzleHash = PuzzleHash([0x22; 32]);

fn bytes(value: &str) -> [u8; 32] {
    assert_eq!(value.len(), 64);
    std::array::from_fn(|i| u8::from_str_radix(&value[i * 2..i * 2 + 2], 16).unwrap())
}

#[test]
fn independent_python_commitment_fixtures_match_native() {
    assert_eq!(
        instance_hash((MODULE, HOLDER, PAYOUT, 1)).0,
        bytes("5c080a7f2c3ce42a21ab82f59cb80fbbb14fe01e234a16bb54a3d11ea443d73e")
    );
    let result = step((MODULE, HOLDER, PAYOUT, 1), true, CHALLENGER, NEXT_PAYOUT);
    assert_eq!(
        result.4.1.0,
        bytes("ea816d3de3e07c6288b68d35718bd6c926076d67d70d3ce85eebce55a7478e14")
    );
    assert_eq!(
        result.0.2.0,
        bytes("bde40448cfe7eb2773f23a9ab1a2bec15b05be912008bed16015d7a36e78387c")
    );
    assert_eq!(
        claim_message().0,
        bytes("f8221fc24f47e0ae4787ed9879fd04c39822f4cbd9a85a0bc4b16381ad160585")
    );
}

#[test]
fn all_seven_takes_recreate_exact_state_and_require_external_top_up() {
    let mut key = HOLDER;
    let mut payout = PAYOUT;
    for round in 1..8 {
        let next_key = if round % 2 == 1 { CHALLENGER } else { HOLDER };
        let next_payout = if round % 2 == 1 { NEXT_PAYOUT } else { PAYOUT };
        let result = step((MODULE, key, payout, round), true, next_key, next_payout);
        let target = instance_hash((MODULE, next_key, next_payout, round + 1));
        assert_eq!(result.0.0, 50);
        assert_eq!(result.0.1.0, next_key.0);
        assert_eq!(result.0.2.0, take_message(target).0);
        assert_eq!(result.1.0, 73);
        assert_eq!(result.1.1.mojos(), u64::from(round) * 1000);
        assert_eq!(result.2.0, 82);
        assert_eq!(result.2.1.blocks(), 1);
        assert_eq!(result.3.0, 86);
        assert_eq!(result.3.1.blocks(), 5);
        assert_eq!(result.4.0, 51);
        assert_eq!(result.4.1.0, target.0);
        assert_eq!(result.4.2.mojos() - result.1.1.mojos(), 1000);
        assert_eq!(result.5.0, 60);
        assert_eq!(result.5.1.0, result.0.2.0);
        assert_ne!(result.5.1.0, claim_message().0);
        key = next_key;
        payout = next_payout;
    }
}

#[test]
fn every_claim_pays_current_holder_and_emits_after_boundary() {
    for round in 1..=8 {
        let result = step(
            (MODULE, HOLDER, PAYOUT, round),
            false,
            CHALLENGER,
            NEXT_PAYOUT,
        );
        assert_eq!(result.0.0, 50);
        assert_eq!(result.0.1.0, HOLDER.0);
        assert_eq!(result.0.2.0, claim_message().0);
        assert_eq!(result.1.1.mojos(), u64::from(round) * 1000);
        assert_eq!(result.2.0, 82);
        assert_eq!(result.2.1.blocks(), 1);
        assert_eq!(result.3.0, 82);
        assert_eq!(result.3.1.blocks(), 5);
        assert_eq!(result.4.1.0, PAYOUT.0);
        assert_eq!(result.4.2.mojos(), result.1.1.mojos());
        assert_eq!(result.5.1.0, claim_message().0);
    }
}

#[test]
fn take_signature_and_top_up_announcement_bind_complete_successor() {
    let original = step((MODULE, HOLDER, PAYOUT, 1), true, CHALLENGER, NEXT_PAYOUT);
    for result in [
        step((MODULE, HOLDER, PAYOUT, 1), true, HOLDER, NEXT_PAYOUT),
        step((MODULE, HOLDER, PAYOUT, 1), true, CHALLENGER, PAYOUT),
        step((MODULE, HOLDER, PAYOUT, 2), true, CHALLENGER, NEXT_PAYOUT),
        step(
            (PuzzleHash([0xbb; 32]), HOLDER, PAYOUT, 1),
            true,
            CHALLENGER,
            NEXT_PAYOUT,
        ),
    ] {
        assert_ne!(result.4.1.0, original.4.1.0);
        assert_ne!(result.0.2.0, original.0.2.0);
        assert_eq!(result.0.2.0, result.5.1.0);
    }
}

#[test]
fn malformed_rounds_reject_and_maximum_pot_stops_taking() {
    for round in [0, 9, u8::MAX] {
        assert!(catch_unwind(|| pot(round)).is_err());
        assert!(catch_unwind(|| instance_hash((MODULE, HOLDER, PAYOUT, round))).is_err());
        for take in [false, true] {
            assert!(
                catch_unwind(|| step(
                    (MODULE, HOLDER, PAYOUT, round),
                    take,
                    CHALLENGER,
                    NEXT_PAYOUT
                ))
                .is_err()
            );
        }
    }
    assert!(
        catch_unwind(|| step((MODULE, HOLDER, PAYOUT, 8), true, CHALLENGER, NEXT_PAYOUT)).is_err()
    );
    assert_eq!(
        step((MODULE, HOLDER, PAYOUT, 8), false, CHALLENGER, NEXT_PAYOUT)
            .4
            .2
            .mojos(),
        8000
    );
    for round in 1..=8 {
        assert_eq!(round_atom_hash(round), atom_hash(&[round]));
        assert_eq!(pot(round).mojos(), u64::from(round) * 1000);
    }
}

#[test]
fn claim_ignores_next_fields_without_delegating_current_payout() {
    let first = step((MODULE, HOLDER, PAYOUT, 1), false, CHALLENGER, NEXT_PAYOUT);
    let second = step((MODULE, HOLDER, PAYOUT, 1), false, HOLDER, PAYOUT);
    assert_eq!(first.0.1.0, second.0.1.0);
    assert_eq!(first.0.2.0, second.0.2.0);
    assert_eq!(first.4.1.0, second.4.1.0);
    assert_eq!(first.4.2.mojos(), second.4.2.mojos());
}

#[test]
fn selected_artifact_binding_is_separate_from_template_policy() {
    let terms = (MODULE, HOLDER, PAYOUT, 1);
    assert_eq!(funding_hash(MODULE, terms).0, instance_hash(terms).0);
    assert!(catch_unwind(|| funding_hash(PuzzleHash([0xbb; 32]), terms)).is_err());
    let changed = PuzzleHash([0xbb; 32]);
    let result = step((changed, HOLDER, PAYOUT, 1), true, CHALLENGER, NEXT_PAYOUT);
    assert_eq!(
        result.4.1.0,
        instance_hash((changed, CHALLENGER, NEXT_PAYOUT, 2)).0
    );
}
