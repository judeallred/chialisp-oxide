//! Owned pair nodes with borrowed, immutable atom bytes.

use alloc::boxed::Box;

/// An ordinary Rust owned tree, preserving every atom byte and improper tail.
///
/// Native execution allocates ordinary `Box` pair nodes. CLVM compilation
/// lowers a restricted set of those operations to logical pairs; it does not
/// expose pointer or allocator APIs or make allocation and recursion free.
pub enum OwnedTree<'a> {
    Atom(&'a [u8]),
    Pair(Box<OwnedTree<'a>>, Box<OwnedTree<'a>>),
}
