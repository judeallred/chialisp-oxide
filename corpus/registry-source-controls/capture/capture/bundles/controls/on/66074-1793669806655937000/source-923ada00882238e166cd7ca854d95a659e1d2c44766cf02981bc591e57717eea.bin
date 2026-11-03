//! Testing variant: omit exactly the returned SEND condition.
//! WIP original teaching policy; not ready for use. Original alpha-only state-bound replacement action.
use crate::registry_recipe::{
    ALPHA_NAME, RegistryTruth, SET_ALPHA_DOMAIN, SLOT_AMOUNT, entry_commitment,
    require_unused_ephemeral, returned_truth, slot_puzzle_hash, update_intent,
};
use chialisp_oxide::{
    AggSigMe, Cons, CreateCoinWithHint, PublicKey, PuzzleHash, agg_sig_me, create_coin_with_hint,
};

pub type ControlOutput<'a> = Cons<RegistryTruth<'a>, (CreateCoinWithHint, AggSigMe)>;

/// Curry owner48 and SLOT first-curry hash32 in that order before membership.
/// Remaining action solution is the proper list (old_value32 new_value32).
#[chialisp_oxide::puzzle]
pub fn puzzle<'a>(
    owner: PublicKey,
    slot_first: PuzzleHash,
    truth: RegistryTruth<'a>,
    old_value: [u8; 32],
    new_value: [u8; 32],
) -> ControlOutput<'a> {
    require_unused_ephemeral(truth.first);
    assert!(truth.rest.first);
    let old_entry = entry_commitment(&ALPHA_NAME, &old_value);
    assert!(truth.rest.rest.first.0 == old_entry.0);
    let new_entry = entry_commitment(&ALPHA_NAME, &new_value);
    let next = Cons {
        first: true,
        rest: Cons {
            first: new_entry,
            rest: truth.rest.rest.rest,
        },
    };
    Cons {
        first: returned_truth(next),
        rest: (
            create_coin_with_hint(
                slot_puzzle_hash(slot_first, new_entry),
                SLOT_AMOUNT,
                slot_first,
            ),
            agg_sig_me(
                owner,
                update_intent(&SET_ALPHA_DOMAIN, &old_value, &new_value),
            ),
        ),
    }
}
