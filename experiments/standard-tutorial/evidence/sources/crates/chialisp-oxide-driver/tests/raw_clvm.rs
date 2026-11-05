//! Actual Rust helper bodies constructing fixed programs, through reviewed apply.
mod support;
#[path = "support/vm.rs"]
mod vm;
use chialisp_oxide_backend_experiment::Outcome;
use std::{path::Path, process::Command};
use support::Fixture;

const SOURCE: &str = r#"
use chialisp_oxide::{Tree,raw_clvm as raw};
#[chialisp_oxide::puzzle] pub fn length<'a>(v:&'a Tree<'a>)->Tree<'static>{raw::byte_length(v)}
#[chialisp_oxide::puzzle] pub fn concat<'a>(a:&'a Tree<'a>,b:&'a Tree<'a>)->Tree<'static>{raw::byte_concat(a,b)}
#[chialisp_oxide::puzzle] pub fn substr<'a>(a:&'a Tree<'a>,b:&'a Tree<'a>,c:&'a Tree<'a>)->Tree<'static>{raw::byte_substr(a,b,c)}
#[chialisp_oxide::puzzle] pub fn divide<'a>(a:&'a Tree<'a>,b:&'a Tree<'a>)->Tree<'static>{raw::div(a,b)}
#[chialisp_oxide::puzzle] pub fn divide_pair<'a>(a:&'a Tree<'a>,b:&'a Tree<'a>)->Tree<'static>{raw::divmod(a,b)}
#[chialisp_oxide::puzzle] pub fn modulo<'a>(a:&'a Tree<'a>,b:&'a Tree<'a>)->Tree<'static>{raw::modulo(a,b)}
#[chialisp_oxide::puzzle] pub fn negate<'a>(v:&'a Tree<'a>)->Tree<'static>{raw::not(v)}
#[chialisp_oxide::puzzle] pub fn any<'a>(a:&'a Tree<'a>,b:&'a Tree<'a>)->Tree<'static>{raw::any2(a,b)}
#[chialisp_oxide::puzzle] pub fn all<'a>(a:&'a Tree<'a>,b:&'a Tree<'a>)->Tree<'static>{raw::all2(a,b)}
#[chialisp_oxide::puzzle] pub fn unused<'a>(a:&'a Tree<'a>,b:&'a Tree<'a>)->u8{let _=raw::byte_concat(a,b);7}
#[chialisp_oxide::puzzle] pub fn lazy<'a>(flag:bool,a:&'a Tree<'a>,b:&'a Tree<'a>)->Tree<'static>{if flag{raw::byte_concat(a,b)}else{Tree::Atom(&[7])}}
fn byte_concat<'a>(a:&'a Tree<'a>,_:&'a Tree<'a>)->Tree<'a>{*a}
#[chialisp_oxide::puzzle] pub fn fake<'a>(a:&'a Tree<'a>,b:&'a Tree<'a>)->Tree<'a>{byte_concat(a,b)}
#[chialisp_oxide::puzzle] pub fn add<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>)->Tree<'static>{raw::add(left, right)}
#[chialisp_oxide::puzzle] pub fn sub<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>)->Tree<'static>{raw::sub(left, right)}
#[chialisp_oxide::puzzle] pub fn mul<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>)->Tree<'static>{raw::mul(left, right)}
#[chialisp_oxide::puzzle] pub fn bitand<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>)->Tree<'static>{raw::bitand(left, right)}
#[chialisp_oxide::puzzle] pub fn bitor<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>)->Tree<'static>{raw::bitor(left, right)}
#[chialisp_oxide::puzzle] pub fn bitxor<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>)->Tree<'static>{raw::bitxor(left, right)}
#[chialisp_oxide::puzzle] pub fn lognot<'a>(value: &'a Tree<'a>)->Tree<'static>{raw::lognot(value)}
#[chialisp_oxide::puzzle] pub fn arithmetic_shift<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>)->Tree<'static>{raw::arithmetic_shift(left, right)}
#[chialisp_oxide::puzzle] pub fn logical_shift<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>)->Tree<'static>{raw::logical_shift(left, right)}
#[chialisp_oxide::puzzle] pub fn atom_equal<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>)->Tree<'static>{raw::atom_equal(left, right)}
#[chialisp_oxide::puzzle] pub fn atom_greater<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>)->Tree<'static>{raw::atom_greater(left, right)}
#[chialisp_oxide::puzzle] pub fn integer_greater<'a>(left: &'a Tree<'a>, right: &'a Tree<'a>)->Tree<'static>{raw::integer_greater(left, right)}
#[chialisp_oxide::puzzle] pub fn coin_id<'a>(parent: &'a Tree<'a>, puzzle_hash: &'a Tree<'a>, amount: &'a Tree<'a>)->Tree<'static>{raw::coin_id(parent, puzzle_hash, amount)}
#[chialisp_oxide::puzzle] pub fn strict_if3<'a>(condition: &'a Tree<'a>, then_value: &'a Tree<'a>, else_value: &'a Tree<'a>)->Tree<'static>{raw::strict_if3(condition, then_value, else_value)}
#[chialisp_oxide::puzzle] pub fn strict_branch<'a>(a:&'a Tree<'a>,b:&'a Tree<'a>)->Tree<'static>{raw::strict_if3(&Tree::Atom(&[]),&raw::div(a,b),&Tree::Atom(&[7]))}
#[cfg(test)]mod tests{use super::*;#[test]fn actual_vm_oracle(){let a=Tree::Atom(&[0,128,255]);let b=Tree::Atom(&[0]);assert_eq!(concat(&a,&b),Tree::Atom(&[0,128,255,0]));assert_eq!(length(&a),Tree::Atom(&[3]));assert_eq!(substr(&a,&Tree::Atom(&[1]),&Tree::Atom(&[3])),Tree::Atom(&[128,255]));assert_eq!(divide(&Tree::Atom(&[249]),&Tree::Atom(&[3])),Tree::Atom(&[253]));assert_eq!(fake(&a,&b),a);assert_eq!(lazy(false,&a,&Tree::Pair(&a,&b)),Tree::Atom(&[7]));}}
"#;

#[test]
fn fixed_operator_helpers_lower_real_bodies_and_match_independent_chialisp() {
    let fixture = Fixture::new();
    fixture.source(SOURCE);
    let manifest = fixture.0.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    let dependency = text
        .lines()
        .find(|line| line.starts_with("chialisp-oxide="))
        .unwrap()
        .replace('}', ",features=[\"vm\"]}");
    std::fs::write(
        &manifest,
        format!("{text}[dev-dependencies]\n{dependency}\n"),
    )
    .unwrap();
    let output = Command::new(Path::new(env!("CHIALISP_OXIDE_SYSROOT")).join("bin/cargo"))
        .current_dir(&fixture.0)
        .env("RUSTC", fixture.compiler())
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CHIALISP_OXIDE_CARGO_MODE")
        .args(["test", "--lib", "--offline", "--target-dir"])
        .arg(fixture.0.join("native-build"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = fixture.cargo(true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let programs = fixture.exports();
    assert_eq!(programs.len(), 27);
    let find = |name: &str| {
        programs
            .iter()
            .find(|p| p.name.ends_with(&format!("::{name}")))
            .unwrap()
    };
    assert!(!find("fake").uses_apply());
    let cases: &[(&str, &str, &[&str])] = &[
        (
            "length",
            "(mod (V) (strlen V))",
            &["(())", "(0x00)", "(0x0080ff)", "((1 . 2))"],
        ),
        (
            "concat",
            "(mod (A B) (concat A B))",
            &["(() ())", "(0x0080ff 0x00)", "((1 . 2) ())", "(() (1 . 2))"],
        ),
        (
            "substr",
            "(mod (A B C) (substr A B C))",
            &[
                "(0x0080ff 1 3)",
                "(0x0080ff 3 3)",
                "(0x0080ff 3 2)",
                "(0x0080ff -1 2)",
                "(0x0080ff 0 4)",
                "(() 0 0)",
                "(0x0080ff 0x0001 0x0003)",
            ],
        ),
        (
            "divide",
            "(mod (A B) (/ A B))",
            &[
                "(-7 3)",
                "(7 -3)",
                "(0x0007 3)",
                "(0x0100000000000000000000000000000000 1)",
                "(1 ())",
            ],
        ),
        (
            "divide_pair",
            "(mod (A B) (divmod A B))",
            &["(-7 3)", "(7 -3)", "(1 ())", "((1) 3)"],
        ),
        (
            "modulo",
            "(mod (A B) (% A B))",
            &["(-7 3)", "(7 -3)", "(1 ())"],
        ),
        (
            "negate",
            "(mod (V) (not V))",
            &["(())", "(0x00)", "((1 . 2))"],
        ),
        (
            "any",
            "(mod (A B) (any A B))",
            &["(() ())", "(() 0x00)", "((1 . 2) ())"],
        ),
        (
            "all",
            "(mod (A B) (all A B))",
            &["(() ())", "(1 0x00)", "((1 . 2) 1)"],
        ),
        (
            "add",
            "(mod (A B) (+ A B))",
            &[
                "(0x0100000000000000000000000000000000 1)",
                "(-7 3)",
                "(0x0001 2)",
                "((1) 2)",
            ],
        ),
        (
            "sub",
            "(mod (A B) (- A B))",
            &[
                "(0x0100000000000000000000000000000000 -1)",
                "(-7 3)",
                "(() 1)",
            ],
        ),
        (
            "mul",
            "(mod (A B) (* A B))",
            &[
                "(0x0100000000000000000000000000000000 2)",
                "(-7 3)",
                "(0x0001 2)",
            ],
        ),
        (
            "bitand",
            "(mod (A B) (logand A B))",
            &[
                "(0x0100000000000000000000000000000000 -1)",
                "(-1 0x0080)",
                "((1) 2)",
            ],
        ),
        (
            "bitor",
            "(mod (A B) (logior A B))",
            &[
                "(0x0100000000000000000000000000000000 1)",
                "(-1 1)",
                "(0x0001 2)",
            ],
        ),
        (
            "bitxor",
            "(mod (A B) (logxor A B))",
            &[
                "(0x0100000000000000000000000000000000 1)",
                "(-1 1)",
                "(() ())",
            ],
        ),
        (
            "arithmetic_shift",
            "(mod (A B) (ash A B))",
            &[
                "(0x0100000000000000000000000000000000 1)",
                "(-1 -1)",
                "(1 65535)",
                "(1 65536)",
                "(1 -65535)",
                "(1 -65536)",
                "(1 2147483648)",
                "((1) 2)",
            ],
        ),
        (
            "logical_shift",
            "(mod (A B) (lsh A B))",
            &[
                "(0x0100000000000000000000000000000000 1)",
                "(0xff -1)",
                "(0x00ff 0)",
                "(1 65536)",
                "(1 -65536)",
            ],
        ),
        (
            "atom_equal",
            "(mod (A B) (= A B))",
            &["(() 0x00)", "(0x0001 1)", "(0x80 0x80)", "((1) 1)"],
        ),
        (
            "atom_greater",
            "(mod (A B) (>s A B))",
            &["(0x80 0x7f)", "(0x0001 1)", "(() 0x00)", "((1) 1)"],
        ),
        (
            "integer_greater",
            "(mod (A B) (> A B))",
            &[
                "(0x0100000000000000000000000000000000 1)",
                "(-1 1)",
                "(0x0001 1)",
                "((1) 1)",
            ],
        ),
        (
            "lognot",
            "(mod (V) (lognot V))",
            &["(-1)", "(())", "(0x0001)", "((1))"],
        ),
        (
            "strict_if3",
            "(mod (A B C) (i A B C))",
            &["(() 1 2)", "(0x00 1 2)", "((1) (2 . 3) 4)"],
        ),
        (
            "coin_id",
            "(mod (A B C) (coinid A B C))",
            &[
                "(0x0000000000000000000000000000000000000000000000000000000000000000 0x0000000000000000000000000000000000000000000000000000000000000000 ())",
                "(0x0000000000000000000000000000000000000000000000000000000000000000 0x0000000000000000000000000000000000000000000000000000000000000000 128)",
                "(0x0000000000000000000000000000000000000000000000000000000000000000 0x0000000000000000000000000000000000000000000000000000000000000000 18446744073709551615)",
                "(0x0000000000000000000000000000000000000000000000000000000000000000 0x0000000000000000000000000000000000000000000000000000000000000000 -1)",
                "(0x0000000000000000000000000000000000000000000000000000000000000000 0x0000000000000000000000000000000000000000000000000000000000000000 0x00)",
                "(0x0000000000000000000000000000000000000000000000000000000000000000 0x0000000000000000000000000000000000000000000000000000000000000000 0x0001)",
                "(0x0000000000000000000000000000000000000000000000000000000000000000 0x0000000000000000000000000000000000000000000000000000000000000000 18446744073709551616)",
                "(0x00 0x0000000000000000000000000000000000000000000000000000000000000000 1)",
                "(0x0000000000000000000000000000000000000000000000000000000000000000 (1) 1)",
            ],
        ),
    ];
    for optimize in [false, true] {
        for (name, source, solutions) in cases {
            let program = find(name);
            assert!(program.uses_apply());
            let rust = chialisp_oxide_ir::compile(program, optimize).unwrap();
            let reference = chialisp_oxide_reference::compile(source).unwrap();
            for solution in *solutions {
                let actual = vm::execute(&rust, solution, 10_000_000).unwrap();
                let expected = vm::execute(&reference, solution, 10_000_000).unwrap();
                assert_eq!(
                    actual.outcome, expected.outcome,
                    "{name}: {solution}; optimize={optimize}"
                );
                if actual.outcome == Outcome::Rejected {
                    assert_eq!(
                        actual.failure.as_ref().map(|f| f.code()),
                        expected.failure.as_ref().map(|f| f.code())
                    );
                }
            }
        }
        let run = |name: &str, solution: &str| {
            vm::execute(
                &chialisp_oxide_ir::compile(find(name), optimize).unwrap(),
                solution,
                10_000_000,
            )
            .unwrap()
            .outcome
        };
        assert_eq!(run("unused", "(() (1 . 2))"), Outcome::Rejected);
        assert_eq!(
            run("lazy", "(() () (1 . 2))"),
            Outcome::Accepted {
                output_hex: "07".into()
            }
        );
        assert_eq!(run("lazy", "(1 () (1 . 2))"), Outcome::Rejected);
        assert_eq!(
            run("fake", "(0x00 (1 . 2))"),
            Outcome::Accepted {
                output_hex: "00".into()
            }
        );
        assert_eq!(run("strict_branch", "(1 ())"), Outcome::Rejected);
        assert_eq!(
            run("strict_branch", "(1 1)"),
            Outcome::Accepted {
                output_hex: "07".into()
            }
        );
        for bad in ["()", "(1)", "(1 2 3)", "(1 . 2)"] {
            assert_eq!(run("concat", bad), Outcome::Rejected);
        }
    }
}
