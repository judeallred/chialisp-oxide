use chialisp_oxide::OwnedTree;

#[chialisp_oxide::puzzle]
pub fn prefix<'a>(list: OwnedTree<'a>, tail: OwnedTree<'a>) -> OwnedTree<'a> {
    match list {
        OwnedTree::Atom([]) => tail,
        OwnedTree::Atom(_) => panic!(),
        OwnedTree::Pair(first, rest) => OwnedTree::Pair(first, Box::new(prefix(*rest, tail))),
    }
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn construct<'a>(value: OwnedTree<'a>) -> OwnedTree<'a> {
    OwnedTree::Pair(Box::new(OwnedTree::Atom(&[1])), Box::new(value))
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn head<'a>(value: OwnedTree<'a>) -> OwnedTree<'a> {
    match value {
        OwnedTree::Pair(first, _rest) => *first,
        OwnedTree::Atom(_) => panic!(),
    }
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn tail<'a>(value: OwnedTree<'a>) -> OwnedTree<'a> {
    match value {
        OwnedTree::Pair(_first, rest) => *rest,
        OwnedTree::Atom(_) => panic!(),
    }
}

fn is_pair(value: &OwnedTree<'_>) -> bool {
    matches!(value, OwnedTree::Pair(_, _))
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn borrow_then_move<'a>(value: OwnedTree<'a>) -> (bool, OwnedTree<'a>) {
    let pair = is_pair(&value);
    (pair, value)
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn boxed_identity<'a>(value: OwnedTree<'a>) -> OwnedTree<'a> {
    let boxed = Box::new(value);
    *boxed
}

fn atom_bytes<'a>(value: &OwnedTree<'a>) -> &'a [u8] {
    match value {
        OwnedTree::Atom(bytes) => bytes,
        OwnedTree::Pair(_, _) => panic!(),
    }
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn snapshot<'a>(value: OwnedTree<'a>) -> OwnedTree<'a> {
    let mut boxed = Box::new(value);
    let saved = atom_bytes(&*boxed);
    let moved = *boxed;
    boxed = Box::new(OwnedTree::Atom(&[2]));
    let _discarded = moved;
    let _other = boxed;
    OwnedTree::Atom(saved)
}

fn snapshot_inner<'a>(value: OwnedTree<'a>, depth: u8) -> OwnedTree<'a> {
    let mut boxed = Box::new(value);
    let saved = atom_bytes(&*boxed);
    let moved = *boxed;
    boxed = Box::new(OwnedTree::Atom(&[2]));
    let _discarded = moved;
    let _other = boxed;
    if depth > 0 {
        let _ignored = snapshot_inner(OwnedTree::Atom(&[2]), depth - 1);
    }
    OwnedTree::Atom(saved)
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn runtime_snapshot<'a>(value: OwnedTree<'a>) -> OwnedTree<'a> {
    snapshot_inner(value, 2)
}

// This deliberately exercises the admitted shared-reference-to-Box boundary.
fn shared_box_pair(value: &Box<OwnedTree<'_>>) -> bool {
    matches!(&**value, OwnedTree::Pair(_, _))
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn shared_box_then_move<'a>(value: OwnedTree<'a>) -> (bool, OwnedTree<'a>) {
    let boxed = Box::new(value);
    let pair = shared_box_pair(&boxed);
    (pair, *boxed)
}

mod fake {
    use chialisp_oxide::OwnedTree;
    pub struct Box;
    impl Box {
        pub fn new(_input: OwnedTree<'_>) -> OwnedTree<'_> {
            OwnedTree::Atom(&[2])
        }
    }
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn misleading_name<'a>(value: OwnedTree<'a>) -> OwnedTree<'a> {
    fake::Box::new(value)
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn ignored_owned(_value: OwnedTree<'_>) -> u8 {
    7
}

fn assert_nonempty<'a>(value: OwnedTree<'a>) -> OwnedTree<'a> {
    match &value {
        OwnedTree::Atom([]) => panic!(),
        _ => value,
    }
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn unused_constructor(value: OwnedTree<'_>) -> u8 {
    let _discarded = Box::new(assert_nonempty(value));
    7
}

fn overflow(value: u8) -> OwnedTree<'static> {
    let _checked = value + 1;
    OwnedTree::Atom(&[7])
}

#[chialisp_oxide::puzzle]
pub fn unused_overflow(value: u8) -> u8 {
    let _discarded = Box::new(overflow(value));
    7
}

#[chialisp_oxide::puzzle]
pub fn lazy(choose: bool, value: u8) -> OwnedTree<'static> {
    if choose {
        overflow(value)
    } else {
        OwnedTree::Atom(&[7])
    }
}

#[allow(unconditional_recursion)]
fn spin<'a>(value: OwnedTree<'a>) -> OwnedTree<'a> {
    spin(value)
}

fn reject() -> OwnedTree<'static> {
    panic!()
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn spin_before_reject<'a>(value: OwnedTree<'a>) -> OwnedTree<'a> {
    OwnedTree::Pair(Box::new(spin(value)), Box::new(reject()))
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn reject_before_spin<'a>(value: OwnedTree<'a>) -> OwnedTree<'a> {
    OwnedTree::Pair(Box::new(reject()), Box::new(spin(value)))
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn unused_spin(value: OwnedTree<'_>) -> u8 {
    let _discarded = Box::new(spin(value));
    7
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn unused_spin_then_reject(value: OwnedTree<'_>) -> u8 {
    let _first = Box::new(spin(value));
    let _second = Box::new(reject());
    7
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn unused_reject_then_spin(value: OwnedTree<'_>) -> u8 {
    let _first = Box::new(reject());
    let _second = Box::new(spin(value));
    7
}

// Names and variant order are deliberately unlike the facade's enum.
pub enum Reversed<'a> {
    Branch(Box<Reversed<'a>>, Box<Reversed<'a>>),
    Leaf(&'a [u8]),
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn structural<'a>(value: Reversed<'a>) -> Reversed<'a> {
    match value {
        Reversed::Branch(first, rest) => Reversed::Branch(rest, first),
        Reversed::Leaf(bytes) => Reversed::Leaf(bytes),
    }
}

#[cfg(test)]
mod native;
