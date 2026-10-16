#[derive(Clone, Copy)]
pub enum Tree<'a> {
    Atom(&'a [u8]),
    Pair(&'a Tree<'a>, &'a Tree<'a>),
}
const QUOTE: Tree<'static> = Tree::Atom(&[1]);
fn quote<'a>(value: &'a Tree<'a>) -> Tree<'a> {
    Tree::Pair(&QUOTE, value)
}
#[doc = "__chialisp_oxide_puzzle_v3_environment_raw"]
pub fn puzzle<'a>(environment: &'a Tree<'a>) -> Tree<'a> {
    match environment {
        Tree::Pair(first, _) => quote(first),
        Tree::Atom(_) => panic!(),
    }
}
