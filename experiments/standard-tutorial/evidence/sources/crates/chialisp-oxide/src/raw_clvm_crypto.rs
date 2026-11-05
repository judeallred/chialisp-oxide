//! Fixed active CLVM cryptographic operators through ordinary tree construction.
//!
//! All functions use the existing reviewed apply bridge. Native calls therefore
//! require the `vm` feature and use that VM oracle; these are not independent
//! Rust cryptographic implementations. Signature and pairing operators return
//! nil on success and reject on failure. They do not produce spend conditions
//! or establish a spend bundle's authorization. No dialect flags are changed.
use crate::{Tree, apply};

fn invoke1<'a>(opcode: &'a [u8], a: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[1]);
    let args0 = Tree::Pair(&path0, &nil);
    let op = Tree::Atom(opcode);
    let program = Tree::Pair(&op, &args0);
    apply(&program, a)
}

fn invoke2<'a>(opcode: &'a [u8], a: &'a Tree<'a>, b: &'a Tree<'a>) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[3]);
    let args1 = Tree::Pair(&path1, &nil);
    let args0 = Tree::Pair(&path0, &args1);
    let op = Tree::Atom(opcode);
    let program = Tree::Pair(&op, &args0);
    let environment = Tree::Pair(a, b);
    apply(&program, &environment)
}

fn invoke3<'a>(
    opcode: &'a [u8],
    a: &'a Tree<'a>,
    b: &'a Tree<'a>,
    c: &'a Tree<'a>,
) -> Tree<'static> {
    let nil = Tree::Atom(&[]);
    let path0 = Tree::Atom(&[2]);
    let path1 = Tree::Atom(&[5]);
    let path2 = Tree::Atom(&[11]);
    let args2 = Tree::Pair(&path2, &nil);
    let args1 = Tree::Pair(&path1, &args2);
    let args0 = Tree::Pair(&path0, &args1);
    let op = Tree::Atom(opcode);
    let program = Tree::Pair(&op, &args0);
    let values2 = Tree::Pair(c, &nil);
    let values1 = Tree::Pair(b, &values2);
    let values0 = Tree::Pair(a, &values1);
    apply(&program, &values0)
}

fn invoke4<'a>(
    opcode: &'a [u8],
    a: &'a Tree<'a>,
    b: &'a Tree<'a>,
    c: &'a Tree<'a>,
    d: &'a Tree<'a>,
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
    let op = Tree::Atom(opcode);
    let program = Tree::Pair(&op, &args0);
    let values3 = Tree::Pair(d, &nil);
    let values2 = Tree::Pair(c, &values3);
    let values1 = Tree::Pair(b, &values2);
    let values0 = Tree::Pair(a, &values1);
    apply(&program, &values0)
}

/// Return the G1 public point for a raw integer scalar, reduced modulo group order.
pub fn pubkey_for_exp<'a>(scalar: &'a Tree<'a>) -> Tree<'static> {
    invoke1(&[30], scalar)
}

/// Add exactly two validated compressed G1 points.
pub fn point_add<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    invoke2(&[29], left, right)
}

/// Subtract the second compressed G1 point from the first.
pub fn g1_subtract<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    invoke2(&[49], left, right)
}

/// Multiply a validated compressed G1 point by a raw integer scalar.
pub fn g1_multiply<'a>(point: &'a Tree<'a>, scalar: &'a Tree<'a>) -> Tree<'static> {
    invoke2(&[50], point, scalar)
}

/// Negate one compressed G1 point under the current strict dialect.
pub fn g1_negate<'a>(point: &'a Tree<'a>) -> Tree<'static> {
    invoke1(&[51], point)
}

/// Add exactly two validated compressed G2 points.
pub fn g2_add<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    invoke2(&[52], left, right)
}

/// Subtract the second compressed G2 point from the first.
pub fn g2_subtract<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>) -> Tree<'static> {
    invoke2(&[53], left, right)
}

/// Multiply a validated compressed G2 point by a raw integer scalar.
pub fn g2_multiply<'a>(point: &'a Tree<'a>, scalar: &'a Tree<'a>) -> Tree<'static> {
    invoke2(&[54], point, scalar)
}

/// Negate one compressed G2 point under the current strict dialect.
pub fn g2_negate<'a>(point: &'a Tree<'a>) -> Tree<'static> {
    invoke1(&[55], point)
}

/// Map atom bytes to G1 using the VM default AUG domain separation tag.
pub fn g1_map<'a>(message: &'a Tree<'a>) -> Tree<'static> {
    invoke1(&[56], message)
}

/// Map atom bytes to G1 using the explicit atom domain separation tag.
pub fn g1_map_with_dst<'a>(message: &'a Tree<'a>, dst: &'a Tree<'a>) -> Tree<'static> {
    invoke2(&[56], message, dst)
}

/// Map atom bytes to G2 using the VM default AUG domain separation tag.
pub fn g2_map<'a>(message: &'a Tree<'a>) -> Tree<'static> {
    invoke1(&[57], message)
}

/// Map atom bytes to G2 using the explicit atom domain separation tag.
pub fn g2_map_with_dst<'a>(message: &'a Tree<'a>, dst: &'a Tree<'a>) -> Tree<'static> {
    invoke2(&[57], message, dst)
}

/// Check one G1/G2 pairing pair for identity; return nil or reject.
pub fn bls_pairing_identity<'a>(g1: &'a Tree<'a>, g2: &'a Tree<'a>) -> Tree<'static> {
    invoke2(&[58], g1, g2)
}

/// Check the product of exactly two G1/G2 pairings for identity.
pub fn bls_pairing_identity_two<'a>(
    first_g1: &'a Tree<'a>,
    first_g2: &'a Tree<'a>,
    second_g1: &'a Tree<'a>,
    second_g2: &'a Tree<'a>,
) -> Tree<'static> {
    invoke4(&[58], first_g1, first_g2, second_g1, second_g2)
}

/// Check one nonempty public-key/message pair against a compressed G2 signature.
pub fn bls_verify<'a>(
    signature: &'a Tree<'a>,
    public_key: &'a Tree<'a>,
    message: &'a Tree<'a>,
) -> Tree<'static> {
    invoke3(&[59], signature, public_key, message)
}

/// Verify SEC1 public key, 32-byte prehash and compact 64-byte k1 signature.
/// Uses the active legacy four-byte opcode, not flag-gated opcode 64 or 65.
pub fn secp256k1_verify<'a>(
    public_key: &'a Tree<'a>,
    digest: &'a Tree<'a>,
    signature: &'a Tree<'a>,
) -> Tree<'static> {
    invoke3(&[19, 214, 31, 0], public_key, digest, signature)
}

/// Verify SEC1 public key, 32-byte prehash and compact 64-byte r1 signature.
/// Uses the active legacy four-byte opcode, not flag-gated opcode 64 or 65.
pub fn secp256r1_verify<'a>(
    public_key: &'a Tree<'a>,
    digest: &'a Tree<'a>,
    signature: &'a Tree<'a>,
) -> Tree<'static> {
    invoke3(&[28, 58, 143, 0], public_key, digest, signature)
}
