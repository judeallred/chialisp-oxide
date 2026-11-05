//! Internal compiler limits, distinct from malformed IR and codegen defects.
//!
//! An optional inlining candidate can concatenate continuation trees that were
//! separately affordable before inlining. Only these typed limits permit
//! compilation to retry the previous optimized program.
use std::{error::Error, fmt};

#[derive(Debug)]
pub(super) enum Limit {
    RuntimeWork,
    FrameBindings,
    PendingOperands,
    RuntimeDepth,
    ExpansionDepth,
    ExpansionSize,
}

impl fmt::Display for Limit {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(match self {
            Self::RuntimeWork => "runtime lowering work budget exceeded",
            Self::FrameBindings => "runtime frame binding budget exceeded",
            Self::PendingOperands => "runtime frame operand budget exceeded",
            Self::RuntimeDepth => "runtime expression depth budget exceeded",
            Self::ExpansionDepth => "CLVM expansion nesting limit",
            Self::ExpansionSize => "CLVM expansion exceeds compiler size budget",
        })
    }
}

impl Error for Limit {}
