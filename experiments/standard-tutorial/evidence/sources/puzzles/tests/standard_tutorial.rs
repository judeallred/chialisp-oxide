//! Actual Cargo policy calls. Keys are public disposable fixtures.
//! Native raw group/apply uses the pinned VM, not independent crypto.
use chialisp_oxide::{OwnedTree, Tree};
use chialisp_oxide_puzzles::standard_delegated_hidden::puzzle;
fn atom(bytes: &'static [u8]) -> Tree<'static> {
    Tree::Atom(bytes)
}
fn pair(a: Tree<'static>, b: Tree<'static>) -> Tree<'static> {
    Tree::Pair(Box::leak(Box::new(a)), Box::leak(Box::new(b)))
}
fn bytes(hex: &str) -> &'static [u8] {
    Box::leak(
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
}
fn same(a: &OwnedTree<'_>, b: &Tree<'_>) -> bool {
    match (a, b) {
        (OwnedTree::Atom(x), Tree::Atom(y)) => x == y,
        (OwnedTree::Pair(x, y), Tree::Pair(u, v)) => same(x, u) && same(y, v),
        _ => false,
    }
}

fn env(
    key: Tree<'static>,
    original: Tree<'static>,
    program: Tree<'static>,
    solution: Tree<'static>,
    tail: Tree<'static>,
) -> Tree<'static> {
    pair(key, pair(original, pair(program, pair(solution, tail))))
}
#[test]
fn delegated_signature_binds_program_and_preserves_arbitrary_solution_tree() {
    let hash = bytes("9dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2");
    for key in [atom(&[]), atom(&[0]), pair(atom(&[1]), atom(&[2]))] {
        for solution in [atom(&[]), atom(&[0, 1]), pair(atom(&[7]), atom(&[8]))] {
            let signature = pair(atom(&[50]), pair(key, pair(atom(hash), atom(&[]))));
            let input = env(key, atom(&[]), atom(&[1]), solution, atom(&[99]));
            assert!(same(&puzzle(&input), &pair(signature, solution)));
        }
    }
}
#[test]
fn hidden_fixture_relation_returns_conditions_without_added_signature() {
    let original = atom(bytes(
        "a010d140e7c43146b5bb59695e6c444abbb62e964a535d0034351a90d1192bff0130de95f9bbc58af254c4dab4e65d3a",
    ));
    let synthetic = atom(bytes(
        "b90cf490d6b15ad0a61943b73cb7b53ec261e2149dabd076ff74ebd0371769916f5927fffb75215063439a5445dd40a7",
    ));
    for solution in [atom(&[]), atom(&[0]), pair(atom(&[7]), atom(&[8]))] {
        let input = env(synthetic, original, atom(&[1]), solution, atom(&[99]));
        assert!(same(&puzzle(&input), &solution));
    }
}
#[test]
fn nonempty_zero_is_truthy_and_hidden_mismatch_never_becomes_a_delegated_spend() {
    for original in [atom(&[0]), atom(&[0, 0]), pair(atom(&[1]), atom(&[2]))] {
        let input = env(atom(&[17]), original, atom(&[1]), atom(&[]), atom(&[]));
        let panic = std::panic::catch_unwind(|| puzzle(&input))
            .err()
            .expect("must reject");
        assert!(!panic.is::<chialisp_oxide::ResourceExhausted>());
    }
    for input in [atom(&[]), atom(&[1]), pair(atom(&[1]), atom(&[]))] {
        assert!(std::panic::catch_unwind(|| puzzle(&input)).is_err());
    }
}

#[test]
fn hidden_key_mismatch_skips_a_diverging_hidden_program() {
    let key = atom(bytes(
        "a010d140e7c43146b5bb59695e6c444abbb62e964a535d0034351a90d1192bff0130de95f9bbc58af254c4dab4e65d3a",
    ));
    // (a 1 1) applied to itself would exhaust the native VM ceiling.
    let program = pair(atom(&[2]), pair(atom(&[1]), pair(atom(&[1]), atom(&[]))));
    let input = env(key, key, program, program, atom(&[]));
    let payload = std::panic::catch_unwind(|| puzzle(&input))
        .err()
        .expect("wrong opening must reject");
    assert!(
        !payload.is::<chialisp_oxide::ResourceExhausted>(),
        "unselected hidden program was evaluated"
    );
}
