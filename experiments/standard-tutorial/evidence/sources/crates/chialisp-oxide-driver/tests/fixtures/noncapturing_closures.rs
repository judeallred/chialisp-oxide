//! Actual Rust closure-to-fn coercions; no captured environment or Fn-trait API.
use chialisp_oxide::{OwnedTree, Tree, raw_clvm as raw};
type Step = fn(u64) -> u64;
fn double(n: u64) -> u64 {
    n * 2
}
fn invoke(step: Step, n: u64) -> u64 {
    step(n)
}

#[chialisp_oxide::puzzle]
pub fn shapes(choose: bool, n: u64, byte: u8) -> (u64, u64, u64, u64) {
    let zero: fn() -> u64 = || 7;
    let one: Step = if choose { |x| x + 1 } else { double };
    let two: fn(u64, u8) -> u64 = |x, y| x + y as u64;
    let tuple: fn((u64, u8)) -> u64 = |value| value.0 + value.1 as u64;
    (zero(), invoke(one, n), two(n, byte), tuple((n, byte)))
}
#[chialisp_oxide::puzzle]
pub fn copied(n: u64) -> (u64, u64) {
    let closure = |value: u64| value + 1;
    let saved = closure;
    let step: Step = saved;
    let copy = step;
    (invoke(step, n), invoke(copy, n + 1))
}
fn borrowed_invoke<'a>(
    step: for<'v> fn(&'v Tree<'v>) -> Tree<'v>,
    value: &'a Tree<'a>,
) -> Tree<'a> {
    step(value)
}
#[chialisp_oxide::puzzle]
pub fn borrowed<'a>(value: &'a Tree<'a>) -> Tree<'a> {
    let step: for<'v> fn(&'v Tree<'v>) -> Tree<'v> = |tree| *tree;
    borrowed_invoke(step, value)
}

#[chialisp_oxide::puzzle]
pub fn literal_borrowed<'a>(value:&'a Tree<'a>)->Tree<'a>{
    let step:for<'v> fn(&'v Tree<'v>)->Tree<'v>=|tree|match *tree{
        Tree::Atom([])=>*(&Tree::Atom(&[0,1])),
        _=>*tree,
    };
    borrowed_invoke(step,value)
}

type Transform = fn(Tree<'static>) -> Tree<'static>;
fn map(step: Transform, values: &Tree<'static>) -> OwnedTree<'static> {
    match *values {
        Tree::Atom([]) => OwnedTree::Atom(&[]),
        Tree::Pair(first, rest) => {
            let value = step(*first);
            match value {
                Tree::Atom(bytes) => {
                    OwnedTree::Pair(Box::new(OwnedTree::Atom(bytes)), Box::new(map(step, rest)))
                }
                Tree::Pair(_, _) => panic!(),
            }
        }
        Tree::Atom(_) => panic!(),
    }
}
#[chialisp_oxide::puzzle(environment = "raw")]
pub fn map_add(environment: &Tree<'_>) -> OwnedTree<'static> {
    let values = raw::first(environment);
    let step: Transform = |value| raw::add(&value, &Tree::Atom(&[1]));
    map(step, &values)
}
#[chialisp_oxide::puzzle]
pub fn strict_initialization(flag: bool) -> u64 {
    let step: fn(Tree<'static>) -> u64 = {
        assert!(flag);
        |_| 7
    };
    step(raw::first(&Tree::Atom(&[])))
}

#[allow(unconditional_recursion)]
fn forever(value:u64)->u64 { forever(value) }
#[chialisp_oxide::puzzle]
pub fn divergence(flag:bool,value:u64)->u64 {
    let step:Step=if flag {|n|forever(n)} else {|n|n+1};
    invoke(step,value)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fail<T>(result: std::thread::Result<T>) -> String {
        let error = result.err().expect("expected policy failure");
        assert!(
            !error.is::<chialisp_oxide::ResourceExhausted>(),
            "resource is inconclusive"
        );
        error.downcast_ref::<String>().cloned().unwrap_or_else(|| {
            error
                .downcast_ref::<&str>()
                .expect("policy diagnostic")
                .to_string()
        })
    }
    fn exact(value: &OwnedTree<'_>, atoms: &[&[u8]]) {
        match (value, atoms.split_first()) {
            (OwnedTree::Atom(atom), None) => assert!(atom.is_empty()),
            (OwnedTree::Pair(first, rest), Some((expected, tail))) => {
                let OwnedTree::Atom(actual) = first.as_ref() else {
                    panic!("atom output")
                };
                assert_eq!(actual, expected);
                exact(rest, tail);
            }
            _ => panic!("incorrect output shape"),
        }
    }
    #[test]
    fn scalar_arities_and_immutable_copies() {
        assert_eq!(shapes(true, 7, 3), (7, 8, 10, 10));
        assert_eq!(shapes(false, 7, 3), (7, 14, 10, 10));
        assert_eq!(shapes(true, 0, 255), (7, 1, 255, 255));
        assert_eq!(copied(7), (8, 9));
        assert_eq!(divergence(false,7),8);
        fail(std::panic::catch_unwind(|| copied(u64::MAX)));
    }
    #[test]
    fn borrowed_bytes_and_arbitrary_numeric_map() {
        let nil = Tree::Atom(&[]);
        let zero = Tree::Atom(&[0]);
        let padded = Tree::Atom(&[0, 1]);
        let negative = Tree::Atom(&[255]);
        let last = Tree::Atom(&[127]);
        let c = Tree::Pair(&last, &nil);
        let b = Tree::Pair(&negative, &c);
        let a = Tree::Pair(&padded, &b);
        let env = Tree::Pair(&a, &nil);
        assert_eq!(borrowed(&zero), zero);
        assert_eq!(literal_borrowed(&nil),padded);
        assert_eq!(literal_borrowed(&zero),zero);
        assert_eq!(borrowed(&env), env);
        exact(&map_add(&env), &[&[2], &[], &[0, 128]]);
        exact(&map_add(&Tree::Pair(&nil, &zero)), &[]);
        fail(std::panic::catch_unwind(|| {
            map_add(&Tree::Pair(&zero, &nil))
        }));
        let bad = Tree::Pair(&env, &nil);
        fail(std::panic::catch_unwind(|| {
            map_add(&Tree::Pair(&bad, &nil))
        }));
    }
    #[test]
    fn initializer_before_argument_and_unused_parameter() {
        assert!(
            fail(std::panic::catch_unwind(|| strict_initialization(false)))
                .contains("assertion failed")
        );
        assert!(
            fail(std::panic::catch_unwind(|| strict_initialization(true))).contains("InvalidOpArg")
        );
    }
}
