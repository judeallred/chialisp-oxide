//! Testing variant: omit exactly the returned CREATE condition.
//! WIP original teaching policy; not ready for use. Original beta-only state-bound replacement action.
use crate::registry_recipe::{
    BETA_NAME, RegistryTruth, SET_BETA_DOMAIN, entry_commitment, require_unused_ephemeral,
    returned_truth, slot_puzzle_hash, update_intent,
};
use chialisp_oxide::{
    AggSigMe, Cons, PublicKey, PuzzleHash, SendPuzzleMessage, agg_sig_me, send_puzzle_message,
};

pub type ControlOutput<'a> = Cons<RegistryTruth<'a>, (SendPuzzleMessage, AggSigMe)>;

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
    let old_entry = entry_commitment(&BETA_NAME, &old_value);
    assert!(truth.rest.rest.rest.0 == old_entry.0);
    let new_entry = entry_commitment(&BETA_NAME, &new_value);
    let next = Cons {
        first: true,
        rest: Cons {
            first: truth.rest.rest.first,
            rest: new_entry,
        },
    };
    Cons {
        first: returned_truth(next),
        rest: (
            send_puzzle_message(slot_puzzle_hash(slot_first, old_entry)),
            agg_sig_me(
                owner,
                update_intent(&SET_BETA_DOMAIN, &old_value, &new_value),
            ),
        ),
    }
}
