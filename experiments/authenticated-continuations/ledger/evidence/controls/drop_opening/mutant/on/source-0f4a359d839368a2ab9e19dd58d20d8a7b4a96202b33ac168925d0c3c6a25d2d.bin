//! Original ordered-payout continuation spike for issue #16.
//! WIP: no complete-spend, live-network or compiler-preservation claim.

use chialisp_oxide::curry::{atom_hash, curried_puzzle_hash, pair_hash};
use chialisp_oxide::{
    Amount, AssertMyAmount, Cons, CreateCoin, Message32, PuzzleHash, assert_my_amount, create_coin,
    sha256_parts,
};

/// Each queue item pays this amount. No reserve, remainder, fee or abort branch.
pub const PAYOUT: Amount = Amount::from_mojos(1_000);
/// A deliberately small finite experiment, not a compiler or consensus limit.
pub const MAX_ITEMS: u8 = 8;
const ITEM_DOMAIN: [u8; 40] = *b"chialisp-oxide/continuation/v1/item:1000";
const END_DOMAIN: [u8; 39] = *b"chialisp-oxide/continuation/v1/end:1000";

/// Curried as ONE proper-list argument: (selected module, queue head, remaining).
/// Before funding, a host verifier MUST compare the module field to the actual
/// selected artifact. Merely accepting an arbitrary field here cannot do that.
pub type Terms = (PuzzleHash, Message32, u8);
/// Untrusted opening: (recipient, next queue digest).
pub type Opening = (PuzzleHash, Message32);
/// Terminal output has two conditions; a nonterminal output has exactly three.
pub type Conditions = Cons<AssertMyAmount, Cons<CreateCoin, Option<CreateCoin>>>;

pub fn terminal() -> Message32 {
    Message32(sha256_parts(&END_DOMAIN, &[], &[]))
}

pub fn item(recipient: PuzzleHash, next: Message32) -> Message32 {
    Message32(sha256_parts(&ITEM_DOMAIN, &recipient.0, &next.0))
}

/// Canonical count atoms for the closed finite policy. Dynamic byte-array
/// construction is not admitted by the current frontend; each arm is literal.
pub fn count_atom_hash(remaining: u8) -> [u8; 32] {
    match remaining {
        1 => atom_hash(&[1]),
        2 => atom_hash(&[2]),
        3 => atom_hash(&[3]),
        4 => atom_hash(&[4]),
        5 => atom_hash(&[5]),
        6 => atom_hash(&[6]),
        7 => atom_hash(&[7]),
        8 => atom_hash(&[8]),
        _ => panic!(),
    }
}

/// Hash the exact proper Terms list, then its standard one-parameter curry.
/// Counts are strictly 1..=8; their canonical integer payload is one byte.
pub fn instance_hash(terms: Terms) -> PuzzleHash {
    let (module, head, remaining) = terms;
    assert!(remaining > 0);
    assert!(remaining <= MAX_ITEMS);
    let nil = atom_hash(&[]);
    let count_list = pair_hash(&count_atom_hash(remaining), &nil);
    let head_list = pair_hash(&atom_hash(&head.0), &count_list);
    let terms_hash = pair_hash(&atom_hash(&module.0), &head_list);
    PuzzleHash(curried_puzzle_hash(&module.0, &terms_hash))
}

/// Fixed finite budget table avoids assuming integer-widening support in the
/// CLVM subset. Native Rust accepts `u8 as u64`; this frontend does not yet.
/// These are exact multiples of PAYOUT, independently checked in native tests.
pub fn remaining_budget(remaining: u8) -> Amount {
    match remaining {
        1 => PAYOUT,
        2 => Amount::from_mojos(2_000),
        3 => Amount::from_mojos(3_000),
        4 => Amount::from_mojos(4_000),
        5 => Amount::from_mojos(5_000),
        6 => Amount::from_mojos(6_000),
        7 => Amount::from_mojos(7_000),
        8 => Amount::from_mojos(8_000),
        _ => panic!(),
    }
}

#[chialisp_oxide::puzzle]
pub fn step(terms: Terms, opening: Opening) -> Conditions {
    let (module, head, remaining) = terms;
    let (recipient, next) = opening;
    assert!(remaining > 0);
    assert!(remaining <= MAX_ITEMS);
    let continuation = if remaining == 1 {
        assert!(next.0 == terminal().0);
        None
    } else {
        assert!(next.0 != terminal().0);
        let count = remaining - 1;
        Some(create_coin(
            instance_hash((module, next, count)),
            remaining_budget(count),
        ))
    };
    Cons {
        first: assert_my_amount(remaining_budget(remaining)),
        rest: Cons {
            first: create_coin(recipient, PAYOUT),
            rest: continuation,
        },
    }
}

/// Host funding preflight, not a second on-chain export or authenticated build.
/// The caller must derive `selected_module` from the selected serialized artifact.
pub fn funding_hash(selected_module: PuzzleHash, terms: Terms) -> PuzzleHash {
    assert!(selected_module.0 == terms.0.0);
    instance_hash(terms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::catch_unwind;

    fn bytes(text: &str) -> [u8; 32] {
        assert_eq!(text.len(), 64);
        std::array::from_fn(|i| u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap())
    }

    fn fixture() -> (PuzzleHash, [PuzzleHash; 3], [Message32; 4]) {
        (
            PuzzleHash([0xaa; 32]),
            [
                PuzzleHash([0x11; 32]),
                PuzzleHash([0x22; 32]),
                PuzzleHash([0x33; 32]),
            ],
            [
                Message32(bytes(
                    "dcc697246e5b5e236fca8d601cdb397146471c7a6ec1159444b2fc74e7534815",
                )),
                Message32(bytes(
                    "4f935a856fb28c9e376dfe8dd01d191b00b50ef5c82daec4e59cf912c8b8b563",
                )),
                Message32(bytes(
                    "a6e94a7b6c7b0d395eefd41355be296b27b184f74f3dbefc10382d59d4515607",
                )),
                Message32(bytes(
                    "bd41c08fb4a96f6379af5f44862570349f94b936e8e94ea3fadf91bbde9c9699",
                )),
            ],
        )
    }

    #[test]
    fn native_matches_independent_python_fixed_byte_and_curry_hashes() {
        let (module, recipients, heads) = fixture();
        assert_eq!(terminal().0, heads[3].0);
        let hashes = [
            "71726a08d4febd01d48bf3c23c6ad22b4727f5ca863bf6e265a887d4df44c3d3",
            "7f846e6f94595803fc199033ee42080b7223e47107b060776ac4341c40b9820e",
            "5d118f71af986f9b1cb2928118aac3560349868a74b39f68d0ec2a096b79f8dd",
        ];
        for i in 0..3 {
            assert_eq!(item(recipients[i], heads[i + 1]).0, heads[i].0);
            assert_eq!(
                instance_hash((module, heads[i], 3 - i as u8)).0,
                bytes(hashes[i])
            );
        }
    }

    #[test]
    fn native_three_steps_conserve_exact_committed_funds_and_end() {
        let (module, recipients, heads) = fixture();
        for i in 0..3 {
            let remaining = 3 - i as u8;
            let result = step((module, heads[i], remaining), (recipients[i], heads[i + 1]));
            assert_eq!(result.first.0, 73);
            assert_eq!(result.first.1.mojos(), 1000 * u64::from(remaining));
            assert_eq!(result.rest.first.0, 51);
            assert_eq!(result.rest.first.1.0, recipients[i].0);
            assert_eq!(result.rest.first.2.mojos(), 1000);
            if remaining == 1 {
                assert!(result.rest.rest.is_none());
            } else {
                let next = result.rest.rest.unwrap();
                assert_eq!(next.0, 51);
                assert_eq!(
                    next.1.0,
                    instance_hash((module, heads[i + 1], remaining - 1)).0
                );
                assert_eq!(next.2.mojos(), 1000 * u64::from(remaining - 1));
                assert_eq!(
                    result.rest.first.2.mojos() + next.2.mojos(),
                    result.first.1.mojos()
                );
            }
        }
    }

    #[test]
    fn native_rejects_skipped_reordered_injected_items_and_wrong_next() {
        let (module, recipients, heads) = fixture();
        for opening in [
            (recipients[1], heads[2]),
            (recipients[1], heads[1]),
            (PuzzleHash([0x44; 32]), heads[1]),
            (recipients[0], heads[2]),
        ] {
            assert!(catch_unwind(|| step((module, heads[0], 3), opening)).is_err());
        }
    }

    #[test]
    fn native_rejects_early_end_and_excess_continuation() {
        let (module, recipients, heads) = fixture();
        assert!(catch_unwind(|| step((module, heads[0], 1), (recipients[0], heads[1]))).is_err());
        assert!(catch_unwind(|| step((module, heads[2], 2), (recipients[2], heads[3]))).is_err());
    }

    #[test]
    fn native_rejects_count_bounds_and_accepts_maximum() {
        let (module, recipients, heads) = fixture();
        for count in [0, 9, u8::MAX] {
            assert!(
                catch_unwind(|| step((module, heads[0], count), (recipients[0], heads[1])))
                    .is_err()
            );
        }
        let out = step((module, heads[0], MAX_ITEMS), (recipients[0], heads[1]));
        assert_eq!(out.first.1.mojos(), 8000);
        assert_eq!(out.rest.rest.unwrap().2.mojos(), 7000);
        // This accepted template step alone does not validate the entire future
        // queue. Funding must reconstruct all items/count before committing.
    }

    #[test]
    fn complete_budget_table_matches_fixed_payout_arithmetic() {
        for count in 1..=MAX_ITEMS {
            assert_eq!(count_atom_hash(count), atom_hash(&[count]));
            assert_eq!(
                remaining_budget(count).mojos(),
                PAYOUT.mojos() * u64::from(count)
            );
        }
    }

    #[test]
    fn funding_preflight_rejects_wrong_selected_module() {
        let (module, _, heads) = fixture();
        let terms = (module, heads[0], 3);
        assert_eq!(funding_hash(module, terms).0, instance_hash(terms).0);
        assert!(catch_unwind(|| funding_hash(PuzzleHash([0xbb; 32]), terms)).is_err());
    }
}
