//! Original finite counter action, not an action-layer or singleton port.
//! WIP prototype; not ready for real funds.

use crate::counter_types::{IncrementOutput, StateTruth};
use chialisp_oxide::Cons;

/// Action solution is nil, giving the exact one-argument environment (truth).
/// Ordinary checked Rust addition rejects incrementing u64::MAX.
#[chialisp_oxide::puzzle]
pub fn puzzle<'a>(truth: StateTruth<'a>) -> IncrementOutput<'a> {
    let next = truth.rest + 1;
    Cons {
        first: Cons {
            first: truth.first,
            rest: next,
        },
        rest: (),
    }
}
