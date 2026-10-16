use std::{
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use chialisp_oxide_backend_experiment::{Outcome, execute};

#[path = "fixtures/sdk_readable.rs"]
mod native;

struct Source(PathBuf);
static NEXT_SOURCE: AtomicUsize = AtomicUsize::new(0);
impl Source {
    fn new(source: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "chialisp-oxide-constant-{}-{}-{}.rs",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_SOURCE.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::write(&path, source).unwrap();
        Self(path)
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn compile(path: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_chialisp-oxide-driver"))
        .arg(path)
        .output()
        .unwrap()
}
fn extract(path: &std::path::Path) -> chialisp_oxide_ir::Program {
    let output = compile(path);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn named_domain_values_and_methods_preserve_exact_code_and_native_conditions() {
    let old = extract(&fixture("sdk_readable_baseline.rs"));
    let new = extract(&fixture("sdk_readable.rs"));
    let native = native::puzzle(
        native::chialisp_oxide::Amount::from_mojos(10),
        native::chialisp_oxide::Timestamp::from_unix_seconds(100),
        native::chialisp_oxide::Message32([3; 32]),
    );
    assert_eq!(native.0.0, 50);
    assert_eq!(native.0.1.0, [1; 48]);
    assert_eq!(native.0.2.0, [3; 32]);
    assert_eq!((native.1.0, native.1.1.unix_seconds()), (81, 100));
    assert_eq!((native.2.0, native.2.1.mojos()), (73, 1000));
    assert_eq!(
        (native.3.0, native.3.1.0, native.3.2.mojos()),
        (51, [42; 32], 10)
    );
    let expected = chialisp_oxide_reference::compile(&format!(
        "(mod () (q . ((50 0x{} 0x{}) (81 100) (73 1000) (51 0x{} 10))))",
        "01".repeat(48),
        "03".repeat(32),
        "2a".repeat(32)
    ))
    .unwrap();
    let expected = execute(&expected, "()", 1_000_000).unwrap().outcome;
    for optimize in [false, true] {
        let old_bytes = chialisp_oxide_ir::compile(&old, optimize).unwrap();
        let new_bytes = chialisp_oxide_ir::compile(&new, optimize).unwrap();
        assert!(
            old_bytes == new_bytes,
            "readability changed CLVM bytes (optimize={optimize}, old={} bytes, new={} bytes)",
            old_bytes.len(),
            new_bytes.len()
        );
        assert_eq!(
            execute(
                &new_bytes,
                &format!("(10 100 0x{})", "03".repeat(32)),
                1_000_000
            )
            .unwrap()
            .outcome,
            expected
        );
        for solution in [
            format!("(1001 100 0x{})", "03".repeat(32)),
            format!("(10 99 0x{})", "03".repeat(32)),
        ] {
            assert_eq!(
                execute(&new_bytes, &solution, 1_000_000).unwrap().outcome,
                Outcome::Rejected
            );
        }
    }
}

#[test]
fn byte_constants_match_inline_bytes_at_all_supported_size_boundaries() {
    for len in [0, 1, 32, 48, 256] {
        let constant = Source::new(&format!(
            "const DATA:[u8;{len}]=[0x80;{len}]; pub fn puzzle()->[u8;{len}] {{ DATA }}"
        ));
        let inline = Source::new(&format!("pub fn puzzle()->[u8;{len}] {{ [0x80;{len}] }}"));
        let constant = extract(&constant.0);
        let inline = extract(&inline.0);
        for optimize in [false, true] {
            let bytes = chialisp_oxide_ir::compile(&constant, optimize).unwrap();
            assert_eq!(
                bytes,
                chialisp_oxide_ir::compile(&inline, optimize).unwrap()
            );
            let atom = if len == 0 {
                "()".into()
            } else {
                format!("0x{}", "80".repeat(len))
            };
            let expected =
                chialisp_oxide_reference::compile(&format!("(mod () (q . {atom}))")).unwrap();
            assert_eq!(
                execute(&bytes, "()", 1_000_000).unwrap().outcome,
                execute(&expected, "()", 1_000_000).unwrap().outcome
            );
        }
    }
}

#[test]
fn nested_transparent_byte_constants_preserve_only_the_inner_bytes() {
    let source = Source::new(
        "#[repr(transparent)] pub struct Inner(pub [u8;32]); #[repr(transparent)] pub struct Outer(pub Inner); const KEY:Outer=Outer(Inner([0xff;32])); pub fn puzzle()->Outer { KEY }",
    );
    let program = extract(&source.0);
    assert_eq!(program.result, chialisp_oxide_ir::Type::Bytes { len: 32 });
    for optimize in [false, true] {
        let bytes = chialisp_oxide_ir::compile(&program, optimize).unwrap();
        let expected =
            chialisp_oxide_reference::compile(&format!("(mod () (q . 0x{}))", "ff".repeat(32)))
                .unwrap();
        assert_eq!(
            execute(&bytes, "()", 1_000_000).unwrap().outcome,
            execute(&expected, "()", 1_000_000).unwrap().outcome
        );
    }
}

#[test]
fn constants_do_not_enable_general_references_aggregate_layouts_or_unsafe_memory() {
    for (source, diagnostic) in [
        (
            "const DATA:(u64,u64)=(1,2); pub fn puzzle()->(u64,u64) { DATA }",
            "only scalar compile-time constants",
        ),
        (
            "const DATA:&u64=&7; pub fn puzzle()->&'static u64 { DATA }",
            "unsupported Rust type",
        ),
        (
            "const DATA:&(u64,u64)=&(1,2); pub fn puzzle()->&'static (u64,u64) { DATA }",
            "unsupported Rust type",
        ),
        (
            "const DATA:[u8;257]=[0;257]; pub fn puzzle()->[u8;257] { DATA }",
            "limited to 256",
        ),
        (
            "const DATA:[u8;32]=[0;31]; pub fn puzzle()->[u8;32] { DATA }",
            "E0308",
        ),
        (
            "const DATA:[u8;32]=unsafe { core::mem::MaybeUninit::uninit().assume_init() }; pub fn puzzle()->[u8;32] { DATA }",
            "unsafe",
        ),
        (
            "const DATA:*const u8 = b\"x\".as_ptr(); pub fn puzzle()->*const u8 { DATA }",
            "unsupported Rust type",
        ),
    ] {
        let source = Source::new(source);
        let output = compile(&source.0);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(errors.contains(diagnostic), "{errors}");
        assert!(!errors.contains("internal compiler error"), "{errors}");
    }
}

#[test]
fn immutable_byte_reference_constants_preserve_exact_bytes_without_pointer_layouts() {
    for (source, ty, value) in [
        (
            "const DATA:&[u8]=&[0,0,1,255]; pub fn puzzle()->&'static [u8] { DATA }",
            chialisp_oxide_ir::Type::Atom,
            "0x000001ff".to_owned(),
        ),
        (
            "const DATA:&[u8;32]=&[0;32]; pub fn puzzle()->&'static [u8;32] { DATA }",
            chialisp_oxide_ir::Type::Bytes { len: 32 },
            format!("0x{}", "00".repeat(32)),
        ),
    ] {
        let source = Source::new(source);
        let program = extract(&source.0);
        assert_eq!(program.result, ty);
        let reference =
            chialisp_oxide_reference::compile(&format!("(mod () (q . {value}))")).unwrap();
        for optimize in [false, true] {
            let bytes = chialisp_oxide_ir::compile(&program, optimize).unwrap();
            assert_eq!(
                execute(&bytes, "()", 1_000_000).unwrap().outcome,
                execute(&reference, "()", 1_000_000).unwrap().outcome
            );
            assert_eq!(
                execute(&bytes, "(1)", 1_000_000).unwrap().outcome,
                Outcome::Rejected
            );
        }
    }
}

#[test]
fn total_helpers_preserve_argument_failures_projection_strictness_and_lazy_branches() {
    let add_one = "fn add_one(value:u64)->u64 { value+1 } fn ignore(_:u64)->u64 {7}";
    let cases: Vec<(String,Vec<(&str,bool)>)> = vec![
        (format!("{add_one} pub fn puzzle(value:u64)->u64 {{ ignore(add_one(value)) }}"),vec![("(0)",true),("(18446744073709551615)",false)]),
        (format!("{add_one} fn first(pair:(u64,u64))->u64 {{ pair.0 }} pub fn puzzle(value:u64)->u64 {{ first((7,add_one(value))) }}"),vec![("(0)",true),("(18446744073709551615)",false)]),
        ("fn ignore(_: (u64,bool))->u64 {7} pub fn puzzle(pair:(u64,bool))->u64 { ignore(pair) }".into(),vec![("((3 1))",true),("((3 2))",false),("((3))",false),("((3 1 2))",false)]),
        ("fn first([byte,_]:[u8;2])->u8 { byte } fn ignore(_:u8)->u64 {7} pub fn puzzle(bytes:[u8;2])->u64 { ignore(first(bytes)) }".into(),vec![("(0x0102)",true),("(0x01)",false),("((1 2))",false)]),
        (format!("{add_one} pub fn puzzle(value:u64,choose:bool)->u64 {{ if choose {{ ignore(add_one(value)) }} else {{7}} }}"),vec![("(0 1)",true),("(18446744073709551615 ())",true),("(18446744073709551615 1)",false),("(0 2)",false)]),
        ("fn check(value:u64) { assert!(value<=10); } pub fn puzzle(value:u64)->u64 { check(value); 7 }".into(),vec![("(10)",true),("(11)",false)]),
        (format!("{add_one} pub fn puzzle(value:u64)->u64 {{ let computed=value+1; let copied=computed; let packed=(3,copied); ignore(packed.1); 7 }}"),vec![("(0)",true),("(18446744073709551615)",false)]),
        (format!("{add_one} fn pack(value:u64)->(u64,u64) {{(7,value)}} fn first(pair:(u64,u64))->u64 {{pair.0}} fn outer(value:u64)->u64 {{first(pack(add_one(value)))}} pub fn puzzle(value:u64)->u64 {{outer(value)}}"),vec![("(0)",true),("(18446744073709551615)",false)]),
        ("fn validate(value:u64) { assert!(value<=10); } fn wrapper(value:u64)->u64 { validate(value); 7 } pub fn puzzle(value:u64,choose:bool)->u64 { if choose { let _=wrapper(value); } 7 }".into(),vec![("(10 1)",true),("(11 1)",false),("(11 ())",true)]),
    ];
    for (source, cases) in cases {
        let source = Source::new(&source);
        let program = extract(&source.0);
        for optimize in [false, true] {
            let bytes = chialisp_oxide_ir::compile(&program, optimize).unwrap();
            for (solution, accepted) in &cases {
                let result = execute(&bytes, solution, 10_000_000).unwrap();
                let expected = if *accepted {
                    Outcome::Accepted {
                        output_hex: "07".into(),
                    }
                } else {
                    Outcome::Rejected
                };
                assert_eq!(
                    result.outcome, expected,
                    "optimize={optimize}, solution={solution}: {result:?}"
                );
            }
        }
    }
}

#[test]
fn typed_split_with_checked_subtraction_has_the_same_code_as_raw_rust() {
    let raw = Source::new(
        "pub fn puzzle(first:u64)->((u64,u64),(u64,[u8;32],u64),(u64,[u8;32],u64)) { assert!(first<=1000); let second=1000-first; ((73,1000),(51,[42;32],first),(51,[43;32],second)) }",
    );
    let sdk = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../sdk/chialisp_oxide.rs")
        .canonicalize()
        .unwrap();
    let readable = Source::new(&format!(
        r#"
        #[path={}] pub mod chialisp_oxide;
        use chialisp_oxide::{{Amount, AssertMyAmount, CreateCoin, PuzzleHash, assert_my_amount, create_coin}};
        const TOTAL:Amount=Amount::from_mojos(1000);
        const FIRST_RECIPIENT:PuzzleHash=PuzzleHash([42;32]);
        const SECOND_RECIPIENT:PuzzleHash=PuzzleHash([43;32]);
        pub fn puzzle(first:Amount)->(AssertMyAmount,CreateCoin,CreateCoin) {{
            assert!(first.is_at_most(TOTAL));
            let second=Amount::from_mojos(TOTAL.mojos()-first.mojos());
            (assert_my_amount(TOTAL),create_coin(FIRST_RECIPIENT,first),create_coin(SECOND_RECIPIENT,second))
        }}
    "#,
        serde_json::to_string(&sdk).unwrap()
    ));
    let raw = extract(&raw.0);
    let readable = extract(&readable.0);
    for optimize in [false, true] {
        let old_bytes = chialisp_oxide_ir::compile(&raw, optimize).unwrap();
        let new_bytes = chialisp_oxide_ir::compile(&readable, optimize).unwrap();
        assert!(
            old_bytes == new_bytes,
            "typed arithmetic helper chain changed code: optimize={optimize}, raw={} bytes, typed={} bytes",
            old_bytes.len(),
            new_bytes.len()
        );
        for first in [0, 1, 999, 1000] {
            let expected = chialisp_oxide_reference::compile(&format!(
                "(mod () (q . ((73 1000) (51 0x{} {first}) (51 0x{} {}))))",
                "2a".repeat(32),
                "2b".repeat(32),
                1000 - first
            ))
            .unwrap();
            assert_eq!(
                execute(&new_bytes, &format!("({first})"), 10_000_000)
                    .unwrap()
                    .outcome,
                execute(&expected, "()", 1_000_000).unwrap().outcome
            );
        }
        for solution in ["(1001)", "(18446744073709551615)", "(0x0001)"] {
            assert_eq!(
                execute(&new_bytes, solution, 10_000_000).unwrap().outcome,
                Outcome::Rejected
            );
        }
    }
}
