//! Reduced ordinary Cargo consumer for actual generated-source mutants.
use chialisp_oxide::Tree;

#[path = "generated/multiply.rs"]
mod multiply;

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn multiply_witness<'a>(environment: &'a Tree<'a>) -> Tree<'static> {
    match environment {
        Tree::Pair(program, complete_environment) => multiply::run(program, complete_environment),
        Tree::Atom(_) => panic!(),
    }
}

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn strict_unused_multiply_witness<'a>(environment: &'a Tree<'a>) -> Tree<'static> {
    let _ = multiply_witness(environment);
    Tree::Atom(&[])
}
