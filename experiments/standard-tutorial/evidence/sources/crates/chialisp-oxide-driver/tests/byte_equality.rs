//! Standard byte equality is recognized through real core trait identities,
//! never through method spelling or erased logical types.
mod support;
#[path = "support/vm.rs"]
mod vm;

use chialisp_oxide_backend_experiment::{FailureCode, FailureKind, Outcome};
use chialisp_oxide_ir::Program;
use std::{path::Path, process::Command};
use support::Fixture;

const SOURCE: &str = r#"
use chialisp_oxide::{Tree,OwnedTree,PuzzleHash};
#[chialisp_oxide::puzzle] pub fn eq(a:&[u8],b:&[u8])->bool {a==b}
#[chialisp_oxide::puzzle] pub fn ne(a:&[u8],b:&[u8])->bool {a!=b}
#[chialisp_oxide::puzzle] pub fn refs(a:&&[u8],b:&&[u8])->bool {a==b}
#[chialisp_oxide::puzzle] pub fn direct_eq(a:&[u8],b:&[u8])->bool {<[u8] as PartialEq<[u8]>>::eq(a,b)}
#[chialisp_oxide::puzzle] pub fn direct_ne(a:&[u8],b:&[u8])->bool {<[u8] as PartialEq<[u8]>>::ne(a,b)}
#[chialisp_oxide::puzzle] pub fn array(a:[u8;2],b:[u8;2])->(bool,bool) {(a==b,a!=b)}
#[chialisp_oxide::puzzle] pub fn array_refs(a:&&[u8;2],b:&&[u8;2])->bool {a==b}
#[chialisp_oxide::puzzle] pub fn empty_arrays(a:[u8;0],b:[u8;0])->bool {a==b}
#[chialisp_oxide::puzzle] pub fn max_arrays(a:[u8;256],b:[u8;256])->bool {a!=b}
#[chialisp_oxide::puzzle] pub fn hash_bytes(a:PuzzleHash,b:PuzzleHash)->bool {a.0==b.0}
fn atom<'a>(value:&'a Tree<'a>)->&'a [u8] {match value {Tree::Atom(b)=>b,Tree::Pair(_,_)=>panic!()}}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn trees(value:&Tree<'_>)->bool {match value {Tree::Pair(a,b)=>atom(a)==atom(b),Tree::Atom(_)=>panic!()}}
fn owned_atom<'a>(value:OwnedTree<'a>)->&'a [u8] {match value {OwnedTree::Atom(b)=>b,OwnedTree::Pair(_,_)=>panic!()}}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn owned(value:OwnedTree<'_>)->bool {match value {OwnedTree::Pair(a,b)=>owned_atom(*a)==owned_atom(*b),OwnedTree::Atom(_)=>panic!()}}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn unused(value:&Tree<'_>)->u8 {let nil:&[u8]=&[];let _=atom(value)==nil;7}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn lazy(value:&Tree<'_>)->bool {match value {Tree::Atom(b)=>b==b,Tree::Pair(_,_)=>true}}
#[chialisp_oxide::puzzle]
pub fn snapshot(a:&[u8],b:&[u8])->(bool,bool) {let mut chosen=a;let before=chosen==b;chosen=b;(before,chosen==b)}
fn descend(depth:u8) {if depth>0 {descend(depth-1)}}
#[chialisp_oxide::puzzle]
pub fn runtime_snapshot(a:&[u8],b:&[u8])->(bool,bool) {let mut chosen=a;let before=chosen==b;chosen=b;descend(2);(before,chosen==b)}
#[chialisp_oxide::puzzle]
pub fn recursive(a:&[u8],b:&[u8],depth:u8)->bool {if depth>0 {recursive(a,b,depth-1)} else {a!=b}}
#[allow(unconditional_recursion)] fn spin<'a>(value:&'a Tree<'a>)->&'a [u8] {spin(value)}
fn fail<'a>(_:&'a Tree<'a>)->&'a [u8] {panic!()}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn reject_then_spin(value:&Tree<'_>)->bool {fail(value)==spin(value)}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn spin_then_reject(value:&Tree<'_>)->bool {spin(value)==fail(value)}
mod core {pub mod cmp {pub fn eq(_: &[u8],_: &[u8])->bool {false} pub fn ne(_: &[u8],_: &[u8])->bool {false}}}
#[chialisp_oxide::puzzle] pub fn fake_eq(a:&[u8],b:&[u8])->bool {core::cmp::eq(a,b)}
#[chialisp_oxide::puzzle] pub fn fake_ne(a:&[u8],b:&[u8])->bool {core::cmp::ne(a,b)}
#[cfg(test)] mod native {
 use super::*;
 #[test] fn ordinary_rust_bytes_snapshots_and_array_values() {
  for (a,b) in [(&[][..],&[][..]),(&[][..],&[0][..]),(&[0][..],&[0,0][..]),(&[1][..],&[0,1][..]),(&[255,128][..],&[255,128][..])] {
   let equal=a.iter().copied().eq(b.iter().copied());
   assert_eq!(eq(a,b),equal);assert_eq!(ne(a,b),!equal);assert_eq!(refs(&a,&b),equal);
   assert_eq!(direct_eq(a,b),equal);assert_eq!(direct_ne(a,b),!equal);
   assert_eq!(snapshot(a,b),(equal,true));assert_eq!(runtime_snapshot(a,b),(equal,true));assert_eq!(recursive(a,b,3),!equal);
   let left=Tree::Atom(a);let right=Tree::Atom(b);assert_eq!(trees(&Tree::Pair(&left,&right)),equal);
   assert_eq!(owned(OwnedTree::Pair(Box::new(OwnedTree::Atom(a)),Box::new(OwnedTree::Atom(b)))),equal);
   assert_eq!(unused(&left),7);assert!(lazy(&Tree::Pair(&left,&right)));
   assert!(!fake_eq(a,b));assert!(!fake_ne(a,b));
  }
  assert_eq!(array([0,1],[0,1]),(true,false));assert_eq!(array([0,1],[1,0]),(false,true));
  assert!(array_refs(&&[0,1],&&[0,1]));assert!(empty_arrays([],[]));assert!(!max_arrays([7;256],[7;256]));
  assert!(hash_bytes(PuzzleHash([42;32]),PuzzleHash([42;32])));
  for size in [63,64,255,256,4096,4097,8192] {
   let a=vec![128;size];let mut b=vec![128;size];assert!(eq(&a,&b));assert!(!ne(&a,&b));
   b[size-1]^=1;assert!(!eq(&a,&b));assert!(ne(&a,&b));
  }
  assert!(std::panic::catch_unwind(||unused(&Tree::Pair(&Tree::Atom(&[]),&Tree::Atom(&[])))).is_err());
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
fn atom(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        "()".into()
    } else {
        format!(
            "0x{}",
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        )
    }
}
fn expected(text: &str) -> Outcome {
    vm::execute(&[1], text, 10_000_000).unwrap().outcome
}
fn bool_outcome(value: bool) -> Outcome {
    expected(if value { "1" } else { "()" })
}

#[test]
fn real_cargo_native_and_vm_agree_on_exact_bytes_and_strict_evaluation() {
    let fixture = Fixture::new();
    let manifest = fixture.0.join("Cargo.toml");
    let contents = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        manifest,
        contents.replace(
            "package=\"chialisp-oxide\",",
            "package=\"chialisp-oxide\",features=[\"alloc\"],",
        ),
    )
    .unwrap();
    fixture.source(SOURCE);
    native(&fixture);
    success(fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 21);
    for program in &programs {
        program.validate().unwrap();
    }
    let mut vectors = vec![
        (vec![], vec![]),
        (vec![], vec![0]),
        (vec![0], vec![0, 0]),
        (vec![1], vec![0, 1]),
        (vec![128, 255], vec![128, 255]),
        (vec![1, 2], vec![1, 3]),
        (vec![1, 2], vec![0, 2]),
    ];
    for size in [63, 64, 255, 256, 4096, 4097, 8192] {
        let a = vec![0x80; size];
        vectors.push((a.clone(), a.clone()));
        let mut b = a.clone();
        b[size - 1] ^= 1;
        vectors.push((a.clone(), b));
        let mut b = a.clone();
        b.push(0);
        vectors.push((a, b));
    }
    for optimize in [false, true] {
        for name in [
            "eq",
            "ne",
            "refs",
            "direct_eq",
            "direct_ne",
            "snapshot",
            "runtime_snapshot",
            "recursive",
            "fake_eq",
            "fake_ne",
        ] {
            let code = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            if name == "eq" {
                for malformed in ["()", "(())", "(() () ())", "(1 . 2)", "((1 . 2) ())"] {
                    let execution = vm::execute(&code, malformed, 10_000_000).unwrap();
                    assert_eq!(
                        execution.outcome,
                        Outcome::Rejected,
                        "malformed strict ABI {malformed}"
                    );
                    assert_eq!(execution.failure.unwrap().kind(), FailureKind::Program);
                }
            }
            for (a, b) in &vectors {
                let equal = a.iter().copied().eq(b.iter().copied());
                let input = if name == "recursive" {
                    format!("({} {} 3)", atom(a), atom(b))
                } else {
                    format!("({} {})", atom(a), atom(b))
                };
                let wanted = match name {
                    "ne" | "direct_ne" | "recursive" => bool_outcome(!equal),
                    "fake_eq" | "fake_ne" => bool_outcome(false),
                    "snapshot" | "runtime_snapshot" => {
                        expected(if equal { "(1 1)" } else { "(() 1)" })
                    }
                    _ => bool_outcome(equal),
                };
                assert_eq!(
                    vm::execute(&code, &input, 10_000_000).unwrap().outcome,
                    wanted,
                    "{name} optimize={optimize} lengths={}/{}",
                    a.len(),
                    b.len()
                );
            }
        }
        for name in ["trees", "owned"] {
            let code = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            for (a, b) in &vectors {
                let input = format!("({} . {})", atom(a), atom(b));
                assert_eq!(
                    vm::execute(&code, &input, 10_000_000).unwrap().outcome,
                    bool_outcome(a == b)
                );
            }
            for input in ["()", "((1 . 2) . ())", "(() . (1 . 2))"] {
                let result = vm::execute(&code, input, 10_000_000).unwrap();
                assert_eq!(result.outcome, Outcome::Rejected);
                assert_eq!(result.failure.unwrap().kind(), FailureKind::Program);
            }
        }
        for (name, input, wanted) in [
            ("array", "(0x0001 0x0001)", "(1 ())"),
            ("array", "(0x0001 0x0100)", "(() 1)"),
            ("array_refs", "(0x0001 0x0001)", "1"),
            ("empty_arrays", "(() ())", "1"),
        ] {
            let code = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            assert_eq!(
                vm::execute(&code, input, 10_000_000).unwrap().outcome,
                expected(wanted)
            );
        }
        for (name, size, wanted) in [("max_arrays", 256, false), ("hash_bytes", 32, true)] {
            let input = format!("({0} {0})", atom(&vec![42; size]));
            let code = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            assert_eq!(
                vm::execute(&code, &input, 10_000_000).unwrap().outcome,
                bool_outcome(wanted)
            );
        }
        let unused = chialisp_oxide_ir::compile(find(&programs, "unused"), optimize).unwrap();
        for input in ["()", "0x00", "0x0000"] {
            assert_eq!(
                vm::execute(&unused, input, 10_000_000).unwrap().outcome,
                expected("7")
            );
        }
        let result = vm::execute(&unused, "(1 . 2)", 10_000_000).unwrap();
        assert_eq!(result.outcome, Outcome::Rejected);
        assert_eq!(result.failure.unwrap().code(), FailureCode::Raise);
        let lazy = chialisp_oxide_ir::compile(find(&programs, "lazy"), optimize).unwrap();
        assert_eq!(
            vm::execute(&lazy, "(1 . 2)", 10_000_000).unwrap().outcome,
            bool_outcome(true)
        );
        for (name, wanted, code) in [
            ("reject_then_spin", Outcome::Rejected, FailureCode::Raise),
            (
                "spin_then_reject",
                Outcome::Inconclusive,
                FailureCode::CostExceeded,
            ),
        ] {
            let bytes = chialisp_oxide_ir::compile(find(&programs, name), optimize).unwrap();
            let execution =
                chialisp_oxide_backend_experiment::execute(&bytes, "()", 100_000).unwrap();
            assert_eq!(execution.outcome, wanted, "{name} optimize={optimize}");
            let failure = execution.failure.unwrap();
            assert_eq!(failure.code(), code);
            assert_eq!(
                failure.kind(),
                if wanted == Outcome::Rejected {
                    FailureKind::Program
                } else {
                    FailureKind::Resource
                }
            );
        }
    }
}

#[test]
fn cargo_dependency_helpers_follow_actual_mir_instead_of_eq_names() {
    let fixture = Fixture::new();
    let sdk = fixture.0.join("counterfeit");
    std::fs::create_dir_all(sdk.join("src")).unwrap();
    std::fs::write(
        sdk.join("Cargo.toml"),
        "[package]\nname=\"chialisp-oxide\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    )
    .unwrap();
    std::fs::write(sdk.join("src/lib.rs"),"pub fn eq(a:&[u8],b:&[u8])->bool {a!=b}\npub fn ne(_: &[u8],_: &[u8])->bool {false}\npub fn real_eq(a:&[u8],b:&[u8])->bool {a==b}\n").unwrap();
    std::fs::write(fixture.0.join("Cargo.toml"),"[package]\nname=\"chialisp-oxide-driver-cargo-fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n[lib]\nname=\"wrapper_fixture\"\n[dependencies]\nbyte_library={package=\"chialisp-oxide\",path=\"counterfeit\"}\n").unwrap();
    fixture.source(r#"#[doc="__oxide_puzzle_v1"] pub fn entry(a:&[u8],b:&[u8])->(bool,bool,bool) {(byte_library::eq(a,b),byte_library::ne(a,b),byte_library::real_eq(a,b))}
    #[cfg(test)] mod tests {#[test] fn actual_helpers(){assert_eq!(super::entry(&[0],&[0]),(false,false,true));assert_eq!(super::entry(&[0],&[0,0]),(true,false,false));}}"#);
    native(&fixture);
    success(fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 1);
    for optimize in [false, true] {
        let code = chialisp_oxide_ir::compile(&programs[0], optimize).unwrap();
        for (input, wanted) in [("(0x00 0x00)", "(() () 1)"), ("(0x00 0x0000)", "(1 () ())")] {
            assert_eq!(
                vm::execute(&code, input, 10_000_000).unwrap().outcome,
                expected(wanted)
            );
        }
    }
}

#[test]
fn unsupported_dispatches_never_inherit_byte_equality_from_erased_types() {
    let cases = [
        (
            "mixed",
            "#[chialisp_oxide::puzzle] pub fn puzzle(a:&[u8],b:&[u8;2])->bool {a==b}",
            "identical Rust operand types",
        ),
        (
            "mutable",
            "#[chialisp_oxide::puzzle] pub fn puzzle(a:&mut [u8],b:&mut [u8])->bool {a==b}",
            "unsupported Rust type",
        ),
        (
            "other_element",
            "#[chialisp_oxide::puzzle] pub fn puzzle(a:&[u16],b:&[u16])->bool {a==b}",
            "unsupported Rust type",
        ),
        (
            "oversized",
            "#[chialisp_oxide::puzzle] pub fn puzzle(a:[u8;257],b:[u8;257])->bool {a==b}",
            "256",
        ),
        (
            "custom",
            "#[repr(transparent)] pub struct Bytes(pub [u8;2]); impl PartialEq for Bytes {fn eq(&self,_:&Self)->bool {false} fn ne(&self,_:&Self)->bool {false}} #[chialisp_oxide::puzzle] pub fn puzzle(a:Bytes,b:Bytes)->bool {a!=b}",
            "custom PartialEq",
        ),
        (
            "derived",
            "#[derive(PartialEq)] #[repr(transparent)] pub struct Bytes(pub [u8;2]); #[chialisp_oxide::puzzle] pub fn puzzle(a:Bytes,b:Bytes)->bool {a==b}",
            "custom PartialEq",
        ),
        (
            "custom_refs",
            "#[repr(transparent)] pub struct Bytes(pub [u8;2]); impl PartialEq for Bytes {fn eq(&self,_:&Self)->bool {false}} #[chialisp_oxide::puzzle] pub fn puzzle(a:Bytes,b:Bytes)->bool {&a==&b}",
            "custom PartialEq",
        ),
        (
            "extra_ref",
            "#[chialisp_oxide::puzzle] pub fn puzzle(a:&&[u8],b:&[u8])->bool {a==b}",
            "PartialEq",
        ),
    ];
    for (name, source, diagnostic) in cases {
        let fixture = Fixture::new();
        fixture.source(source);
        let output = fixture.cargo(true);
        assert!(!output.status.success(), "{name}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(diagnostic), "{name}: {error}");
        assert!(fixture.exports().is_empty(), "{name}");
    }
}
