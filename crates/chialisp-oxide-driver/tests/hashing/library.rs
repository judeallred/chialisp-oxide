use chialisp_oxide::{Tree, sha256, sha256_parts as multipart};
#[chialisp_oxide::puzzle]
pub fn one(bytes: &[u8]) -> [u8; 32] {
    sha256(bytes)
}
#[chialisp_oxide::puzzle]
pub fn parts(a: &[u8], b: &[u8], c: &[u8]) -> [u8; 32] {
    multipart(a, b, c)
}
#[chialisp_oxide::puzzle]
pub fn nested(bytes: &[u8]) -> [u8; 32] {
    sha256(&sha256(bytes))
}
#[chialisp_oxide::puzzle]
pub fn prefixed(bytes: &[u8]) -> [u8; 32] {
    multipart(&[1], bytes, &[])
}
#[chialisp_oxide::puzzle]
pub fn pair_hash(a: &[u8], b: &[u8]) -> [u8; 32] {
    let left = sha256(a);
    let right = sha256(b);
    multipart(&[2], &left, &right)
}
#[chialisp_oxide::puzzle]
pub fn fixed(bytes: [u8; 32]) -> [u8; 32] {
    sha256(&bytes)
}
#[chialisp_oxide::puzzle]
pub fn snapshot(first: [u8; 32], second: [u8; 32]) -> ([u8; 32], [u8; 32]) {
    let mut bytes = first;
    let borrowed: &[u8] = &bytes;
    let old = sha256(borrowed);
    bytes = second;
    (old, sha256(&bytes))
}
fn atom<'a>(tree: &'a Tree<'a>) -> &'a [u8] {
    match tree {
        Tree::Atom(bytes) => bytes,
        Tree::Pair(_, _) => panic!(),
    }
}
#[chialisp_oxide::puzzle(environment = "raw")]
pub fn raw(tree: &Tree<'_>) -> [u8; 32] {
    sha256(atom(tree))
}
#[chialisp_oxide::puzzle(environment = "raw")]
pub fn lazy(tree: &Tree<'_>) -> [u8; 32] {
    match tree {
        Tree::Atom(bytes) => sha256(bytes),
        Tree::Pair(_, _) => [7; 32],
    }
}
fn ignore(_: [u8; 32]) -> u8 {
    7
}
#[chialisp_oxide::puzzle]
pub fn ignored_argument(bytes: &[u8]) -> u8 {
    ignore(sha256(bytes))
}
#[chialisp_oxide::puzzle]
pub fn unused(bytes: &[u8]) -> u8 {
    let _ = sha256(bytes);
    7
}
#[chialisp_oxide::puzzle]
pub fn no_hash(_: &[u8]) -> u8 {
    7
}
#[chialisp_oxide::puzzle]
pub fn hash_before_reject(bytes: &[u8]) -> u8 {
    let _ = sha256(bytes);
    panic!()
}
#[allow(unconditional_recursion)]
fn spin<'a>(value: &'a Tree<'a>) -> &'a [u8] {
    spin(value)
}
fn fail<'a>(_: &'a Tree<'a>) -> &'a [u8] {
    panic!()
}
#[chialisp_oxide::puzzle(environment = "raw")]
pub fn reject_then_spin(tree: &Tree<'_>) -> [u8; 32] {
    multipart(fail(tree), spin(tree), &[])
}
#[chialisp_oxide::puzzle(environment = "raw")]
pub fn spin_then_reject(tree: &Tree<'_>) -> [u8; 32] {
    multipart(spin(tree), fail(tree), &[])
}
mod misleading {
    pub fn sha256(_: &[u8]) -> [u8; 32] {
        [9; 32]
    }
    pub fn sha256_parts(_: &[u8], _: &[u8], _: &[u8]) -> [u8; 32] {
        [8; 32]
    }
}
#[chialisp_oxide::puzzle]
pub fn fake_one(bytes: &[u8]) -> [u8; 32] {
    misleading::sha256(bytes)
}
#[chialisp_oxide::puzzle]
pub fn fake_parts(bytes: &[u8]) -> [u8; 32] {
    misleading::sha256_parts(bytes, &[], &[])
}

#[chialisp_oxide::puzzle]
pub fn as_tree(bytes: &[u8; 2]) -> Tree<'_> {
    Tree::Atom(bytes)
}
