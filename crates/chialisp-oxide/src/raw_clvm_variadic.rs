//! Proper-list CLVM operand APIs and explicit softfork guards.
//!
//! The operand-list helpers quote every value as data before applying a fixed
//! operator. Program-shaped list values cannot select executable code, and
//! improper lists reject even after a decisive boolean value. The separate
//! explicit `softfork` helper deliberately executes its program argument inside
//! a guard. All source operands remain strict. These are the VM oracle APIs
//! described by `raw_clvm`, not general Rust collection support.
use crate::{Tree, apply};

/// Construct the exact pair `(first . rest)`; this is not a proper-list constructor.
pub fn cons<'a>(first: &'a Tree<'a>, rest: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let first_path = Tree::Atom(&[2]);
    let rest_path = Tree::Atom(&[3]);
    let tail = Tree::Pair(&rest_path, &nil);
    let args = Tree::Pair(&first_path, &tail);
    let opcode = Tree::Atom(&[4]);
    let program = Tree::Pair(&opcode, &args);
    let environment = Tree::Pair(first, rest);
    apply(&program, &environment)
}

fn quote_values<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    match values {
        Tree::Atom([]) => Tree::Atom(&[]),
        Tree::Pair(first, rest) => {
            let quoted = cons(&Tree::Atom(&[1]), first);
            let remaining = quote_values(rest);
            cons(&quoted, &remaining)
        }
        _ => panic!(),
    }
}

// Native-only protection for this recursive test/oracle implementation. These
// host budgets do not become compiled puzzle guards or semantic rejection.
#[cfg(feature = "vm")]
fn preflight_values(values: &Tree<'_>) {
    extern crate std;
    use std::vec::Vec;
    const MAX_NODES: usize = 32_768;
    const MAX_BYTES: usize = 1_048_576;
    const MAX_DEPTH: usize = 256;
    const MAX_ITEMS: usize = 128;
    const MAX_COPIED_NODES: usize = 4_000_000;
    const MAX_COPIED_BYTES: usize = 16_777_216;
    fn exhausted(limit: usize) -> ! {
        std::panic::panic_any(crate::ResourceExhausted {
            limit: limit as u64,
        })
    }
    fn add(current: usize, additional: usize, limit: usize) -> usize {
        current
            .checked_add(additional)
            .filter(|n| *n <= limit)
            .unwrap_or_else(|| exhausted(limit))
    }
    fn projected(expanded: usize, count: usize, limit: usize) {
        // Each item is copied into a quote and successively larger suffixes.
        // Count all occurrences, including repeated references to shared data.
        // Four covers input/output copies of both cons calls; the per-item
        // quoted/program/frame overhead is deliberately conservative.
        let overhead = count
            .checked_mul(2)
            .and_then(|n| n.checked_add(64))
            .unwrap_or_else(|| exhausted(limit));
        let base = expanded
            .checked_add(overhead)
            .unwrap_or_else(|| exhausted(limit));
        let factor = count
            .checked_add(1)
            .and_then(|n| n.checked_mul(4))
            .unwrap_or_else(|| exhausted(limit));
        if base.checked_mul(factor).is_none_or(|n| n > limit) {
            exhausted(limit);
        }
    }
    // Charge both children before extending the pending stack. No traversal
    // follows pointers through a unique-node cache: a DAG expands as CLVM data.
    let mut nodes = 1usize;
    let mut bytes = 0usize;
    let mut pending = Vec::new();
    pending.push((values, 0usize));
    while let Some((value, depth)) = pending.pop() {
        match value {
            Tree::Atom(atom) => bytes = add(bytes, atom.len(), MAX_BYTES),
            Tree::Pair(first, rest) => {
                let next_depth = add(depth, 1, MAX_DEPTH);
                nodes = add(nodes, 2, MAX_NODES);
                // Nodes were charged before either push (or Vec growth).
                pending.push((rest, next_depth));
                pending.push((first, next_depth));
            }
        }
    }
    // Shape is intentionally not rejected here. First complete the resource
    // preflight, including a malformed tail and every sibling; quote_values
    // subsequently performs the original proper-list semantic check.
    let mut count = 0usize;
    let mut cursor = values;
    while let Tree::Pair(_, rest) = cursor {
        count = add(count, 1, MAX_ITEMS);
        cursor = rest;
    }
    projected(nodes, count, MAX_COPIED_NODES);
    projected(bytes, count, MAX_COPIED_BYTES);
}

fn invoke_list<'a>(opcode: &'a [u8], values: &'a Tree<'a>) -> Tree<'static> {
    #[cfg(feature = "vm")]
    preflight_values(values);
    let arguments = quote_values(values);
    let program = cons(&Tree::Atom(opcode), &arguments);
    apply(&program, &Tree::Atom(&[]))
}

/// Add all raw signed integer atoms; the empty sum is zero.
/// The input must be a proper list, including for empty or singleton operands.
pub fn add_list<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    invoke_list(&[16], values)
}

/// Subtract trailing raw integer atoms from the first; empty input returns zero.
/// The input must be a proper list, including for empty or singleton operands.
pub fn sub_list<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    invoke_list(&[17], values)
}

/// Multiply all raw integer atoms; the empty product is one.
/// The input must be a proper list, including for empty or singleton operands.
pub fn mul_list<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    invoke_list(&[18], values)
}

/// Bitwise AND all raw integer atoms; empty input returns negative one.
/// The input must be a proper list, including for empty or singleton operands.
pub fn bitand_list<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    invoke_list(&[24], values)
}

/// Bitwise OR all raw integer atoms; empty input returns zero.
/// The input must be a proper list, including for empty or singleton operands.
pub fn bitor_list<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    invoke_list(&[25], values)
}

/// Bitwise XOR all raw integer atoms; empty input returns zero.
/// The input must be a proper list, including for empty or singleton operands.
pub fn bitxor_list<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    invoke_list(&[26], values)
}

/// Concatenate every atom payload exactly; empty input returns nil.
/// The input must be a proper list, including for empty or singleton operands.
pub fn concat_list<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    invoke_list(&[14], values)
}

/// Hash the concatenated atom payloads without framing or domain separation.
/// The input must be a proper list, including for empty or singleton operands.
/// This explicit VM route does not add native SHA-kernel admission or a proof model.
pub fn sha256_list<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    invoke_list(&[11], values)
}

/// Return one if any value is non-nil, otherwise nil; traversal is strict.
/// The input must be a proper list, including for empty or singleton operands.
pub fn any_list<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    invoke_list(&[33], values)
}

/// Return one if every value is non-nil, including for an empty list.
/// The input must be a proper list, including for empty or singleton operands.
pub fn all_list<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    invoke_list(&[34], values)
}

/// Add all compressed G1 points; empty input returns compressed infinity.
/// The input must be a proper list, including for empty or singleton operands.
pub fn point_add_list<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    invoke_list(&[29], values)
}

/// Subtract trailing G1 points from the first; empty input returns infinity.
/// The input must be a proper list, including for empty or singleton operands.
pub fn g1_subtract_list<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    invoke_list(&[49], values)
}

/// Add all compressed G2 points; empty input returns compressed infinity.
/// The input must be a proper list, including for empty or singleton operands.
pub fn g2_add_list<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    invoke_list(&[52], values)
}

/// Subtract trailing G2 points from the first; empty input returns infinity.
/// The input must be a proper list, including for empty or singleton operands.
pub fn g2_subtract_list<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    invoke_list(&[53], values)
}

/// Check a flat proper list of G1/G2 pairs; return nil or reject.
/// The input must be a proper list, including for empty or singleton operands.
/// An empty pairing or signature-only infinity check may succeed vacuously; it is not authority.
pub fn pairing_identity_list<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    invoke_list(&[58], values)
}

/// Check a signature followed by a flat proper list of public-key/message pairs.
/// The input must be a proper list, including for empty or singleton operands.
/// An empty pairing or signature-only infinity check may succeed vacuously; it is not authority.
pub fn bls_verify_list<'a>(values: &'a Tree<'a>) -> Tree<'static> {
    invoke_list(&[59], values)
}

/// Execute the current dialect's explicit four-argument softfork guard.
///
/// A successful guard returns nil; its internal result cannot escape. The
/// caller supplies the declared cost, extension ID, executable program and its
/// environment deliberately. This API changes no activation flags and provides
/// no cross-network activation guarantee. Wrong cost reservations and resource
/// failures retain their typed VM distinction; they are not policy rejection.
pub fn softfork<'a>(
    cost: &'a Tree<'a>,
    extension: &'a Tree<'a>,
    program: &'a Tree<'a>,
    environment: &'a Tree<'a>,
) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[5]);
    let path2 = Tree::Atom(&[11]);
    let path3 = Tree::Atom(&[23]);
    let args3 = Tree::Pair(&path3, &nil);
    let args2 = Tree::Pair(&path2, &args3);
    let args1 = Tree::Pair(&path1, &args2);
    let args0 = Tree::Pair(&path0, &args1);
    let opcode = Tree::Atom(&[36]);
    let guard = Tree::Pair(&opcode, &args0);
    let values3 = Tree::Pair(environment, &nil);
    let values2 = Tree::Pair(program, &values3);
    let values1 = Tree::Pair(extension, &values2);
    let values0 = Tree::Pair(cost, &values1);
    apply(&guard, &values0)
}

fn unary<'a>(opcode: &'a [u8], value: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path = Tree::Atom(&[1]);
    let args = Tree::Pair(&path, &nil);
    let op = Tree::Atom(opcode);
    let program = Tree::Pair(&op, &args);
    apply(&program, value)
}

/// Return the first child of a pair; an atom rejects.
pub fn first<'a>(value: &'a Tree<'a>) -> Tree<'static> {
    unary(&[5], value)
}
/// Return the rest child of a pair, preserving improper tails; an atom rejects.
pub fn rest<'a>(value: &'a Tree<'a>) -> Tree<'static> {
    unary(&[6], value)
}
/// Return one for any pair and nil for an atom. This does not validate a proper list.
pub fn listp<'a>(value: &'a Tree<'a>) -> Tree<'static> {
    unary(&[7], value)
}
/// Raise with one already evaluated raw tree argument; this call never succeeds.
pub fn raise<'a>(value: &'a Tree<'a>) -> Tree<'static> {
    unary(&[8], value)
}
