//! Independent host wire spelling for native test results, not compiler lowering.
use chialisp_oxide::{Amount, Cons, Message32, PublicKey, PuzzleHash, Tree};
use chialisp_oxide_puzzles::registry_recipe::EntryCommitment as OriginalEntry;
use chialisp_oxide_registry_controls::registry_recipe::EntryCommitment as ControlEntry;

pub trait Wire {
    fn wire(&self) -> Vec<u8>;
}
fn atom(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    match bytes.len() {
        1 if bytes[0] < 128 => (),
        n if n < 64 => output.push(128 + n as u8),
        _ => panic!("native control fixtures only use fixed short atoms"),
    }
    output.extend_from_slice(bytes);
    output
}
fn pair(first: Vec<u8>, rest: Vec<u8>) -> Vec<u8> {
    let mut output = vec![255];
    output.extend(first);
    output.extend(rest);
    output
}
fn list(items: &[Vec<u8>]) -> Vec<u8> {
    items
        .iter()
        .rev()
        .fold(vec![128], |rest, item| pair(item.clone(), rest))
}
impl Wire for () {
    fn wire(&self) -> Vec<u8> {
        vec![128]
    }
}
impl Wire for bool {
    fn wire(&self) -> Vec<u8> {
        if *self { vec![1] } else { vec![128] }
    }
}
impl Wire for u64 {
    fn wire(&self) -> Vec<u8> {
        if *self == 0 {
            return vec![128];
        }
        let bytes = self.to_be_bytes();
        let first = bytes.iter().position(|byte| *byte != 0).unwrap();
        let mut encoded = Vec::new();
        if bytes[first] & 128 != 0 {
            encoded.push(0);
        }
        encoded.extend_from_slice(&bytes[first..]);
        atom(&encoded)
    }
}
impl<const N: usize> Wire for [u8; N] {
    fn wire(&self) -> Vec<u8> {
        atom(self)
    }
}
impl Wire for PuzzleHash {
    fn wire(&self) -> Vec<u8> {
        self.0.wire()
    }
}
impl Wire for PublicKey {
    fn wire(&self) -> Vec<u8> {
        self.0.wire()
    }
}
impl Wire for Message32 {
    fn wire(&self) -> Vec<u8> {
        self.0.wire()
    }
}
impl Wire for Amount {
    fn wire(&self) -> Vec<u8> {
        self.mojos().wire()
    }
}
impl Wire for OriginalEntry {
    fn wire(&self) -> Vec<u8> {
        self.0.wire()
    }
}
impl Wire for ControlEntry {
    fn wire(&self) -> Vec<u8> {
        self.0.wire()
    }
}
impl Wire for Tree<'_> {
    fn wire(&self) -> Vec<u8> {
        match self {
            Tree::Atom(bytes) => atom(bytes),
            Tree::Pair(first, rest) => pair(first.wire(), rest.wire()),
        }
    }
}
impl<A: Wire, B: Wire> Wire for Cons<A, B> {
    fn wire(&self) -> Vec<u8> {
        pair(self.first.wire(), self.rest.wire())
    }
}
impl<A: Wire> Wire for (A,) {
    fn wire(&self) -> Vec<u8> {
        list(&[self.0.wire()])
    }
}
impl<A: Wire, B: Wire> Wire for (A, B) {
    fn wire(&self) -> Vec<u8> {
        list(&[self.0.wire(), self.1.wire()])
    }
}
impl<A: Wire, B: Wire, C: Wire> Wire for (A, B, C) {
    fn wire(&self) -> Vec<u8> {
        list(&[self.0.wire(), self.1.wire(), self.2.wire()])
    }
}
impl<A: Wire, B: Wire, C: Wire, D: Wire> Wire for (A, B, C, D) {
    fn wire(&self) -> Vec<u8> {
        list(&[self.0.wire(), self.1.wire(), self.2.wire(), self.3.wire()])
    }
}
