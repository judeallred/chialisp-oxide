//! Real Cargo/native Rust byte ordering and the separately compiled CLVM result.
mod support;
#[path = "support/vm.rs"]
mod vm;

use chialisp_oxide_backend_experiment::{FailureCode, FailureKind, Outcome};
use chialisp_oxide_ir::Program;
use std::{cmp::Ordering, path::Path, process::Command};
use support::Fixture;

const SOURCE: &str = r#"
use chialisp_oxide::{Tree,OwnedTree,PuzzleHash};
#[chialisp_oxide::puzzle] pub fn lt(a:&[u8],b:&[u8])->bool {a<b}
#[chialisp_oxide::puzzle] pub fn le(a:&[u8],b:&[u8])->bool {a<=b}
#[chialisp_oxide::puzzle] pub fn gt(a:&[u8],b:&[u8])->bool {a>b}
#[chialisp_oxide::puzzle] pub fn ge(a:&[u8],b:&[u8])->bool {a>=b}
#[chialisp_oxide::puzzle] pub fn direct(a:&[u8],b:&[u8])->(bool,bool,bool,bool) {(<[u8] as PartialOrd<[u8]>>::lt(a,b),<[u8] as PartialOrd<[u8]>>::le(a,b),<[u8] as PartialOrd<[u8]>>::gt(a,b),<[u8] as PartialOrd<[u8]>>::ge(a,b))}
#[chialisp_oxide::puzzle] pub fn refs(a:&&[u8],b:&&[u8])->(bool,bool,bool,bool) {(a<b,a<=b,a>b,a>=b)}
#[chialisp_oxide::puzzle] pub fn array(a:[u8;2],b:[u8;2])->(bool,bool,bool,bool) {(a<b,a<=b,a>b,a>=b)}
#[chialisp_oxide::puzzle] pub fn array_refs(a:&&[u8;2],b:&&[u8;2])->(bool,bool,bool,bool) {(a<b,a<=b,a>b,a>=b)}
#[chialisp_oxide::puzzle] pub fn empty_arrays(a:[u8;0],b:[u8;0])->(bool,bool,bool,bool) {(a<b,a<=b,a>b,a>=b)}
#[chialisp_oxide::puzzle] pub fn max_arrays(a:[u8;256],b:[u8;256])->(bool,bool,bool,bool) {(a<b,a<=b,a>b,a>=b)}
#[chialisp_oxide::puzzle] pub fn hash_bytes(a:PuzzleHash,b:PuzzleHash)->(bool,bool,bool,bool) {(a.0<b.0,a.0<=b.0,a.0>b.0,a.0>=b.0)}
fn atom<'a>(value:&'a Tree<'a>)->&'a [u8] {match value {Tree::Atom(b)=>b,Tree::Pair(_,_)=>panic!()}}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn trees(value:&Tree<'_>)->(bool,bool,bool,bool) {match value {Tree::Pair(a,b)=>(atom(a)<atom(b),atom(a)<=atom(b),atom(a)>atom(b),atom(a)>=atom(b)),Tree::Atom(_)=>panic!()}}
fn owned_atom<'a>(value:OwnedTree<'a>)->&'a [u8] {match value {OwnedTree::Atom(b)=>b,OwnedTree::Pair(_,_)=>panic!()}}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn owned(value:OwnedTree<'_>)->bool {match value {OwnedTree::Pair(a,b)=>owned_atom(*a)>=owned_atom(*b),OwnedTree::Atom(_)=>panic!()}}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn unused(value:&Tree<'_>)->u8 {let nil:&[u8]=&[];let _=atom(value)>=nil;7}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn lazy(value:&Tree<'_>)->bool {match value {Tree::Atom(b)=>b<=b,Tree::Pair(_,_)=>true}}
#[chialisp_oxide::puzzle]
pub fn snapshot(a:&[u8],b:&[u8])->(bool,bool) {let mut chosen=a;let before=chosen<b;chosen=b;(before,chosen<=b)}
fn descend(depth:u8) {if depth>0 {descend(depth-1)}}
#[chialisp_oxide::puzzle]
pub fn runtime_snapshot(a:&[u8],b:&[u8])->(bool,bool) {let mut chosen=a;let before=chosen<b;chosen=b;descend(2);(before,chosen<=b)}
#[chialisp_oxide::puzzle]
pub fn recursive(a:&[u8],b:&[u8],depth:u8)->bool {if depth>0 {recursive(a,b,depth-1)} else {a>b}}
#[allow(unconditional_recursion)] fn spin<'a>(value:&'a Tree<'a>)->&'a [u8] {spin(value)}
fn fail<'a>(_:&'a Tree<'a>)->&'a [u8] {panic!()}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn reject_then_spin(value:&Tree<'_>)->bool {fail(value)<spin(value)}
#[chialisp_oxide::puzzle(environment="raw")]
pub fn spin_then_reject(value:&Tree<'_>)->bool {spin(value)>=fail(value)}
mod core {pub mod cmp {pub fn lt(_: &[u8],_: &[u8])->bool {false} pub fn ge(_: &[u8],_: &[u8])->bool {false}}}
#[chialisp_oxide::puzzle] pub fn fake_lt(a:&[u8],b:&[u8])->bool {core::cmp::lt(a,b)}
#[chialisp_oxide::puzzle] pub fn fake_ge(a:&[u8],b:&[u8])->bool {core::cmp::ge(a,b)}
#[cfg(test)] mod native {
 use super::*;
 fn wanted(a:&[u8],b:&[u8])->(bool,bool,bool,bool) {let order=a.iter().cmp(b.iter());(order.is_lt(),order.is_le(),order.is_gt(),order.is_ge())}
 #[test] fn actual_rust_lexical_operators_and_references() {
  for (a,b) in [(&[][..],&[][..]),(&[][..],&[0][..]),(&[0][..],&[0,0][..]),(&[1][..],&[0,1][..]),(&[128][..],&[127][..]),(&[255][..],&[0][..]),(&[255,128][..],&[255,128][..])] {
   let expected=wanted(a,b);assert_eq!((lt(a,b),le(a,b),gt(a,b),ge(a,b)),expected);assert_eq!(refs(&a,&b),expected);assert_eq!(direct(a,b),expected);
   let left=Tree::Atom(a);let right=Tree::Atom(b);assert_eq!(trees(&Tree::Pair(&left,&right)),expected);
   assert_eq!(owned(OwnedTree::Pair(Box::new(OwnedTree::Atom(a)),Box::new(OwnedTree::Atom(b)))),expected.3);
   assert_eq!(snapshot(a,b),(expected.0,true));assert_eq!(runtime_snapshot(a,b),(expected.0,true));assert_eq!(recursive(a,b,3),expected.2);
   assert_eq!(unused(&left),7);assert!(lazy(&Tree::Pair(&left,&right)));assert!(!fake_lt(a,b));assert!(!fake_ge(a,b));
  }
  assert_eq!(array([0,1],[1,0]),(true,true,false,false));assert_eq!(array_refs(&&[0,1],&&[1,0]),(true,true,false,false));
  assert_eq!(empty_arrays([],[]),(false,true,false,true));assert_eq!(max_arrays([128;256],[127;256]),(false,false,true,true));
  assert_eq!(hash_bytes(PuzzleHash([128;32]),PuzzleHash([127;32])),(false,false,true,true));
  for size in [63,64,255,256,4096,4097,8192] {let a=vec![128;size];let mut b=a.clone();b[size-1]=127;assert!(gt(&a,&b));b.push(255);assert!(gt(&a,&b));}
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
fn bool_atom(value: bool) -> &'static str {
    if value { "1" } else { "()" }
}
fn relation(order: Ordering) -> [bool; 4] {
    [order.is_lt(), order.is_le(), order.is_gt(), order.is_ge()]
}
fn tuple(values: &[bool]) -> Outcome {
    expected(&format!(
        "({})",
        values
            .iter()
            .map(|&value| bool_atom(value))
            .collect::<Vec<_>>()
            .join(" ")
    ))
}

#[test]
fn real_cargo_native_and_vm_agree_on_unsigned_lexical_order_and_strictness() {
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
    assert_eq!(programs.len(), 22);
    assert!(find(&programs, "runtime_snapshot").uses_runtime());
    assert!(find(&programs, "recursive").uses_runtime());
    let mut vectors = vec![
        (vec![], vec![]),
        (vec![], vec![0]),
        (vec![0], vec![0, 0]),
        (vec![1], vec![0, 1]),
        (vec![128], vec![127]),
        (vec![255], vec![0]),
        (vec![128, 255], vec![128, 255]),
        (vec![1, 2], vec![1, 3]),
        (vec![1, 2], vec![0, 2]),
        (vec![255], vec![0, 0, 0]),
    ];
    for size in [63, 64, 255, 256, 4096, 4097, 8192] {
        let a = vec![128; size];
        vectors.push((a.clone(), a.clone()));
        let mut b = a.clone();
        b[size - 1] = 127;
        vectors.push((a.clone(), b));
        let mut b = a.clone();
        b[0] = 255;
        vectors.push((a.clone(), b));
        let mut b = a.clone();
        b.push(0);
        vectors.push((a, b));
    }
    for optimize in [false, true] {
        let compiled = programs
            .iter()
            .map(|program| {
                (
                    program.name.rsplit("::").next().unwrap(),
                    chialisp_oxide_ir::compile(program, optimize).unwrap(),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        for (a, b) in &vectors {
            let wanted = relation(a.iter().cmp(b.iter()));
            let left = atom(a);
            let right = atom(b);
            let input = format!("({left} {right})");
            for (name, value) in ["lt", "le", "gt", "ge"].into_iter().zip(wanted) {
                assert_eq!(
                    vm::execute(&compiled[name], &input, 10_000_000)
                        .unwrap()
                        .outcome,
                    expected(bool_atom(value)),
                    "{name} optimize={optimize},a={a:?},b={b:?}"
                );
            }
            for name in ["direct", "refs"] {
                assert_eq!(
                    vm::execute(&compiled[name], &input, 10_000_000)
                        .unwrap()
                        .outcome,
                    tuple(&wanted)
                );
            }
            let raw = format!("({left} . {right})");
            assert_eq!(
                vm::execute(&compiled["trees"], &raw, 10_000_000)
                    .unwrap()
                    .outcome,
                tuple(&wanted)
            );
            assert_eq!(
                vm::execute(&compiled["owned"], &raw, 10_000_000)
                    .unwrap()
                    .outcome,
                expected(bool_atom(wanted[3]))
            );
            for name in ["snapshot", "runtime_snapshot"] {
                assert_eq!(
                    vm::execute(&compiled[name], &input, 10_000_000)
                        .unwrap()
                        .outcome,
                    tuple(&[wanted[0], true])
                );
            }
            assert_eq!(
                vm::execute(
                    &compiled["recursive"],
                    &format!("({left} {right} 3)"),
                    10_000_000
                )
                .unwrap()
                .outcome,
                expected(bool_atom(wanted[2]))
            );
            for name in ["fake_lt", "fake_ge"] {
                assert_eq!(
                    vm::execute(&compiled[name], &input, 10_000_000)
                        .unwrap()
                        .outcome,
                    expected("()")
                );
            }
        }
        for name in ["array", "array_refs"] {
            assert_eq!(
                vm::execute(&compiled[name], "(0x0001 0x0100)", 10_000_000)
                    .unwrap()
                    .outcome,
                tuple(&[true, true, false, false])
            );
            for bad in ["(0x00 0x0100)", "(0x000001 0x0100)", "((1 . 2) 0x0100)"] {
                assert_eq!(
                    vm::execute(&compiled[name], bad, 10_000_000)
                        .unwrap()
                        .outcome,
                    Outcome::Rejected
                );
            }
        }
        assert_eq!(
            vm::execute(&compiled["empty_arrays"], "(() ())", 10_000_000)
                .unwrap()
                .outcome,
            tuple(&[false, true, false, true])
        );
        for (name, size) in [("max_arrays", 256), ("hash_bytes", 32)] {
            let input = format!("({} {})", atom(&vec![128; size]), atom(&vec![127; size]));
            assert_eq!(
                vm::execute(&compiled[name], &input, 10_000_000)
                    .unwrap()
                    .outcome,
                tuple(&[false, false, true, true])
            );
        }
        for bad in [
            "()",
            "(1)",
            "(1 2 3)",
            "(1 . 2)",
            "((1 . 2) 1)",
            "(1 (2 . 3))",
        ] {
            assert_eq!(
                vm::execute(&compiled["lt"], bad, 10_000_000)
                    .unwrap()
                    .outcome,
                Outcome::Rejected
            );
        }
        for atom in ["()", "0x00", "0x0000"] {
            assert_eq!(
                vm::execute(&compiled["unused"], atom, 10_000_000)
                    .unwrap()
                    .outcome,
                expected("7")
            );
        }
        let unused = vm::execute(&compiled["unused"], "(1 . 2)", 10_000_000).unwrap();
        assert_eq!(unused.outcome, Outcome::Rejected);
        assert_eq!(unused.failure.unwrap().code(), FailureCode::Raise);
        assert_eq!(
            vm::execute(&compiled["lazy"], "(1 . 2)", 10_000_000)
                .unwrap()
                .outcome,
            expected("1")
        );
        for (name, wanted, code, kind) in [
            (
                "reject_then_spin",
                Outcome::Rejected,
                FailureCode::Raise,
                FailureKind::Program,
            ),
            (
                "spin_then_reject",
                Outcome::Inconclusive,
                FailureCode::CostExceeded,
                FailureKind::Resource,
            ),
        ] {
            let run =
                chialisp_oxide_backend_experiment::execute(&compiled[name], "()", 100_000).unwrap();
            assert_eq!(run.outcome, wanted, "{name} optimize={optimize}");
            let failure = run.failure.unwrap();
            assert_eq!(failure.code(), code);
            assert_eq!(failure.kind(), kind);
        }
    }
}

#[test]
fn sdk_named_helpers_keep_their_actual_comparison_bodies() {
    let fixture = Fixture::new();
    let sdk = fixture.0.join("counterfeit");
    std::fs::create_dir_all(sdk.join("src")).unwrap();
    std::fs::write(
        sdk.join("Cargo.toml"),
        "[package]\nname=\"chialisp-oxide\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    )
    .unwrap();
    std::fs::write(sdk.join("src/lib.rs"),"pub fn lt(_: &[u8],_: &[u8])->bool {false}\npub fn ge(a:&[u8],b:&[u8])->bool {a<b}\npub fn real_le(a:&[u8],b:&[u8])->bool {a<=b}\n").unwrap();
    std::fs::write(fixture.0.join("Cargo.toml"),"[package]\nname=\"chialisp-oxide-driver-cargo-fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n[lib]\nname=\"wrapper_fixture\"\n[dependencies]\nbyte_library={package=\"chialisp-oxide\",path=\"counterfeit\"}\n").unwrap();
    fixture.source(r#"#[doc="__oxide_puzzle_v1"] pub fn entry(a:&[u8],b:&[u8])->(bool,bool,bool) {(byte_library::lt(a,b),byte_library::ge(a,b),byte_library::real_le(a,b))}
    #[cfg(test)] mod tests {#[test] fn bodies(){assert_eq!(super::entry(&[0],&[0]),(false,false,true));assert_eq!(super::entry(&[0],&[0,0]),(false,true,true));}}"#);
    native(&fixture);
    success(fixture.cargo(true));
    let programs = fixture.exports();
    assert_eq!(programs.len(), 1);
    for optimize in [false, true] {
        let code = chialisp_oxide_ir::compile(&programs[0], optimize).unwrap();
        for (input, wanted) in [("(0x00 0x00)", "(() () 1)"), ("(0x00 0x0000)", "(() 1 1)")] {
            assert_eq!(
                vm::execute(&code, input, 10_000_000).unwrap().outcome,
                expected(wanted)
            );
        }
    }
}

#[test]
fn custom_partial_ordering_and_nonbyte_types_never_inherit_lexical_semantics() {
    let custom = r#"#[repr(transparent)] pub struct Bytes(pub [u8;2]);
    impl PartialEq for Bytes {fn eq(&self,_:&Self)->bool {true}}
    impl PartialOrd for Bytes {fn partial_cmp(&self,_:&Self)->Option<std::cmp::Ordering> {None} fn lt(&self,_:&Self)->bool {true} fn le(&self,_:&Self)->bool {false} fn gt(&self,_:&Self)->bool {true} fn ge(&self,_:&Self)->bool {false}}
    "#;
    let cases=[
        ("custom_lt",format!("{custom} #[chialisp_oxide::puzzle] pub fn puzzle(a:Bytes,b:Bytes)->bool {{a<b}}"),"custom PartialOrd"),
        ("custom_le",format!("{custom} #[chialisp_oxide::puzzle] pub fn puzzle(a:Bytes,b:Bytes)->bool {{a<=b}}"),"custom PartialOrd"),
        ("custom_gt",format!("{custom} #[chialisp_oxide::puzzle] pub fn puzzle(a:Bytes,b:Bytes)->bool {{a>b}}"),"custom PartialOrd"),
        ("custom_ge",format!("{custom} #[chialisp_oxide::puzzle] pub fn puzzle(a:Bytes,b:Bytes)->bool {{a>=b}}"),"custom PartialOrd"),
        ("custom_refs",format!("{custom} #[chialisp_oxide::puzzle] pub fn puzzle(a:Bytes,b:Bytes)->bool {{&a<&b}}"),"custom PartialOrd"),
        ("derived","#[derive(PartialEq,PartialOrd)] #[repr(transparent)] pub struct Bytes(pub [u8;2]); #[chialisp_oxide::puzzle] pub fn puzzle(a:Bytes,b:Bytes)->bool {a<b}".into(),"custom PartialOrd"),
        ("asymmetric","#[repr(transparent)] pub struct Bytes(pub [u8;2]); impl PartialEq<Bytes> for [u8] {fn eq(&self,_:&Bytes)->bool {true}} impl PartialOrd<Bytes> for [u8] {fn partial_cmp(&self,_:&Bytes)->Option<std::cmp::Ordering>{None}} #[chialisp_oxide::puzzle] pub fn puzzle(a:&[u8],b:Bytes)->bool {*a<b}".into(),"custom PartialOrd"),
        ("mutable","#[chialisp_oxide::puzzle] pub fn puzzle(a:&mut [u8],b:&mut [u8])->bool {a<b}".into(),"unsupported Rust type"),
        ("other_element","#[chialisp_oxide::puzzle] pub fn puzzle(a:&[u16],b:&[u16])->bool {a<b}".into(),"unsupported Rust type"),
        ("boolean_array","#[chialisp_oxide::puzzle] pub fn puzzle(a:[bool;2],b:[bool;2])->bool {a<b}".into(),"other elements"),
        ("oversized","#[chialisp_oxide::puzzle] pub fn puzzle(a:[u8;257],b:[u8;257])->bool {a<b}".into(),"256"),
        ("partial_cmp","#[chialisp_oxide::puzzle] pub fn puzzle(a:&[u8],b:&[u8])->bool {a.partial_cmp(b).is_some()}".into(),"generic call"),
    ];
    for (name, source, diagnostic) in cases {
        let fixture = Fixture::new();
        fixture.source(&source);
        success(fixture.cargo(false));
        let output = fixture.cargo(true);
        assert!(!output.status.success(), "{name}");
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(errors.contains(diagnostic), "{name}: {errors}");
        assert!(
            !errors.contains("internal compiler error"),
            "{name}: {errors}"
        );
        assert!(fixture.exports().is_empty(), "{name}");
    }
}
