//! Native aliases of the actual paired corpus Cargo exports; no copied policy.
//! WIP prototype; not ready to secure real funds.
pub use chialisp_oxide_puzzles::counter_increment::puzzle as increment;
pub use chialisp_oxide_puzzles::counter_reset::puzzle as reset;
pub use chialisp_oxide_puzzles::counter_types::{
    IncrementOutput, RESET_INTENT, ResetOutput, StateTruth,
};
