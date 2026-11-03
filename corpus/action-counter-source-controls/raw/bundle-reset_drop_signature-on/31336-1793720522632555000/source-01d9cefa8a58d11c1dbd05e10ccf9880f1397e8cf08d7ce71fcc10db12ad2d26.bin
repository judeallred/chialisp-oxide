//! Original finite counter action, not an action-layer or singleton port.
//! WIP prototype; not ready for real funds.

use crate::counter_types::{RESET_INTENT, ResetOutput, StateTruth};
use chialisp_oxide::{Cons, PublicKey, agg_sig_me};

/// Curry owner as one 48-byte atom before committing the reset Merkle leaf.
/// The exact remaining environment is (truth). The typed ABI validates every
/// input field, including the discarded old canonical u64 count.
#[chialisp_oxide::puzzle]
pub fn puzzle<'a>(owner: PublicKey, truth: StateTruth<'a>) -> ResetOutput<'a> {
    Cons {
        first: Cons {
            first: truth.first,
            rest: 0,
        },
        rest: {
            let _discarded_signature = agg_sig_me(owner, RESET_INTENT);
        },
    }
}
