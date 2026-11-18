#![forbid(unsafe_code)]
//! Small reviewed helpers for explicit authenticated continuations.
//!
//! A continuation coin curries its own module hash and a commitment to the
//! remaining work, does one bounded step, and recreates itself with the next
//! commitment. These are ordinary functions over the reviewed SHA bridge and
//! the standard curry arithmetic; the compiler lowers their bodies, and their
//! names grant no behavior. They do not choose a policy: order, funds,
//! termination and authorization stay explicit in each export, and a host
//! must still check that a funded instance's module field names the selected
//! artifact. SHA-256 collision and preimage resistance are assumptions.
use crate::curry::{atom_hash, curried_puzzle_hash, pair_hash};
use crate::{Message32, PuzzleHash, sha256_parts};

/// Suffix that distinguishes a queue's terminal digest from its items.
pub const QUEUE_END: [u8; 3] = *b"end";

/// Digest of one queue item: `sha256(domain || recipient || next)`.
///
/// `next` is the digest of the rest of the queue, so a head digest commits to
/// the complete ordered queue. Use a domain unique to one policy and payout.
pub fn queue_item(domain: &[u8], recipient: PuzzleHash, next: Message32) -> Message32 {
    Message32(sha256_parts(domain, &recipient.0, &next.0))
}

/// Terminal digest of a queue: `sha256(domain || "end")`.
///
/// Its preimage is shorter than any item's, so the two never share an input.
pub fn queue_end(domain: &[u8]) -> Message32 {
    Message32(sha256_parts(domain, &QUEUE_END, &[]))
}

/// Tree hash of the proper two-element list `(first second)` from element hashes.
pub fn list_hash_2(first: &[u8; 32], second: &[u8; 32]) -> [u8; 32] {
    pair_hash(first, &pair_hash(second, &atom_hash(&[])))
}

/// Tree hash of the proper three-element list `(first second third)`.
pub fn list_hash_3(first: &[u8; 32], second: &[u8; 32], third: &[u8; 32]) -> [u8; 32] {
    pair_hash(first, &list_hash_2(second, third))
}

/// Puzzle hash of `module` curried with one terms value whose tree hash is given.
///
/// A self-referential continuation passes its own curried module hash, so the
/// successor runs the same code with new terms.
pub fn self_curried(module: PuzzleHash, terms_hash: &[u8; 32]) -> PuzzleHash {
    PuzzleHash(curried_puzzle_hash(&module.0, terms_hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_hashes_nest_proper_pairs() {
        let (a, b, c) = ([1; 32], [2; 32], [3; 32]);
        let nil = atom_hash(&[]);
        assert_eq!(list_hash_2(&a, &b), pair_hash(&a, &pair_hash(&b, &nil)));
        assert_eq!(
            list_hash_3(&a, &b, &c),
            pair_hash(&a, &pair_hash(&b, &pair_hash(&c, &nil)))
        );
    }

    #[test]
    fn queue_end_differs_from_items_and_domains() {
        let end = queue_end(b"d1");
        assert_ne!(end.0, queue_end(b"d2").0);
        assert_ne!(end.0, queue_item(b"d1", PuzzleHash([0; 32]), end).0);
        assert_eq!(end.0, sha256_parts(b"d1end", &[], &[]));
    }

    #[test]
    fn self_curried_matches_curry_arithmetic() {
        let module = PuzzleHash([7; 32]);
        let terms = [9; 32];
        assert_eq!(
            self_curried(module, &terms).0,
            curried_puzzle_hash(&module.0, &terms)
        );
    }
}
