use chialisp_oxide::{Amount, CreateCoin, PuzzleHash, Tree, create_coin};

pub fn puzzle(approvals: &Tree<'_>, recipient: PuzzleHash, amount: Amount) -> (CreateCoin,) {
    match approvals {
        Tree::Pair(_, _) => (create_coin(recipient, amount),),
        Tree::Atom(_) => panic!("no approvals"),
    }
}

#[test]
fn a_nonempty_list_pays() {
    let one = Tree::Atom(&[1]);
    let nil = Tree::Atom(&[]);
    let approvals = Tree::Pair(&one, &nil);
    let (condition,) = puzzle(&approvals, PuzzleHash([0x42; 32]), Amount::from_mojos(7));
    assert_eq!(condition.0, 51);
    assert_eq!(condition.2.mojos(), 7);
}

#[test]
#[should_panic]
fn an_empty_list_is_rejected() {
    puzzle(&Tree::Atom(&[]), PuzzleHash([0x42; 32]), Amount::from_mojos(7));
}

#[test]
#[should_panic]
fn a_nonempty_atom_is_not_a_list() {
    puzzle(&Tree::Atom(&[0]), PuzzleHash([0x42; 32]), Amount::from_mojos(7));
}
