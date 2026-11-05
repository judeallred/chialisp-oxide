use chialisp_oxide::OwnedTree;

/// Copy a proper list spine onto any tail tree, preserving every atom byte.
#[chialisp_oxide::puzzle]
pub fn puzzle<'a>(list: OwnedTree<'a>, tail: OwnedTree<'a>) -> OwnedTree<'a> {
    prefix(list, tail)
}

fn prefix<'a>(list: OwnedTree<'a>, tail: OwnedTree<'a>) -> OwnedTree<'a> {
    match list {
        OwnedTree::Atom([]) => tail,
        OwnedTree::Pair(first, rest) => OwnedTree::Pair(first, Box::new(prefix(*rest, tail))),
        OwnedTree::Atom(_) => panic!(),
    }
}
