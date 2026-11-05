//! Explicit raw CLVM operators, constructed as ordinary Rust tree data.
//!
//! These helpers call the reviewed [`crate::apply`] bridge. The compiler lowers
//! their real Rust bodies; their names grant no intrinsic behavior. Arguments
//! and results have CLVM tree/atom semantics, not Rust slice or integer semantics.
//! For example, division rounds toward negative infinity and accepts integer
//! atoms outside Rust's fixed widths. Byte operations preserve exact atom bytes.
//!
//! Native calls require the facade's `vm` feature and execute through its pinned
//! VM oracle. Without it they reach `apply`'s panic body. That evaluator leaks
//! returned tree storage for host tests; it is not an allocation-free native
//! byte API. Applied failures propagate, resource failures remain inconclusive,
//! and no proof model admits these calls. Dialect/activation rules still apply.
use crate::{Tree, apply};

// Fixed active crypto operators share this explicit CLVM interface.
pub use crate::raw_clvm_crypto::*;
pub use crate::raw_clvm_variadic::*;

/// Return the atom byte length as a CLVM integer atom; a pair rejects.
pub fn byte_length<'a>(value: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[1]);
    let args0 = Tree::Pair(&path0, &nil);
    let opcode = Tree::Atom(&[13]);
    let program = Tree::Pair(&opcode, &args0);
    apply(&program, value)
}

/// Return the end-exclusive atom substring under CLVM's index rules.
/// `start` and `end` are raw integer atoms; this is not Rust slice indexing.
pub fn byte_substr<'a>(
    value: &'a Tree<'a>,
    start: &'a Tree<'a>,
    end: &'a Tree<'a>,
) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[5]);
    let path2 = Tree::Atom(&[11]);
    let args2 = Tree::Pair(&path2, &nil);
    let args1 = Tree::Pair(&path1, &args2);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[12]);
    let program = Tree::Pair(&opcode, &args0);
    let last = Tree::Pair(end, &nil);
    let rest = Tree::Pair(start, &last);
    let environment = Tree::Pair(value, &rest);
    apply(&program, &environment)
}

/// Concatenate two atom payloads exactly; either pair argument rejects.
pub fn byte_concat<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[14]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Divide raw signed integer atoms, rounding toward negative infinity.
pub fn div<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[19]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Return CLVM's improper `(quotient . remainder)` pair, using floor division.
pub fn divmod<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[20]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Return CLVM's signed modulo result; a zero divisor rejects.
pub fn modulo<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[61]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Return one for nil and nil for every other tree, including pairs.
pub fn not<'a>(value: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[1]);
    let args0 = Tree::Pair(&path0, &nil);
    let opcode = Tree::Atom(&[32]);
    let program = Tree::Pair(&opcode, &args0);
    apply(&program, value)
}

/// Return CLVM truth for two already evaluated tree operands.
/// This function is strict in both operands; it is not Rust `||` or `&&`.
pub fn any2<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[33]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Return CLVM conjunction for two already evaluated tree operands.
/// This function is strict in both operands; it is not Rust `||` or `&&`.
pub fn all2<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[34]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Add exactly two raw signed integer atoms.
pub fn add<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[16]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Subtract the second raw signed integer atom from the first.
pub fn sub<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[17]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Multiply exactly two raw signed integer atoms.
pub fn mul<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[18]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Bitwise AND of two signed integer atoms, using infinite sign extension.
pub fn bitand<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[24]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Bitwise OR of two signed integer atoms, using infinite sign extension.
pub fn bitor<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[25]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Bitwise XOR of two signed integer atoms, using infinite sign extension.
pub fn bitxor<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[26]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Bitwise complement of a raw signed integer atom.
pub fn lognot<'a>(value: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[1]);
    let args0 = Tree::Pair(&path0, &nil);
    let opcode = Tree::Atom(&[27]);
    let program = Tree::Pair(&opcode, &args0);
    apply(&program, value)
}

/// Shift a signed integer atom; a negative shift count shifts right.
/// The VM requires a signed i32 shift count in -65535..=65535.
pub fn arithmetic_shift<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[22]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Shift unsigned atom bytes; a negative shift count shifts right.
/// The VM requires a signed i32 shift count in -65535..=65535.
pub fn logical_shift<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[23]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Compare exact atom bytes for equality; either pair argument rejects.
pub fn atom_equal<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[9]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Compare exact atom bytes lexicographically; either pair rejects.
pub fn atom_greater<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[10]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Compare the signed numeric values of two raw integer atoms.
pub fn integer_greater<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[21]);
    let program = Tree::Pair(&opcode, &args0);
    let environment = Tree::Pair(left, right);
    apply(&program, &environment)
}

/// Hash parent ID, puzzle hash and canonical nonnegative u64 amount atom.
/// IDs must be exactly 32 bytes; amount padding, negatives and overflow reject.
pub fn coin_id<'a>(
    parent: &'a Tree<'a>,
    puzzle_hash: &'a Tree<'a>,
    amount: &'a Tree<'a>,
) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[5]);
    let path2 = Tree::Atom(&[11]);
    let args2 = Tree::Pair(&path2, &nil);
    let args1 = Tree::Pair(&path1, &args2);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[48]);
    let program = Tree::Pair(&opcode, &args0);
    let last = Tree::Pair(amount, &nil);
    let rest = Tree::Pair(puzzle_hash, &last);
    let environment = Tree::Pair(parent, &rest);
    apply(&program, &environment)
}

/// Select the second value unless the first is nil, then select the third.
/// All three Rust operands are evaluated before this call. Use Rust `if` for lazy branches.
pub fn strict_if3<'a>(
    condition: &'a Tree<'a>,
    then_value: &'a Tree<'a>,
    else_value: &'a Tree<'a>,
) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[5]);
    let path2 = Tree::Atom(&[11]);
    let args2 = Tree::Pair(&path2, &nil);
    let args1 = Tree::Pair(&path1, &args2);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[3]);
    let program = Tree::Pair(&opcode, &args0);
    let last = Tree::Pair(else_value, &nil);
    let rest = Tree::Pair(then_value, &last);
    let environment = Tree::Pair(condition, &rest);
    apply(&program, &environment)
}
