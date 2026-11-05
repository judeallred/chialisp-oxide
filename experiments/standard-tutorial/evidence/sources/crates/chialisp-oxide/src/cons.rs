//! Explicit wire pairs; tuples retain their existing proper-list encoding.

/// Own a head value and retain the tail's ordinary Rust ownership.
///
/// The representation marker declares a wire shape, not callable behavior.
#[doc = "__chialisp_oxide_wire_pair_v1"]
pub struct Cons<Head, Tail> {
    pub first: Head,
    pub rest: Tail,
}
