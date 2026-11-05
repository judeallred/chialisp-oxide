//! Raw numeric translation of the pinned modern Chialisp let example.
use chialisp_oxide::{Tree, raw_clvm as raw};

const ONE: [u8; 1] = [1];

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn puzzle<'a>(environment: &Tree<'a>) -> Tree<'static> {
    let value = raw::first(environment);
    let x = raw::add(&value, &Tree::Atom(&ONE));
    let y = raw::sub(&value, &Tree::Atom(&ONE));
    raw::mul(&x, &y)
}
