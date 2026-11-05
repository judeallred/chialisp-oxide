//! Actual native Rust inputs for the closed local callable prototype.
use chialisp_oxide::{OwnedTree, Tree, apply, raw_clvm as raw};

type Step = fn(u64) -> u64;
fn increment(value: u64) -> u64 {
    assert!(value < 100);
    value + 1
}
fn double(value: u64) -> u64 {
    assert!(value < 100);
    value * 2
}
fn invoke(step: Step, value: u64) -> u64 {
    step(value)
}

#[chialisp_oxide::puzzle]
pub fn choose(flag: bool, value: u64) -> u64 {
    let step: Step = if flag { increment } else { double };
    invoke(step, value)
}
#[chialisp_oxide::puzzle]
pub fn copied(flag: bool, value: u64) -> (u64, u64) {
    let step: Step = if flag { increment } else { double };
    let saved = step;
    let step: Step = if flag { double } else { increment };
    (invoke(saved, value), invoke(step, value))
}
#[chialisp_oxide::puzzle]
pub fn unused_ref(flag: bool) -> u64 {
    let _step: Step = if flag { increment } else { double };
    7
}

mod core {
    pub mod panicking {
        fn sha256(value: u64) -> u64 { value + 1 }
        fn apply(value: u64) -> u64 { value + 2 }
        pub fn panic(value: u64) -> u64 {
            let hash_name: fn(u64) -> u64 = sha256;
            let apply_name: fn(u64) -> u64 = apply;
            apply_name(hash_name(value))
        }
    }
}
#[chialisp_oxide::puzzle]
pub fn same_name(value: u64) -> u64 {
    let step: Step = core::panicking::panic;
    invoke(step, value)
}

fn borrowed_identity<'a>(value: &'a Tree<'a>) -> Tree<'a> {
    *value
}
fn borrowed_invoke<'a>(
    step: for<'v> fn(&'v Tree<'v>) -> Tree<'v>,
    value: &'a Tree<'a>,
) -> Tree<'a> {
    step(value)
}
#[chialisp_oxide::puzzle]
pub fn borrowed<'a>(value: &'a Tree<'a>) -> Tree<'a> {
    let step: for<'v> fn(&'v Tree<'v>) -> Tree<'v> = borrowed_identity;
    borrowed_invoke(step, value)
}

fn countdown(value: u64) -> u64 {
    if value == 0 { 7 } else { countdown(value - 1) }
}
#[chialisp_oxide::puzzle]
pub fn recursive(value: u64) -> u64 {
    let step: Step = countdown;
    invoke(step, value)
}
#[chialisp_oxide::puzzle]
pub fn self_callback(value: u64) -> u64 {
    if value == 0 {
        7
    } else {
        let step: Step = self_callback;
        invoke(step, value - 1)
    }
}
#[allow(unconditional_recursion)]
fn forever(value: u64) -> u64 {
    forever(value)
}
#[chialisp_oxide::puzzle]
pub fn divergence(flag: bool, value: u64) -> u64 {
    let step: Step = if flag { forever } else { increment };
    invoke(step, value)
}

type Transform = fn(Tree<'static>) -> Tree<'static>;
fn keep(value: Tree<'static>) -> Tree<'static> {
    value
}
fn keep_first(first: Tree<'static>, _: Tree<'static>) -> Tree<'static> {
    first
}
fn discard(_: Tree<'static>) -> u64 {
    7
}
#[chialisp_oxide::puzzle]
pub fn callee_order(flag: bool) -> Tree<'static> {
    let step: Transform = {
        assert!(flag);
        keep
    };
    step(raw::first(&Tree::Atom(&[])))
}
#[chialisp_oxide::puzzle]
pub fn arguments_order(reverse: bool) -> Tree<'static> {
    let step: fn(Tree<'static>, Tree<'static>) -> Tree<'static> = keep_first;
    if reverse {
        step(raw::raise(&Tree::Atom(&[])), raw::first(&Tree::Atom(&[])))
    } else {
        step(raw::first(&Tree::Atom(&[])), raw::raise(&Tree::Atom(&[])))
    }
}
#[chialisp_oxide::puzzle]
pub fn unused_argument() -> u64 {
    let step: fn(Tree<'static>) -> u64 = discard;
    step(raw::first(&Tree::Atom(&[])))
}

fn add_one(value: Tree<'static>) -> Tree<'static> {
    raw::add(&value, &Tree::Atom(&[1]))
}
fn twice(value: Tree<'static>) -> Tree<'static> {
    raw::mul(&value, &Tree::Atom(&[2]))
}
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
    let step: Transform = add_one;
    map(step, &values)
}
#[chialisp_oxide::puzzle]
pub fn map_choose(flag: bool, values: &Tree<'_>) -> OwnedTree<'static> {
    let values = apply(&Tree::Atom(&[1]), values);
    let step: Transform = if flag { add_one } else { twice };
    map(step, &values)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn exact(value: &OwnedTree<'_>, bytes: &[&[u8]]) {
        match (value, bytes.split_first()) {
            (OwnedTree::Atom(atom), None) => assert!(atom.is_empty()),
            (OwnedTree::Pair(first, rest), Some((expected, tail))) => {
                let OwnedTree::Atom(actual) = first.as_ref() else {
                    panic!("expected an atom")
                };
                assert_eq!(actual, expected);
                exact(rest, tail);
            }
            _ => panic!("wrong output shape"),
        }
    }
    fn failure<T>(result: std::thread::Result<T>) -> String {
        let payload = result.err().expect("expected program failure");
        assert!(
            !payload.is::<chialisp_oxide::ResourceExhausted>(),
            "resource is inconclusive"
        );
        if let Some(message) = payload.downcast_ref::<String>() {
            message.clone()
        } else {
            payload
                .downcast_ref::<&str>()
                .expect("policy diagnostic")
                .to_string()
        }
    }
    #[test]
    fn real_function_pointer_values_retain_the_selected_target_and_shadow_snapshot() {
        assert_eq!(choose(true, 7), 8);
        assert_eq!(choose(false, 7), 14);
        assert_eq!(copied(true, 7), (8, 14));
        assert_eq!(copied(false, 7), (14, 8));
        assert_eq!(same_name(7), 10);
        assert_eq!(recursive(5), 7);
        assert_eq!(self_callback(5), 7);
        assert_eq!(unused_ref(true), 7);
        assert_eq!(divergence(false, 7), 8);
        assert_eq!(borrowed(&Tree::Atom(&[0, 1])), Tree::Atom(&[0, 1]));
        failure(std::panic::catch_unwind(|| choose(true, 100)));
    }
    #[test]
    fn actual_native_map_reuses_a_callable_with_arbitrary_numeric_atoms() {
        let nil = Tree::Atom(&[]);
        let third = Tree::Atom(&[0x7f]);
        let second = Tree::Atom(&[0xff]);
        let first = Tree::Atom(&[0, 1]);
        let c = Tree::Pair(&third, &nil);
        let b = Tree::Pair(&second, &c);
        let a = Tree::Pair(&first, &b);
        let env = Tree::Pair(&a, &nil);
        exact(&map_add(&env), &[&[2], &[], &[0, 0x80]]);
        exact(&map_choose(false, &a), &[&[2], &[0xfe], &[0, 0xfe]]);
        failure(std::panic::catch_unwind(|| {
            map_choose(true, &Tree::Atom(&[0]))
        }));
    }
    #[test]
    fn call_operand_and_argument_failures_remain_strict_and_distinguishable() {
        assert!(
            failure(std::panic::catch_unwind(|| callee_order(false))).contains("assertion failed")
        );
        assert!(failure(std::panic::catch_unwind(|| callee_order(true))).contains("InvalidOpArg"));
        assert!(
            failure(std::panic::catch_unwind(|| arguments_order(false))).contains("InvalidOpArg")
        );
        assert!(failure(std::panic::catch_unwind(|| arguments_order(true))).contains("Raise"));
        assert!(failure(std::panic::catch_unwind(unused_argument)).contains("InvalidOpArg"));
    }
}
