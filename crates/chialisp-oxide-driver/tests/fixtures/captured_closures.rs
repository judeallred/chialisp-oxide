#![allow(dead_code)]
use chialisp_oxide::{Tree, puzzle, raw_clvm as raw};

#[puzzle]
pub fn snapshot(mut base: u64) -> (u64, u64, u64) {
    let f = move |n: u64| base + n;
    base += 100;
    (f(1), f(2), base)
}
#[puzzle]
pub fn copies(base: u64) -> (u64, u64, u64) {
    let f = move |n: u64| base + n;
    let copied = f;
    let f = move |n: u64| base * n;
    (copied(2), copied(3), f(4))
}
#[puzzle]
pub fn capture_order(a: u64, b: u8) -> u64 {
    let f = move |n: u64| (b as u64) + a + n;
    f(3)
}
#[puzzle]
pub fn fixed(bytes: [u8; 4]) -> u8 {
    let f = move |n: u8| bytes[1] + n;
    f(2)
}
#[puzzle]
pub fn whole_tuple(v: (u64, u8)) -> (u64, u8) {
    let f = move || v;
    f()
}
#[puzzle]
pub fn borrowed<'a>(tree: &'a Tree<'a>) -> Tree<'a> {
    let f = move || *tree;
    f()
}
#[puzzle]
pub fn unused_capture(flag: bool) -> u64 {
    let value = capture_value(flag);
    let _f = move || value + 1;
    99
}
fn capture_value(flag: bool) -> u64 { assert!(flag); 7 }
#[puzzle]
pub fn lazy_body(value: u64) -> u64 {
    let _f = move || { assert!(value == 0); value };
    99
}
#[puzzle]
pub fn call_order<'a>(value: u64, tree: Tree<'a>) -> u64 {
    let f = move |a: Tree<'a>, b: Tree<'a>| { assert!(value == 0); let _ = (a, b); 1 };
    f(fail_first(), raw::first(&tree))
}
fn fail_first<'a>() -> Tree<'a> { assert!(false); Tree::Atom(&[]) }
fn with_capture(base:u64)->u64 {let step=move |n:u64|base+n;step(1)}
#[puzzle]
pub fn fresh_frames(first:u64,second:u64)->(u64,u64){(with_capture(first),with_capture(second))}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn snapshots_and_fixed_values() {
        assert_eq!(snapshot(7), (8, 9, 107));
        assert_eq!(fresh_frames(7,100),(8,101));
        assert_eq!(fresh_frames(100,7),(101,8));
        assert_eq!(copies(7), (9, 10, 28));
        assert_eq!(capture_order(7, 2), 12);
        assert_eq!(fixed([1, 3, 5, 7]), 5);
        assert_eq!(whole_tuple((7, 3)), (7, 3));
    }
    #[test] fn borrowed_tree_bytes_and_improper_tail() {
        let atom = Tree::Atom(&[0, 0x80, 0xff]);
        let tail = Tree::Atom(&[7]);
        let pair = Tree::Pair(&atom, &tail);
        assert_eq!(borrowed(&atom), atom);
        assert_eq!(borrowed(&pair), pair);
    }
    #[test] fn strict_creation_and_lazy_body() {
        assert_eq!(unused_capture(true), 99);
        assert!(std::panic::catch_unwind(|| unused_capture(false)).is_err());
        assert_eq!(lazy_body(7), 99);
        assert!(std::panic::catch_unwind(|| call_order(7, Tree::Atom(&[]))).is_err());
    }
}
