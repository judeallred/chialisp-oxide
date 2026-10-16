mod support;
use chialisp_oxide_backend_experiment::Outcome;
use chialisp_oxide_ir::{Environment, Program, Type};
use support::Fixture;

#[path = "support/vm.rs"]
mod vm;
use vm::execute;

const EXPORTS: &str = r#"
use clvm::{Tree,quote};
#[clvm::puzzle(environment="raw")]
pub fn p2<'a>(environment:&'a Tree<'a>)->Tree<'a> {
    match environment { Tree::Pair(conditions,_)=>quote(conditions), Tree::Atom(_)=>panic!() }
}
#[clvm::puzzle(environment="raw")]
pub fn identity<'a>(value:&'a Tree<'a>)->Tree<'a> { *value }
#[clvm::puzzle(environment="raw")]
pub fn atom_bytes<'a>(value:&'a Tree<'a>)->&'a [u8] {
    match value { Tree::Atom(bytes)=>bytes, Tree::Pair(_,_)=>panic!() }
}
#[clvm::puzzle(environment="raw")]
pub fn rebuild<'a>(value:&'a Tree<'a>)->Tree<'a> {
    match value { Tree::Atom(bytes)=>Tree::Atom(bytes),Tree::Pair(first,rest)=>Tree::Pair(first,rest) }
}
fn first<'a>(value:&'a Tree<'a>)->&'a Tree<'a> {
    match value { Tree::Pair(first,_)=>first, Tree::Atom(_)=>panic!() }
}
fn validate<'a>(value:&'a Tree<'a>) { let _ = first(value); }
#[clvm::puzzle(environment="raw")]
pub fn ignored_failure<'a>(value:&'a Tree<'a>)->Tree<'a> { validate(value); *value }
#[clvm::puzzle(environment="raw")]
pub fn lazy<'a>(value:&'a Tree<'a>)->Tree<'a> {
    match value { Tree::Atom(_)=>*value, Tree::Pair(_,_)=>*first(value) }
}
#[clvm::puzzle(environment="raw")]
pub fn discarded_tuple<'a>(value:&'a Tree<'a>)->Tree<'a> {
    let tuple = (*value,*first(value)); tuple.0
}
#[clvm::puzzle(environment="raw")]
pub fn is_pair<'a>(value:&'a Tree<'a>)->bool {
    match value { Tree::Atom(_)=>false,Tree::Pair(_,_)=>true }
}
#[clvm::puzzle] pub fn strict<'a>(value:&'a Tree<'a>)->Tree<'a> { *value }
#[clvm::puzzle(environment="ignored")] pub fn ignored()->u8 {7}
// Names and discriminant ordering grant no special interpretation.
#[derive(Clone,Copy)] pub enum Reversed<'a> { Cons(&'a Reversed<'a>,&'a Reversed<'a>), Leaf(&'a [u8]) }
#[clvm::puzzle(environment="raw")]
pub fn reversed<'a>(value:&'a Reversed<'a>)->Reversed<'a> {
    match value { Reversed::Cons(first,_)=>**first,Reversed::Leaf(bytes)=>Reversed::Leaf(bytes) }
}
#[cfg(test)] mod native {
    use super::*;
    #[test] fn ordinary_rust_tree_semantics() {
        let empty=Tree::Atom(&[]); let padded=Tree::Atom(&[0,1]);
        let improper=Tree::Pair(&padded,&empty);
        assert_eq!(p2(&improper),Tree::Pair(&clvm::QUOTE,&padded));
        assert_eq!(identity(&improper),improper);
        assert_eq!(rebuild(&improper),improper);
        assert_eq!(atom_bytes(&padded),&[0,1]);
        assert_eq!(lazy(&empty),empty);
        assert_eq!(lazy(&improper),padded);
        assert!(std::panic::catch_unwind(||p2(&padded)).is_err());
        assert!(std::panic::catch_unwind(||ignored_failure(&empty)).is_err());
        assert!(std::panic::catch_unwind(||discarded_tuple(&empty)).is_err());
        assert_eq!(ignored_failure(&improper),improper);
    }
}
"#;

fn find<'a>(programs: &'a [Program], suffix: &str) -> &'a Program {
    programs
        .iter()
        .find(|program| program.name.ends_with(&format!("::{suffix}")))
        .unwrap()
}
fn run(program: &Program, input: &str, optimized: bool) -> Outcome {
    execute(
        &chialisp_oxide_ir::compile(program, optimized).unwrap(),
        input,
        10_000_000,
    )
    .unwrap()
    .outcome
}
fn expected(input: &str) -> Outcome {
    // Independent CLVM identity, not the frontend or its expression evaluator.
    execute(&[1], input, 10_000_000).unwrap().outcome
}
fn assert_success(output: std::process::Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn real_sdk_borrows_matches_constants_and_helpers_preserve_raw_tree_semantics() {
    let fixture = Fixture::with_sdk_alias("clvm");
    fixture.source(EXPORTS);
    assert_success(fixture.cargo(true));
    let native = std::process::Command::new(
        std::path::Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/cargo"),
    )
    .current_dir(&fixture.0)
    .env("RUSTC", fixture.compiler())
    .env_remove("RUSTC_WRAPPER")
    .env_remove("RUSTC_WORKSPACE_WRAPPER")
    .env_remove("CHIALISP_OXIDE_CARGO_MODE")
    .args(["test", "--offline", "--lib", "--target-dir"])
    .arg(fixture.0.join("native-build"))
    .output()
    .unwrap();
    assert_success(native);
    let programs = fixture.exports();
    assert_eq!(programs.len(), 11);
    let reference =
        chialisp_oxide_reference::compile("(mod (conditions) (qq (q . (unquote conditions))))")
            .unwrap();
    let atoms = ["()", "1", "0x00", "0x0001", "0xff", "0x8000", "0x00000000"];
    let mut trees: Vec<String> = atoms.iter().map(|s| (*s).into()).collect();
    for first in atoms {
        for rest in atoms {
            trees.push(format!("({first} . {rest})"));
        }
    }
    trees.extend([
        "((0x0001 . 0x80) . (1 . 2))".into(),
        "(((1 . 2) . 3) . 4)".into(),
    ]);
    for optimized in [false, true] {
        for tree in &trees {
            let p2 = find(&programs, "p2");
            assert_eq!(p2.environment, Environment::Raw);
            assert_eq!(p2.parameters[0].ty, Type::Tree);
            assert_eq!(
                run(p2, tree, optimized),
                execute(&reference, tree, 10_000_000).unwrap().outcome,
                "p2 {tree} optimized={optimized}"
            );
            for name in ["identity", "rebuild"] {
                assert_eq!(
                    run(find(&programs, name), tree, optimized),
                    expected(tree),
                    "{name} {tree}"
                );
            }
            let pair = tree.starts_with('(') && tree != "()";
            for name in ["ignored_failure", "discarded_tuple"] {
                assert_eq!(
                    run(find(&programs, name), tree, optimized),
                    if pair {
                        expected(tree)
                    } else {
                        Outcome::Rejected
                    },
                    "{name} {tree}"
                );
            }
            assert_eq!(
                run(find(&programs, "atom_bytes"), tree, optimized),
                if pair {
                    Outcome::Rejected
                } else {
                    expected(tree)
                },
                "atom {tree}"
            );
            assert_eq!(
                run(find(&programs, "strict"), &format!("({tree})"), optimized),
                expected(tree)
            );
            assert_eq!(
                run(find(&programs, "is_pair"), tree, optimized),
                expected(if pair { "1" } else { "()" })
            );
        }
        assert_eq!(
            run(find(&programs, "lazy"), "()", optimized),
            expected("()")
        );
        assert_eq!(
            run(find(&programs, "lazy"), "((1 . 2) . 3)", optimized),
            expected("(1 . 2)")
        );
        assert_eq!(
            run(find(&programs, "reversed"), "(0x0001 . 3)", optimized),
            expected("0x0001")
        );
        assert_eq!(
            run(find(&programs, "reversed"), "0x0001", optimized),
            expected("0x0001")
        );
        assert_eq!(
            run(find(&programs, "strict"), "(1 . 2)", optimized),
            Outcome::Rejected
        );
    }
}

#[test]
fn raw_policy_and_borrowed_tree_subset_fail_closed_before_any_export() {
    let fixture = Fixture::new();
    for (source, diagnostic) in [
        (
            "#[chialisp_oxide::puzzle(environment=\"raw\")] pub fn invalid() {}",
            "exactly one tree parameter",
        ),
        (
            "#[chialisp_oxide::puzzle(environment=\"raw\")] pub fn invalid(_:()) {}",
            "exactly one tree parameter",
        ),
        (
            "#[chialisp_oxide::puzzle(environment=\"raw\")] pub fn invalid(_:u8) {}",
            "exactly one tree parameter",
        ),
        (
            "#[chialisp_oxide::puzzle(environment=\"raw\")] pub fn invalid(_: &[u8]) {}",
            "exactly one tree parameter",
        ),
        (
            "#[chialisp_oxide::puzzle(environment=\"raw\")] pub fn invalid(_: &chialisp_oxide::Tree<'_>,_:u8) {}",
            "exactly one tree parameter",
        ),
        (
            "#[chialisp_oxide::puzzle(environment=\"raw\")] pub fn invalid(_: &mut chialisp_oxide::Tree<'_>) {}",
            "unsupported Rust type",
        ),
        (
            "#[chialisp_oxide::puzzle] pub fn invalid(_: &u8) {}",
            "unsupported Rust type",
        ),
        (
            "#[chialisp_oxide::puzzle(environment=\"raw\")] pub fn invalid<'a,T>(v:&'a chialisp_oxide::Tree<'a>)->chialisp_oxide::Tree<'a>{*v}",
            "generic puzzle functions",
        ),
        (
            "#[chialisp_oxide::puzzle(environment=\"raw\")] pub fn invalid<'a,const N:usize>(v:&'a chialisp_oxide::Tree<'a>)->chialisp_oxide::Tree<'a>{*v}",
            "generic puzzle functions",
        ),
        (
            "#[doc=\"__chialisp_oxide_puzzle_v3_environment_raw\"] #[doc=\"__oxide_puzzle_v1\"] pub fn invalid() {}",
            "one annotation",
        ),
        (
            "#[doc=\"__chialisp_oxide_puzzle_v3_environment_raw\"] #[doc=\"__chialisp_oxide_puzzle_v2_environment_ignored\"] pub fn invalid() {}",
            "one annotation",
        ),
        (
            "#[doc=\"__chialisp_oxide_puzzle_v3_environment_raw\"] #[doc=\"__chialisp_oxide_puzzle_v3_environment_raw\"] pub fn invalid() {}",
            "one annotation",
        ),
        (
            "#[doc=\"__chialisp_oxide_puzzle_v3_environment_raw\"] pub fn invalid(_:u8) {}",
            "exactly one tree parameter",
        ),
        (
            "#[doc=\"__chialisp_oxide_puzzle_v3_environment_raw\"] pub struct Invalid;",
            "on a free function",
        ),
        (
            "pub enum Bad<'a>{Atom(&'a [u8]),Pair(&'a Bad<'a>,&'a Bad<'a>),Extra} #[chialisp_oxide::puzzle(environment=\"raw\")] pub fn invalid<'a>(v:&'a Bad<'a>)->Bad<'a>{panic!()}",
            "borrowed trees require exactly",
        ),
        (
            "#[repr(u8)] pub enum Bad<'a>{Atom(&'a [u8]),Pair(&'a Bad<'a>,&'a Bad<'a>)} #[chialisp_oxide::puzzle(environment=\"raw\")] pub fn invalid<'a>(v:&'a Bad<'a>)->Bad<'a>{panic!()}",
            "borrowed trees require exactly",
        ),
        (
            "pub enum Bad<'a>{Atom(&'a [u8]),Pair(&'a Bad<'a>,&'a Bad<'a>)} impl Drop for Bad<'_>{fn drop(&mut self){}} #[chialisp_oxide::puzzle(environment=\"raw\")] pub fn invalid<'a>(v:&'a Bad<'a>)->Bad<'a>{panic!()}",
            "borrowed trees require exactly",
        ),
        (
            "pub enum Bad<'a>{Atom(&'a [u8]),Pair(&'a Bad<'a>,u8)} #[chialisp_oxide::puzzle(environment=\"raw\")] pub fn invalid<'a>(v:&'a Bad<'a>)->Bad<'a>{panic!()}",
            "borrowed trees require exactly",
        ),
        (
            "macro_rules! tree {()=>{pub enum Bad<'a>{Atom(&'a [u8]),Pair(&'a Bad<'a>,&'a Bad<'a>)}}} tree!(); #[chialisp_oxide::puzzle(environment=\"raw\")] pub fn invalid<'a>(v:&'a Bad<'a>)->Bad<'a>{panic!()}",
            "borrowed trees require exactly",
        ),
        (
            "#[chialisp_oxide::puzzle(environment=\"raw\")] pub fn invalid<'a>(v:&'a chialisp_oxide::Tree<'a>)->&'a chialisp_oxide::Tree<'a>{let local=*v; &local}",
            "cannot return reference to local variable",
        ),
        (
            "#[chialisp_oxide::puzzle] pub fn invalid(v:&[u8;2])->chialisp_oxide::Tree<'_>{chialisp_oxide::Tree::Atom(v)}",
            "dynamic byte-array unsizing is unsupported",
        ),
    ] {
        fixture.source(&format!(
            "#[chialisp_oxide::puzzle] pub fn valid()->u8{{7}}\n{source}"
        ));
        let output = fixture.cargo(true);
        assert!(!output.status.success(), "accepted {source}");
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(
            errors.contains(diagnostic),
            "{source}: {diagnostic}: {errors}"
        );
        assert!(!errors.contains("internal compiler error"), "{errors}");
        assert!(fixture.exports().is_empty(), "partial export: {source}");
    }
}

#[test]
fn same_named_fake_sdk_cannot_substitute_tree_layout_or_helper_meaning() {
    let fixture = Fixture::new();
    let forged = fixture.dependency_source(
        "chialisp_oxide",
        true,
        r#"
        #[derive(Clone,Copy)] pub enum Tree<'a>{Atom(&'a [u8]),Pair(&'a Tree<'a>,&'a Tree<'a>)}
        pub fn quote<'a>(value:&'a Tree<'a>)->Tree<'a>{ *value }
    "#,
    );
    fixture.source(r#"#[doc="__chialisp_oxide_puzzle_v3_environment_raw"] pub fn entry<'a>(v:&'a fake::Tree<'a>)->fake::Tree<'a>{fake::quote(v)}"#);
    assert_success(fixture.check_external("fake", &forged));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 1);
    for optimized in [false, true] {
        for tree in ["0x0001", "(1 . 2)"] {
            assert_eq!(run(&programs[0], tree, optimized), expected(tree));
        }
    }
    std::fs::remove_dir_all(fixture.0.join("exports")).unwrap();
    let changed_quote = fixture.dependency_source(
        "chialisp_oxide",
        true,
        r#"
        #[derive(Clone,Copy)] pub enum Tree<'a>{Atom(&'a [u8]),Pair(&'a Tree<'a>,&'a Tree<'a>)}
        pub const QUOTE:Tree<'static>=Tree::Atom(&[2]);
        pub fn quote<'a>(value:&'a Tree<'a>)->Tree<'a>{Tree::Pair(&QUOTE,value)}
        "#,
    );
    assert_success(fixture.check_external("fake", &changed_quote));
    let programs = fixture.exports();
    for optimized in [false, true] {
        for tree in ["0x0001", "(1 . 2)"] {
            assert_eq!(
                run(&programs[0], tree, optimized),
                expected(&format!("(2 . {tree})")),
                "named QUOTE bytes must come from actual constant MIR"
            );
        }
    }
    std::fs::remove_dir_all(fixture.0.join("exports")).unwrap();
    let altered = fixture.dependency_source(
        "chialisp_oxide",
        true,
        "pub enum Tree<'a>{Atom(&'a [u8]),Pair(&'a Tree<'a>,u8)}",
    );
    fixture.source(r#"#[doc="__chialisp_oxide_puzzle_v3_environment_raw"] pub fn entry<'a>(v:&'a fake::Tree<'a>)->u8 {7}"#);
    let output = fixture.check_external("fake", &altered);
    assert!(!output.status.success());
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(
        errors.contains("borrowed trees require exactly"),
        "{errors}"
    );
    assert!(fixture.exports().is_empty());
}
