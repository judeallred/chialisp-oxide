//! Shared types for the original finite counter actions.
//! WIP prototype; not ready for real funds.

use chialisp_oxide::{Cons, Message32, Tree};

/// Canonical action truth is an improper pair (opaque ephemeral . count).
/// The ephemeral tree is preserved byte for byte, including improper tails.
pub type StateTruth<'a> = Cons<Tree<'a>, u64>;

pub type IncrementOutput<'a> = Cons<StateTruth<'a>, ()>;
pub type ResetOutput<'a> = Cons<StateTruth<'a>, ()>;

/// AGG_SIG_ME additionally binds this fixed reset capability to the coin and
/// network. It does not authorize an exact batch or action order.
pub const RESET_INTENT: Message32 = Message32(*b"chialisp-oxide/counter/reset:v1!");
