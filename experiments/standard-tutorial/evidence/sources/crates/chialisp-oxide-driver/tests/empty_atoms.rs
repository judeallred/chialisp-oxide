//! Empty-slice patterns retain real byte-length semantics, including padded
//! zero atoms. General slice lengths and pointers remain outside the subset.
mod support;
#[path = "support/vm.rs"]
mod vm;

use std::{path::Path, process::Command};

use chialisp_oxide_backend_experiment::{FailureCode, FailureKind, Outcome};
use chialisp_oxide_ir::Program;
use support::Fixture;

const SOURCE: &str = r#"
use chialisp_oxide::Tree;
#[chialisp_oxide::puzzle(environment="raw")]
pub fn empty(value:&Tree<'_>)->bool {matches!(value,Tree::Atom([]))}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn classify(value:&Tree<'_>)->u8 {
    match value {Tree::Atom([])=>0,Tree::Atom(_)=>1,Tree::Pair(_,_)=>2}
}
fn slice_empty(bytes:&[u8])->bool {match bytes {[]=>true,_=>false}}
#[chialisp_oxide::puzzle]
pub fn bytes_empty(bytes:&[u8])->bool {slice_empty(bytes)}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn aliases(value:&Tree<'_>)->bool {
    let alias=&value;
    match alias {Tree::Atom(bytes)=>{let bytes=&*bytes; matches!(bytes,[])},Tree::Pair(_,_)=>false}
}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn discarded(value:&Tree<'_>)->u8 {let _=matches!(value,Tree::Atom([]));7}
#[chialisp_oxide::puzzle]
pub fn lazy(choose:bool,bytes:&[u8])->bool {if choose {matches!(bytes,[])} else {false}}
fn atom_checked<'a>(value:&'a Tree<'a>)->&'a [u8] {
    match value {Tree::Atom(bytes)=>bytes,Tree::Pair(_,_)=>panic!()}
}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn discarded_fallible(value:&Tree<'_>)->u8 {let _=slice_empty(atom_checked(value));7}
fn last<'a>(value:&'a Tree<'a>)->&'a Tree<'a> {
    match value {
        Tree::Pair(first,rest)=>match rest {Tree::Atom([])=>first,_=>last(rest)},
        Tree::Atom(_)=>panic!()
    }
}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn last_item<'a>(value:&'a Tree<'a>)->&'a Tree<'a> {last(value)}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn terminal_empty(value:&Tree<'_>)->bool {
    match value {Tree::Pair(_,rest)=>terminal_empty(rest),Tree::Atom(bytes)=>slice_empty(bytes)}
}
#[chialisp_oxide::puzzle]
pub fn snapshot(bytes:&[u8],other:&[u8])->(bool,bool) {
    let mut selected=bytes;
    let before=matches!(selected,[]);
    selected=other;
    (before,matches!(selected,[]))
}
#[chialisp_oxide::puzzle]
pub fn recursive_snapshot(bytes:&[u8],other:&[u8],depth:u8)->bool {
    let mut selected=bytes;
    let before=matches!(selected,[]);
    selected=other;
    if depth>0 {let _=recursive_snapshot(selected,bytes,depth-1);}
    before
}
#[derive(Clone,Copy)] pub enum Reversed<'a> {Branch(&'a Reversed<'a>,&'a Reversed<'a>),Leaf(&'a [u8])}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn structural(value:&Reversed<'_>)->bool {matches!(value,Reversed::Leaf([]))}
#[cfg(test)] mod native {
    use super::*;
    #[test] fn empty_means_zero_bytes_and_recursive_last_returns_the_original_subtree() {
        let nil=Tree::Atom(&[]);let zero=Tree::Atom(&[0]);let padded=Tree::Atom(&[0,0,1]);
        let single=Tree::Pair(&padded,&nil);let list=Tree::Pair(&zero,&single);
        assert!(empty(&nil));assert!(!empty(&zero));assert!(!empty(&padded));assert!(!empty(&list));
        assert_eq!(classify(&nil),0);assert_eq!(classify(&zero),1);assert_eq!(classify(&list),2);
        assert!(bytes_empty(&[]));assert!(!bytes_empty(&[0]));
        assert!(aliases(&nil));assert!(!aliases(&zero));assert!(!aliases(&list));
        assert_eq!(discarded(&list),7);assert_eq!(discarded_fallible(&zero),7);
        assert!(!lazy(false,&[]));assert!(lazy(true,&[]));assert!(!lazy(true,&[0]));
        assert!(std::ptr::eq(last_item(&list),&padded));
        assert!(terminal_empty(&list));assert!(!terminal_empty(&Tree::Pair(&nil,&zero)));
        assert_eq!(snapshot(&[],&[0]),(true,false));assert_eq!(snapshot(&[0],&[]),(false,true));
        assert!(recursive_snapshot(&[],&[0],3));assert!(!recursive_snapshot(&[0],&[],3));
        assert!(structural(&Reversed::Leaf(&[])));assert!(!structural(&Reversed::Leaf(&[0])));
        assert!(std::panic::catch_unwind(||last_item(&nil)).is_err());
        assert!(std::panic::catch_unwind(||{let _=last_item(&Tree::Pair(&nil,&zero));}).is_err());
        assert!(std::panic::catch_unwind(||discarded_fallible(&list)).is_err());
    }
}
"#;

fn success(output: std::process::Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn native(fixture: &Fixture) {
    success(
        Command::new(Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/cargo"))
            .current_dir(&fixture.0)
            .env("RUSTC", fixture.compiler())
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER")
            .env_remove("CHIALISP_OXIDE_CARGO_MODE")
            .args(["test", "--lib", "--offline", "--target-dir"])
            .arg(fixture.0.join("native-build"))
            .output()
            .unwrap(),
    );
}
fn find<'a>(programs: &'a [Program], name: &str) -> &'a Program {
    programs
        .iter()
        .find(|program| program.name.ends_with(&format!("::{name}")))
        .unwrap()
}
fn expected(value: &str) -> Outcome {
    vm::execute(&[1], value, 10_000_000).unwrap().outcome
}

#[test]
fn cargo_native_and_vm_agree_for_empty_atoms_and_recursive_borrowed_last() {
    let fixture = Fixture::new();
    fixture.source(SOURCE);
    native(&fixture);
    success(fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 12);
    for program in &programs {
        program.validate().unwrap();
        assert_eq!(
            program.uses_runtime(),
            matches!(
                program.name.rsplit("::").next().unwrap(),
                "last_item" | "terminal_empty" | "recursive_snapshot"
            )
        );
        if !program.name.ends_with("::discarded") {
            assert!(
                serde_json::to_string(program)
                    .unwrap()
                    .contains("atom_is_empty")
            );
        }
    }
    for optimize in [false, true] {
        for name in [
            "empty",
            "aliases",
            "structural",
            "classify",
            "discarded",
            "discarded_fallible",
            "terminal_empty",
        ] {
            let code = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            for input in [
                "()",
                "0x00",
                "0x0000",
                "0x0001",
                "1",
                "0x80",
                "(1 . 2)",
                "(() . ())",
                "(0x0001 . (0x80))",
            ] {
                let nil = input == "()";
                let pair = input.starts_with('(') && !nil;
                let terminal_nil = nil || input == "(() . ())" || input == "(0x0001 . (0x80))";
                let wanted = match name {
                    "classify" => expected(if nil {
                        "0"
                    } else if pair {
                        "2"
                    } else {
                        "1"
                    }),
                    "discarded" => expected("7"),
                    "discarded_fallible" if pair => Outcome::Rejected,
                    "discarded_fallible" => expected("7"),
                    "terminal_empty" => expected(if terminal_nil { "1" } else { "()" }),
                    _ => expected(if nil { "1" } else { "()" }),
                };
                let result = vm::execute(&code, input, 10_000_000).unwrap();
                assert_eq!(
                    result.outcome, wanted,
                    "{name}, optimize={optimize}, input={input}"
                );
                if result.outcome == Outcome::Rejected {
                    assert_eq!(result.failure.unwrap().code(), FailureCode::Raise);
                }
            }
        }
        let code = chialisp_oxide_ir::compile(find(&programs, "bytes_empty"), optimize).unwrap();
        for (input, wanted) in [("(())", true), ("(0x00)", false), ("(0x0001)", false)] {
            assert_eq!(
                vm::execute(&code, input, 10_000_000).unwrap().outcome,
                expected(if wanted { "1" } else { "()" })
            );
        }
        for bad in ["()", "((1 . 2))", "(() ())"] {
            assert_eq!(
                vm::execute(&code, bad, 10_000_000).unwrap().outcome,
                Outcome::Rejected,
                "strict Atom ABI {bad}"
            );
        }
        let code = chialisp_oxide_ir::compile(find(&programs, "last_item"), optimize).unwrap();
        for (input, wanted) in [
            ("(0x0001)", "0x0001"),
            ("(1 0x0001)", "0x0001"),
            ("((1 . 2))", "(1 . 2)"),
            ("(1 () (0x00 . 0xff))", "(0x00 . 0xff)"),
        ] {
            assert_eq!(
                vm::execute(&code, input, 10_000_000).unwrap().outcome,
                expected(wanted),
                "last {input}"
            );
        }
        for input in ["()", "0x00", "0x0001", "(1 . 0x00)", "(1 2 . 3)"] {
            let result = vm::execute(&code, input, 10_000_000).unwrap();
            assert_eq!(result.outcome, Outcome::Rejected, "last {input}");
            assert_eq!(result.failure.unwrap().code(), FailureCode::Raise);
        }
        let lazy = chialisp_oxide_ir::compile(find(&programs, "lazy"), optimize).unwrap();
        for (input, wanted) in [("(() ())", false), ("(1 ())", true), ("(1 0x00)", false)] {
            assert_eq!(
                vm::execute(&lazy, input, 10_000_000).unwrap().outcome,
                expected(if wanted { "1" } else { "()" })
            );
        }
        let snapshot = chialisp_oxide_ir::compile(find(&programs, "snapshot"), optimize).unwrap();
        for (input, wanted) in [("(() 0x00)", "(1 ())"), ("(0x00 ())", "(() 1)")] {
            assert_eq!(
                vm::execute(&snapshot, input, 10_000_000).unwrap().outcome,
                expected(wanted)
            );
        }
        let snapshot =
            chialisp_oxide_ir::compile(find(&programs, "recursive_snapshot"), optimize).unwrap();
        for (input, wanted) in [("(() 0x00 3)", "1"), ("(0x00 () 3)", "()")] {
            assert_eq!(
                vm::execute(&snapshot, input, 10_000_000).unwrap().outcome,
                expected(wanted)
            );
        }
    }
}

#[test]
fn immutable_fixed_array_coercions_keep_empty_patterns_and_the_exact_byte_abi() {
    let fixture = Fixture::new();
    fixture.source(
        r#"
#[chialisp_oxide::puzzle]
pub fn fixed(bytes:[u8;2])->bool {
    let slice:&[u8]=&bytes;
    matches!(slice,[])
}
#[chialisp_oxide::puzzle]
pub fn zero(bytes:[u8;0])->bool {
    let slice:&[u8]=&bytes;
    matches!(slice,[])
}
#[cfg(test)] mod native {
    use super::*;
    #[test] fn immutable_array_coercion_preserves_its_length() {
        for bytes in [[0,0],[0,1],[0x7f,0x80],[0xff,0xff]] {
            assert!(!fixed(bytes));
        }
        assert!(zero([]));
    }
}
"#,
    );
    native(&fixture);
    success(fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 2);
    for optimize in [false, true] {
        for (name, accepted, malformed, wanted) in [
            (
                "fixed",
                vec!["(0x0000)", "(0x0001)", "(0x7f80)", "(0xffff)"],
                vec![
                    "()",
                    "0x0000",
                    "(())",
                    "(0x00)",
                    "(0x000001)",
                    "((1 . 2))",
                    "(0x0000 ())",
                    "(0x0000 . 0x00)",
                ],
                expected("()"),
            ),
            (
                "zero",
                vec!["(())"],
                vec![
                    "()",
                    "0x00",
                    "(0x00)",
                    "(0x0000)",
                    "((1 . 2))",
                    "(() ())",
                    "(() . 0x00)",
                ],
                expected("1"),
            ),
        ] {
            let code = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            for input in accepted {
                assert_eq!(
                    vm::execute(&code, input, 10_000_000).unwrap().outcome,
                    wanted,
                    "{name}, optimize={optimize}, input={input}"
                );
            }
            // Even a compile-time-known false/true body must not bypass any
            // declared fixed-byte input or exact argument-list validation.
            for input in malformed {
                let result = vm::execute(&code, input, 10_000_000).unwrap();
                assert_eq!(
                    result.outcome,
                    Outcome::Rejected,
                    "fixed-byte ABI {name}, optimize={optimize}, input={input}"
                );
                assert_eq!(result.failure.unwrap().kind(), FailureKind::Program);
            }
        }
    }
}

#[test]
fn lengths_positive_patterns_pointers_and_metadata_escapes_fail_before_any_export() {
    let cases = [
        ("positive_exact", "match bytes {[_]=>true,_=>false}"),
        ("positive_prefix", "match bytes {[_,..]=>true,_=>false}"),
        ("length", "bytes.len()==0"),
        ("length_alias", "{let zero=0;bytes.len()==zero}"),
        ("method", "bytes.is_empty()"),
        ("index", "bytes[0]==0"),
        (
            "pointer",
            "{let _pointer=&raw const *bytes;matches!(bytes,[])}",
        ),
    ];
    for (name, body) in cases {
        let fixture = Fixture::new();
        fixture.source(&format!("#[chialisp_oxide::puzzle]pub fn good(v:bool)->bool{{v}} #[chialisp_oxide::puzzle]pub fn bad(bytes:&[u8])->bool{{{body}}}"));
        success(fixture.cargo(false));
        let output = fixture.cargo(true);
        assert!(!output.status.success(), "unsupported {name} compiled");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("unsupported") || error.contains("empty-slice"),
            "{name}: {error}"
        );
        assert!(fixture.exports().is_empty(), "partial exports for {name}");
    }
    for source in [
        "#[chialisp_oxide::puzzle]pub fn bad(bytes:&[u8])->usize{bytes.len()}",
        "#[chialisp_oxide::puzzle]pub fn bad(mut bytes:[u8;2])->bool{let slice:&mut[u8]=&mut bytes;matches!(slice,[])}",
        "#[chialisp_oxide::puzzle]pub fn bad(bytes:[u16;2])->bool{let slice:&[u16]=&bytes;matches!(slice,[])}",
        "#[chialisp_oxide::puzzle]pub fn bad(bytes:[u8;257])->bool{let slice:&[u8]=&bytes;matches!(slice,[])}",
        "#[chialisp_oxide::puzzle]pub fn bad(bytes:&[u8])->bool{let _=bytes.len();matches!(bytes,[])}",
    ] {
        let fixture = Fixture::new();
        fixture.source(source);
        success(fixture.cargo(false));
        let output = fixture.cargo(true);
        assert!(!output.status.success(), "{source}");
        assert!(fixture.exports().is_empty());
    }
}
