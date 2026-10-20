use chialisp_oxide::{Tree, apply};

/// Run an untrusted inner puzzle on its solution and return its result.
#[chialisp_oxide::puzzle]
pub fn delegate<'a>(inner_puzzle: &'a Tree<'a>, inner_solution: &'a Tree<'a>) -> Tree<'static> {
    apply(inner_puzzle, inner_solution)
}

/// Apply twice: the first result is itself the program for the second run.
#[chialisp_oxide::puzzle]
pub fn nested<'a>(program: &'a Tree<'a>, environment: &'a Tree<'a>) -> Tree<'static> {
    let produced = apply(program, environment);
    apply(&produced, environment)
}

/// An unused application still evaluates: a failing program rejects.
#[chialisp_oxide::puzzle]
pub fn unused<'a>(program: &'a Tree<'a>, environment: &'a Tree<'a>) -> u64 {
    let _ignored = apply(program, environment);
    7
}

/// Unchosen branches stay lazy: a failing program in the other arm is fine.
#[chialisp_oxide::puzzle]
pub fn lazy<'a>(choose: bool, program: &'a Tree<'a>, environment: &'a Tree<'a>) -> Tree<'static> {
    if choose {
        apply(program, environment)
    } else {
        Tree::Atom(&[9])
    }
}

/// Operands bind in order before the application; a later rejection still
/// happens after the application was evaluated.
#[chialisp_oxide::puzzle]
pub fn apply_then_reject<'a>(program: &'a Tree<'a>, environment: &'a Tree<'a>) -> Tree<'static> {
    let produced = apply(program, environment);
    assert!(matches!(produced, Tree::Pair(_, _)));
    produced
}

mod misleading {
    use chialisp_oxide::Tree;
    /// Same name, ordinary body: never opcode 2.
    pub fn apply<'a>(_: &'a Tree<'a>, _: &'a Tree<'a>) -> Tree<'static> {
        Tree::Atom(&[8])
    }
}

#[chialisp_oxide::puzzle]
pub fn fake_apply<'a>(program: &'a Tree<'a>, environment: &'a Tree<'a>) -> Tree<'static> {
    misleading::apply(program, environment)
}

#[cfg(test)]
mod native {
    use super::*;
    use chialisp_oxide::ResourceExhausted;
    const NIL: Tree<'static> = Tree::Atom(&[]);
    const ONE: Tree<'static> = Tree::Atom(&[1]);
    const SEVEN: Tree<'static> = Tree::Atom(&[7]);
    #[test]
    fn quoted_program_returns_its_payload() {
        let quoted = Tree::Pair(&ONE, &SEVEN);
        assert_eq!(delegate(&quoted, &NIL), SEVEN);
        assert_eq!(unused(&quoted, &NIL), 7);
        assert_eq!(lazy(false, &NIL, &NIL), Tree::Atom(&[9]));
    }
    #[test]
    fn environment_program_returns_the_environment() {
        let environment = Tree::Pair(&SEVEN, &NIL);
        assert_eq!(delegate(&ONE, &environment), environment);
    }
    #[test]
    fn raising_program_rejects() {
        let raise = Tree::Pair(&Tree::Atom(&[8]), &NIL);
        assert!(std::panic::catch_unwind(|| delegate(&raise, &NIL)).is_err());
        let error = std::panic::catch_unwind(|| unused(&raise, &NIL)).unwrap_err();
        assert!(error.downcast_ref::<ResourceExhausted>().is_none());
    }
    #[test]
    fn same_name_helper_keeps_its_body() {
        assert_eq!(fake_apply(&NIL, &NIL), Tree::Atom(&[8]));
    }
}
