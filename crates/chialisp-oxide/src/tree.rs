//! Immutable CLVM data, with ordinary Rust lifetimes and pattern matching.
//!
//! A tree carries its original atom bytes. Numeric padding, improper pairs and
//! arbitrary tails are data; this type does not interpret or normalize them.
//! The compiler supports this restricted borrowed shape, not general enums,
//! references, allocation, or recursive Rust calls.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tree<'a> {
    Atom(&'a [u8]),
    Pair(&'a Tree<'a>, &'a Tree<'a>),
}

/// CLVM's quote operator as data, for constructing a quoted program.
pub const QUOTE: Tree<'static> = Tree::Atom(&[1]);

/// Construct a program returning `value` when applied to any environment.
///
/// This is an ordinary Rust function. The compiler checks and lowers its MIR;
/// its name does not grant it intrinsic semantics.
pub fn quote<'a>(value: &'a Tree<'a>) -> Tree<'a> {
    Tree::Pair(&QUOTE, value)
}
