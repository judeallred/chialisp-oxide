//! Original teaching policy: an authenticated ordered payout queue, one item per spend.
//! WIP prototype; not ready to secure real funds.

use chialisp_oxide::continuation::{list_hash_2, queue_end, queue_item, self_curried};
use chialisp_oxide::curry::atom_hash;
use chialisp_oxide::{
    Amount, AssertMyAmount, Cons, CreateCoin, Message32, PuzzleHash, assert_my_amount, create_coin,
};

/// Each queue item pays exactly this amount. No reserve, fee or abort branch.
pub const PAYOUT: Amount = Amount::from_mojos(1_000);
/// Domain for this policy's queue digests (a fixed payout is part of the policy).
pub const QUEUE_DOMAIN: [u8; 38] = *b"chialisp-oxide/ordered-payout/v1/1000:";

/// Curried as ONE proper list: (this module's hash, digest of the remaining queue).
/// A host must check, before funding, that the module field names the selected
/// artifact and that the coin holds exactly `PAYOUT` times the queue length.
pub type Terms = (PuzzleHash, Message32);

/// Nonterminal steps add the continuation coin; the final step has none.
pub type Conditions = Cons<AssertMyAmount, Cons<CreateCoin, Option<CreateCoin>>>;

/// Puzzle hash of an instance whose remaining queue has digest `head`.
pub fn instance_hash(module: PuzzleHash, head: Message32) -> PuzzleHash {
    self_curried(
        module,
        &list_hash_2(&atom_hash(&module.0), &atom_hash(&head.0)),
    )
}

/// Pay the next committed recipient and recreate the coin for the rest.
///
/// The spender supplies the opening (`recipient`, `next`) and the coin's own
/// amount. ASSERT_MY_AMOUNT makes consensus check that amount, so funds are not
/// a curried counter: with correct funding, `amount == PAYOUT * remaining` holds
/// at every step, the last step leaves exactly one payout, and no step can end
/// early, skip, reorder or inject an item, or choose the successor's puzzle.
#[chialisp_oxide::puzzle]
pub fn puzzle(
    terms: Terms,
    recipient: PuzzleHash,
    next: Message32,
    my_amount: Amount,
) -> Conditions {
    let (module, head) = terms;
    assert!(queue_item(&QUEUE_DOMAIN, recipient, next).0 == head.0);
    let continuation = if next.0 == queue_end(&QUEUE_DOMAIN).0 {
        assert!(my_amount.mojos() == PAYOUT.mojos());
        None
    } else {
        // At least one further payout must remain for a nonterminal successor.
        assert!(my_amount.mojos() >= PAYOUT.mojos() * 2);
        let remaining = Amount::from_mojos(my_amount.mojos() - PAYOUT.mojos());
        Some(create_coin(instance_hash(module, next), remaining))
    };
    Cons {
        first: assert_my_amount(my_amount),
        rest: Cons {
            first: create_coin(recipient, PAYOUT),
            rest: continuation,
        },
    }
}
