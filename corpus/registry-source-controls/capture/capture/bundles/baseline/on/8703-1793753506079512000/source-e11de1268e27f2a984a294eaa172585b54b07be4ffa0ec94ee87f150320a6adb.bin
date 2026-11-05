use chialisp_oxide::OwnedTree;

/// Apply the published insertion helper to arbitrary trees, without presorting
/// or validating keys that its original lazy branches would never inspect.
#[chialisp_oxide::puzzle]
pub fn puzzle<'a>(sorted: OwnedTree<'a>, insertions: OwnedTree<'a>) -> OwnedTree<'a> {
    insertion_sort_by_keys(sorted, insertions)
}

fn insertion_sort_by_keys<'a>(sorted: OwnedTree<'a>, insertions: OwnedTree<'a>) -> OwnedTree<'a> {
    if empty(&insertions) {
        sorted
    } else if empty(&sorted) || first_key(&insertions) > first_key(&sorted) {
        match insertions {
            OwnedTree::Pair(first, rest) => {
                insertion_sort_by_keys(OwnedTree::Pair(first, Box::new(sorted)), *rest)
            }
            OwnedTree::Atom(_) => panic!(),
        }
    } else {
        match sorted {
            OwnedTree::Pair(first_sorted, rest_sorted) => match insertions {
                OwnedTree::Pair(first, rest) => {
                    let singleton = OwnedTree::Pair(first, Box::new(OwnedTree::Atom(&[])));
                    let inserted = insertion_sort_by_keys(*rest_sorted, singleton);
                    insertion_sort_by_keys(OwnedTree::Pair(first_sorted, Box::new(inserted)), *rest)
                }
                OwnedTree::Atom(_) => panic!(),
            },
            OwnedTree::Atom(_) => panic!(),
        }
    }
}

fn empty(tree: &OwnedTree<'_>) -> bool {
    matches!(tree, OwnedTree::Atom([]))
}

fn first_key<'a>(list: &OwnedTree<'a>) -> &'a [u8] {
    match list {
        OwnedTree::Pair(entry, _) => match &**entry {
            OwnedTree::Pair(key, _) => match &**key {
                OwnedTree::Atom(bytes) => bytes,
                OwnedTree::Pair(_, _) => panic!(),
            },
            OwnedTree::Atom(_) => panic!(),
        },
        OwnedTree::Atom(_) => panic!(),
    }
}
