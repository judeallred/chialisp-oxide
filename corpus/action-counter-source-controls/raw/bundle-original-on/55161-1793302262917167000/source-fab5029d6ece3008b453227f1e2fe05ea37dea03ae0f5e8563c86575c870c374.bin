//! WIP original teaching policy; not ready for use. Original fixed two-name initializer; no slot port claim.
use crate::registry_recipe::{
    ALPHA_NAME, BETA_NAME, INITIAL_VALUE, InitializeOutput, RegistryTruth, SLOT_AMOUNT,
    entry_commitment, require_unused_ephemeral, returned_truth, slot_puzzle_hash,
};
use chialisp_oxide::{Cons, PuzzleHash, create_coin_with_hint};

/// Curry SLOT first-curry hash before membership commitment. Initializing is
/// deliberately permissionless and creates only the two fixed zero entries.
/// Sets have separately owner-curried actions and actual BLS requirements.
#[chialisp_oxide::puzzle]
pub fn puzzle<'a>(slot_first: PuzzleHash, truth: RegistryTruth<'a>) -> InitializeOutput<'a> {
    require_unused_ephemeral(truth.first);
    assert!(!truth.rest.first);
    let alpha = entry_commitment(&ALPHA_NAME, &INITIAL_VALUE);
    let beta = entry_commitment(&BETA_NAME, &INITIAL_VALUE);
    let next = Cons {
        first: true,
        rest: Cons {
            first: alpha,
            rest: beta,
        },
    };
    Cons {
        first: returned_truth(next),
        rest: (
            create_coin_with_hint(slot_puzzle_hash(slot_first, alpha), SLOT_AMOUNT, slot_first),
            create_coin_with_hint(slot_puzzle_hash(slot_first, beta), SLOT_AMOUNT, slot_first),
        ),
    }
}
