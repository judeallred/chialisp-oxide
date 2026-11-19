//! Original explicit two-payout continuation experiment for issue #16.
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
/// Exactly two authenticated openings, in queue order.
pub type BatchOpening = (Opening, Opening);
/// Terminal output has three conditions; a nonterminal output has four.
pub type Conditions = Cons<AssertMyAmount, Cons<CreateCoin, Cons<CreateCoin, Option<CreateCoin>>>>;

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
        2 => atom_hash(&[2]),
        4 => atom_hash(&[4]),
        6 => atom_hash(&[6]),
        8 => atom_hash(&[8]),
        _ => panic!(),
    }
}

/// Hash the exact proper Terms list, then its standard one-parameter curry.
/// Counts are exactly 2, 4, 6, 8; their canonical integer payload is one byte.
pub fn instance_hash(terms: Terms) -> PuzzleHash {
    let (module, head, remaining) = terms;
    assert!(remaining == 2 || remaining == 4 || remaining == 6 || remaining == 8);
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
        2 => Amount::from_mojos(2_000),
        4 => Amount::from_mojos(4_000),
        6 => Amount::from_mojos(6_000),
        8 => Amount::from_mojos(8_000),
        _ => panic!(),
    }
}

#[chialisp_oxide::puzzle]
pub fn step(terms: Terms, openings: BatchOpening) -> Conditions {
    let (module, head, remaining) = terms;
    let ((first_recipient, middle), (second_recipient, next)) = openings;
    assert!(remaining == 2 || remaining == 4 || remaining == 6 || remaining == 8);
    assert!(item(first_recipient, middle).0 == head.0);
    assert!(middle.0 != terminal().0);
    assert!(item(second_recipient, next).0 == middle.0);
    // Equal amounts to an identical PH under one parent create duplicate coins.
    // Batch one permits these recipients in separate spends; this policy does not.
    assert!(first_recipient.0 != second_recipient.0);
    let continuation = if remaining == 2 {
        let _ = next;
        None
    } else {
        assert!(next.0 != terminal().0);
        let count = remaining - 2;
        Some(create_coin(
            instance_hash((module, next, count)),
            remaining_budget(count),
        ))
    };
    Cons {
        first: assert_my_amount(remaining_budget(remaining)),
        rest: Cons {
            first: create_coin(first_recipient, PAYOUT),
            rest: Cons {
                first: create_coin(second_recipient, PAYOUT),
                rest: continuation,
            },
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

    fn fixture() -> (PuzzleHash, [PuzzleHash; 4], [Message32; 5]) {
        (
            PuzzleHash([0xaa; 32]),
            [
                PuzzleHash([0x11; 32]),
                PuzzleHash([0x22; 32]),
                PuzzleHash([0x33; 32]),
                PuzzleHash([0x44; 32]),
            ],
            [
                Message32(bytes(
                    "7e124616984ebc64697bc2cd4c9ad74ea5907ce8a5df900ff97e842cfe64c3c4",
                )),
                Message32(bytes(
                    "a99cd0d5303887b4b233a3cc5307fdf209c3cf1a911dbc4886d44362b334dcc1",
                )),
                Message32(bytes(
                    "7ffdd0c444ebe751f4482e75f8499cffbbb0dc8d62e826508571013b4b60b4d4",
                )),
                Message32(bytes(
                    "2c3b374a6c2a86346511713131bdd90098f014af7c5881b60b99f97813e1492e",
                )),
                Message32(bytes(
                    "bd41c08fb4a96f6379af5f44862570349f94b936e8e94ea3fadf91bbde9c9699",
                )),
            ],
        )
    }

    #[test]
    fn independent_python_commitment_and_curry_fixtures_match_native() {
        let (module, recipients, heads) = fixture();
        assert_eq!(terminal().0, heads[4].0);
        for i in 0..4 {
            assert_eq!(item(recipients[i], heads[i + 1]).0, heads[i].0);
        }
        assert_eq!(
            instance_hash((module, heads[0], 4)).0,
            bytes("36e673bb2991a4d331cf23319e6fc91b497f6144424c872c0f264a8df16da64c")
        );
        assert_eq!(
            instance_hash((module, heads[2], 2)).0,
            bytes("d7aa4724f76f899c36a154775ebdca4f9625a40586f8e37d7564c86ca5088f38")
        );
    }

    #[test]
    fn two_native_batches_preserve_order_budget_and_terminal_output() {
        let (module, recipients, heads) = fixture();
        for index in [0, 2] {
            let count = 4 - index as u8;
            let result = step(
                (module, heads[index], count),
                (
                    (recipients[index], heads[index + 1]),
                    (recipients[index + 1], heads[index + 2]),
                ),
            );
            assert_eq!(result.first.0, 73);
            assert_eq!(result.first.1.mojos(), 1000 * u64::from(count));
            assert_eq!(result.rest.first.0, 51);
            assert_eq!(result.rest.first.1.0, recipients[index].0);
            assert_eq!(result.rest.first.2.mojos(), 1000);
            assert_eq!(result.rest.rest.first.0, 51);
            assert_eq!(result.rest.rest.first.1.0, recipients[index + 1].0);
            assert_eq!(result.rest.rest.first.2.mojos(), 1000);
            if count == 2 {
                assert!(result.rest.rest.rest.is_none());
            } else {
                let successor = result.rest.rest.rest.unwrap();
                assert_eq!(successor.0, 51);
                assert_eq!(
                    successor.1.0,
                    instance_hash((module, heads[index + 2], count - 2)).0
                );
                assert_eq!(successor.2.mojos(), 1000 * u64::from(count - 2));
                assert_eq!(2000 + successor.2.mojos(), result.first.1.mojos());
            }
        }
    }

    #[test]
    fn native_rejects_skipped_reordered_injected_and_malformed_openings() {
        let (module, recipients, heads) = fixture();
        let first = (recipients[0], heads[1]);
        let second = (recipients[1], heads[2]);
        for openings in [
            (second, first),
            ((recipients[2], heads[3]), second),
            ((PuzzleHash([0x99; 32]), heads[1]), second),
            ((recipients[0], heads[2]), second),
            (first, (recipients[2], heads[3])),
            (first, (PuzzleHash([0x99; 32]), heads[2])),
            (first, (recipients[1], heads[3])),
        ] {
            assert!(catch_unwind(|| step((module, heads[0], 4), openings)).is_err());
        }
    }

    #[test]
    fn native_rejects_early_end_and_excess_continuation() {
        let (module, recipients, heads) = fixture();
        assert!(
            catch_unwind(|| step(
                (module, heads[0], 2),
                ((recipients[0], heads[1]), (recipients[1], heads[2]))
            ))
            .is_err()
        );
        assert!(
            catch_unwind(|| step(
                (module, heads[2], 4),
                ((recipients[2], heads[3]), (recipients[3], heads[4]))
            ))
            .is_err()
        );
        let head = item(recipients[0], terminal());
        assert!(
            catch_unwind(|| step(
                (module, head, 2),
                ((recipients[0], terminal()), (recipients[1], terminal()))
            ))
            .is_err()
        );
    }

    #[test]
    fn identical_recipients_with_valid_openings_reject_duplicate_coin_outputs() {
        let (module, recipients, _) = fixture();
        let middle = item(recipients[0], terminal());
        let head = item(recipients[0], middle);
        assert!(
            catch_unwind(|| step(
                (module, head, 2),
                ((recipients[0], middle), (recipients[0], terminal()))
            ))
            .is_err()
        );
    }

    #[test]
    fn finite_even_budget_and_count_tables_and_invalid_counts() {
        let (module, recipients, heads) = fixture();
        for count in [0, 1, 3, 5, 7, 9, u8::MAX] {
            assert!(
                catch_unwind(|| step(
                    (module, heads[0], count),
                    ((recipients[0], heads[1]), (recipients[1], heads[2]))
                ))
                .is_err()
            );
            assert!(catch_unwind(|| instance_hash((module, heads[0], count))).is_err());
            assert!(catch_unwind(|| remaining_budget(count)).is_err());
        }
        for count in [2, 4, 6, 8] {
            assert_eq!(count_atom_hash(count), atom_hash(&[count]));
            assert_eq!(remaining_budget(count).mojos(), 1000 * u64::from(count));
        }
        // A template step with count eight does not validate its future queue.
        let result = step(
            (module, heads[0], 8),
            ((recipients[0], heads[1]), (recipients[1], heads[2])),
        );
        assert_eq!(result.first.1.mojos(), 8000);
        assert_eq!(result.rest.rest.rest.unwrap().2.mojos(), 6000);
    }

    #[test]
    fn complete_valid_even_queues_replay_to_terminal_in_native_rust() {
        let module = PuzzleHash([0xaa; 32]);
        for count in [2u8, 4, 6, 8] {
            let recipients = (0..count)
                .map(|index| PuzzleHash([index + 1; 32]))
                .collect::<Vec<_>>();
            let mut heads = vec![terminal()];
            for recipient in recipients.iter().rev() {
                heads.insert(0, item(*recipient, heads[0]));
            }
            let mut paid = 0;
            for index in (0..usize::from(count)).step_by(2) {
                let remaining = count - index as u8;
                let output = step(
                    (module, heads[index], remaining),
                    (
                        (recipients[index], heads[index + 1]),
                        (recipients[index + 1], heads[index + 2]),
                    ),
                );
                assert_eq!(output.rest.first.1.0, recipients[index].0);
                assert_eq!(output.rest.rest.first.1.0, recipients[index + 1].0);
                let payout = output.rest.first.2.mojos() + output.rest.rest.first.2.mojos();
                assert_eq!(payout, 2000);
                paid += payout;
                if remaining == 2 {
                    assert!(output.rest.rest.rest.is_none());
                } else {
                    let successor = output.rest.rest.rest.unwrap();
                    assert_eq!(
                        successor.1.0,
                        instance_hash((module, heads[index + 2], remaining - 2)).0
                    );
                    assert_eq!(successor.2.mojos(), u64::from(remaining - 2) * 1000);
                    assert_eq!(payout + successor.2.mojos(), output.first.1.mojos());
                }
            }
            assert_eq!(paid, u64::from(count) * 1000);
        }
    }

    #[test]
    fn selected_module_preflight_is_distinct_from_template_policy() {
        let (module, recipients, heads) = fixture();
        let terms = (module, heads[0], 4);
        assert_eq!(funding_hash(module, terms).0, instance_hash(terms).0);
        let wrong = PuzzleHash([0xbb; 32]);
        assert!(catch_unwind(|| funding_hash(wrong, terms)).is_err());
        // The template cannot intrinsically identify its own selected artifact.
        // A creator choosing another module field gets a different successor.
        let output = step(
            (wrong, heads[0], 4),
            ((recipients[0], heads[1]), (recipients[1], heads[2])),
        );
        assert_eq!(
            output.rest.rest.rest.unwrap().1.0,
            instance_hash((wrong, heads[2], 2)).0
        );
    }
}
