//! Actual Cargo/native source for the internal-only v9 callable capability.
//! Independent source comparisons below are finite behavior checks, not a Rust
//! closure/dependency admission, compiler-preservation or complete-spend proof.
mod support;
#[path = "support/vm.rs"]
mod vm;
use chialisp_oxide_backend_experiment::{FailureCode, Outcome};
#[path = "support/callable_fixture.rs"]
mod callable_fixture;
use callable_fixture::{expected, fixture, native};

const SOURCE: &str = include_str!("fixtures/local_callables.rs");

fn cases(name: &str) -> &'static [(&'static str, &'static str)] {
    match name {
        "choose" => &[
            ("(1 7)", "8"),
            ("(() 7)", "14"),
            ("(1 0)", "1"),
            ("(() 99)", "198"),
        ],
        "copied" => &[("(1 7)", "(8 14)"), ("(() 7)", "(14 8)")],
        "same_name" => &[("(7)", "10")],
        "borrowed" => &[("(0x0001)", "0x0001"), ("((1 . 2))", "(1 . 2)")],
        "recursive" | "self_callback" => &[("(0)", "7"), ("(5)", "7")],
        "unused_ref" => &[("(1)", "7"), ("(())", "7")],
        "divergence" => &[("(() 7)", "8")],
        "map_add" => &[
            ("((1 2 3))", "(2 3 4)"),
            ("(())", "()"),
            ("((0x0001 -1 127))", "(2 () 128)"),
            (
                "((340282366920938463463374607431768211456))",
                "(340282366920938463463374607431768211457)",
            ),
            ("((1 2) . 9)", "(2 3)"),
        ],
        "map_choose" => &[
            ("(1 (1 2 3))", "(2 3 4)"),
            ("(() (0x0001 -1 127))", "(2 -2 254)"),
        ],
        "callee_order" | "arguments_order" | "unused_argument" => &[],
        _ => panic!("unknown source export {name}"),
    }
}
fn reference(name: &str) -> Option<String> {
    // These independent typed references describe the listed canonical typed
    // values. Strict malformed Rust ABI checks below are a separate lane.
    Some(match name {
        "choose"=>"(mod (C N) (if (> N 99) (x) (if C (+ N 1) (* N 2))))".into(),
        "copied"=>"(mod (C N) (if (> N 99) (x) (list (if C (+ N 1) (* N 2)) (if C (* N 2) (+ N 1)))))".into(),
        "same_name"=>"(mod (N) (+ N 3))".into(),
        "borrowed"=>"(mod (V) V)".into(),
        "recursive" | "self_callback"=>"(mod (N) (defun go (N) (if N (go (- N 1)) 7)) (go N))".into(),
        "unused_ref"=>"(mod (C) 7)".into(),
        "divergence"=>"(mod (C N) (defun forever (N) (forever N)) (if C (forever N) (+ N 1)))".into(),
        "map_choose"=>"(mod (C L) (defun map (C L) (if L (c (if C (+ (f L) 1) (* (f L) 2)) (map C (r L))) ())) (map C L))".into(),
        "map_add"|"callee_order"|"arguments_order"|"unused_argument"=>return None,
        _=>panic!("{name}"),
    })
}

#[test]
fn actual_local_function_values_match_native_independent_sources_and_both_modes() {
    let fixture = fixture(SOURCE);
    native(&fixture, 3);
    let extraction = fixture.cargo(true);
    assert!(
        extraction.status.success(),
        "{}",
        String::from_utf8_lossy(&extraction.stderr)
    );
    let programs = fixture.exports();
    assert_eq!(programs.len(), 13);
    let named = chialisp_oxide_reference::compile_modern_cl24(include_str!(
        "../../../puzzles/modern-named-map/reference.clsp"
    ))
    .unwrap();
    let lambda = chialisp_oxide_reference::compile_modern_cl24(include_str!(
        "../../../puzzles/modern-lambda-map/reference.clsp"
    ))
    .unwrap();
    let rue = fixture.0.join("independent-map.rue");
    std::fs::write(
        &rue,
        r#"
fn increment(value: Int) -> Int { value + 1 }
fn map(step: fn(value: Int) -> Int, values: Any) -> Any {
    if values is nil { nil } else {
        assert values is (Any, Any);
        let value = values.first;
        assert value is Bytes;
        (step(value as Int), map(step, values.rest))
    }
}
fn main(...environment: Any) -> Any {
    assert environment is (Any, Any);
    map(increment, environment.first)
}
"#,
    )
    .unwrap();
    let independent_rue = [
        chialisp_oxide_rue::compile_file(&rue, false).unwrap(),
        chialisp_oxide_rue::compile_file(&rue, true).unwrap(),
    ];
    for program in programs {
        let name = program.name.rsplit("::").next().unwrap();
        program.validate().unwrap();
        assert!(program.uses_callables(), "{name}");
        assert_eq!(
            serde_json::to_value(&program).unwrap()["ir_version"],
            9,
            "{name}"
        );
        let independent =
            reference(name).map(|source| chialisp_oxide_reference::compile(&source).unwrap());
        for optimize in [false, true] {
            let code = chialisp_oxide_ir::compile(&program, optimize).unwrap();
            for &(solution, value) in cases(name) {
                let expected = expected(value);
                assert_eq!(
                    vm::execute(&code, solution, 20_000_000).unwrap().outcome,
                    expected,
                    "{name}/{optimize}/{solution}"
                );
                if let Some(reference) = &independent {
                    assert_eq!(
                        vm::execute(reference, solution, 20_000_000)
                            .unwrap()
                            .outcome,
                        expected,
                        "Chialisp/{name}/{solution}"
                    );
                }
                if name == "map_add" {
                    for reference in [&named, &lambda, &independent_rue[0], &independent_rue[1]] {
                        assert_eq!(
                            vm::execute(reference, solution, 20_000_000)
                                .unwrap()
                                .outcome,
                            expected,
                            "independent raw map/{solution}"
                        );
                    }
                }
            }
            if ["choose", "copied"].contains(&name) {
                for solution in ["(1 100)", "(() 100)"] {
                    let execution = vm::execute(&code, solution, 20_000_000).unwrap();
                    assert_eq!(execution.outcome, Outcome::Rejected);
                    assert_eq!(execution.failure.unwrap().code(), FailureCode::Raise);
                }
                for malformed in [
                    "()",
                    "1",
                    "(1)",
                    "(1 7 8)",
                    "(1 0x0001)",
                    "(0x00 7)",
                    "(1 -1)",
                    "(1 . 7)",
                ] {
                    assert_eq!(
                        vm::execute(&code, malformed, 20_000_000).unwrap().outcome,
                        Outcome::Rejected,
                        "strictABI/{name}/{malformed}"
                    );
                }
            }
            let ordered: &[(&str, FailureCode)] = match name {
                "callee_order" => &[
                    ("(())", FailureCode::Raise),
                    ("(1)", FailureCode::InvalidOpArg),
                ],
                "arguments_order" => &[
                    ("(())", FailureCode::InvalidOpArg),
                    ("(1)", FailureCode::Raise),
                ],
                "unused_argument" => &[("()", FailureCode::InvalidOpArg)],
                _ => &[],
            };
            for &(solution, failure) in ordered {
                let execution = vm::execute(&code, solution, 20_000_000).unwrap();
                assert_eq!(
                    execution.outcome,
                    Outcome::Rejected,
                    "strictcall/{name}/{solution}"
                );
                assert_eq!(
                    execution.failure.unwrap().code(),
                    failure,
                    "strictcall/{name}/{solution}"
                );
            }
            if name == "map_add" {
                for malformed in [
                    "()",
                    "7",
                    "(0x00)",
                    "(0x0000)",
                    "((1 . 0x00))",
                    "(((1 . 2)))",
                    "(((8)))",
                ] {
                    assert_eq!(
                        vm::execute(&code, malformed, 20_000_000).unwrap().outcome,
                        Outcome::Rejected,
                        "rawmap/{malformed}"
                    );
                    for reference in [&named, &lambda, &independent_rue[0], &independent_rue[1]] {
                        assert_eq!(
                            vm::execute(reference, malformed, 20_000_000)
                                .unwrap()
                                .outcome,
                            Outcome::Rejected,
                            "independentraw/{malformed}"
                        );
                    }
                }
            }
            if name == "divergence" {
                let execution =
                    chialisp_oxide_backend_experiment::execute(&code, "(1 0)", 200_000).unwrap();
                assert_eq!(execution.outcome, Outcome::Inconclusive);
                assert_eq!(execution.failure.unwrap().code(), FailureCode::CostExceeded);
            }
        }
    }
}

#[test]
fn native_valid_callable_extensions_remain_outside_the_closed_profile() {
    let sources = [
        (
            "public ABI",
            "#[chialisp_oxide::puzzle] pub fn puzzle(step:fn(u64)->u64)->u64{step(7)}",
        ),
        (
            "return",
            "fn inc(x:u64)->u64{x+1} #[chialisp_oxide::puzzle] pub fn puzzle()->fn(u64)->u64{inc}",
        ),
        (
            "aggregate",
            "fn inc(x:u64)->u64{x+1} #[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let steps=(inc as fn(u64)->u64,);(steps.0)(x)}",
        ),
        (
            "mutable",
            "fn inc(x:u64)->u64{x+1} fn twice(x:u64)->u64{x*2} #[chialisp_oxide::puzzle] pub fn puzzle(c:bool,x:u64)->u64{let mut f:fn(u64)->u64=inc;if c{f=twice;}f(x)}",
        ),
        (
            "forwarded mutable parameter",
            "type Step=fn(u64)->u64; fn inc(x:u64)->u64{x+1} fn twice(x:u64)->u64{x*2} fn invoke(f:Step,x:u64)->u64{f(x)} fn forward(mut f:Step,g:Step,c:bool,x:u64)->u64{if c{f=g;}invoke(f,x)} #[chialisp_oxide::puzzle] pub fn puzzle(c:bool,x:u64)->u64{forward(inc,twice,c,x)}",
        ),
        (
            "unused mutable binding in selected target",
            "type Step=fn(u64)->u64; fn twice(x:u64)->u64{x*2} fn inc(x:u64)->u64{let mut unused:Step=twice;let _=unused;x+1} fn invoke(f:Step,x:u64)->u64{f(x)} #[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{invoke(inc,x)}",
        ),
        (
            "unreachable mutable binding",
            "type Step=fn(u64)->u64; fn inc(x:u64)->u64{x+1} fn invoke(f:Step,x:u64)->u64{f(x)} #[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{if false{let mut never:Step=inc;let _=never;}invoke(inc,x)}",
        ),
        (
            "shadowed mutable binding",
            "type Step=fn(u64)->u64; fn inc(x:u64)->u64{x+1} fn twice(x:u64)->u64{x*2} fn invoke(f:Step,x:u64)->u64{f(x)} #[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let mut step:Step=inc;{let step:Step=twice;let _=step;}invoke(step,x)}",
        ),
        (
            "macro-injected mutable binding",
            "type Step=fn(u64)->u64; fn inc(x:u64)->u64{x+1} fn twice(x:u64)->u64{x*2} fn invoke(f:Step,x:u64)->u64{f(x)} macro_rules! pointer {()=>{{let mut hidden:Step=inc;hidden=twice;hidden}}} #[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let f=pointer!();invoke(f,x)}",
        ),
        (
            "projected capturing closure",
            "#[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let base=(x,2u8);let f=move |n:u64|base.0+n;f(1)}",
        ),
        (
            "capture",
            "#[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let f=|y:u64|x+y;f(1)}",
        ),
        (
            "Fn trait",
            "fn invoke<F:Fn(u64)->u64>(f:F,x:u64)->u64{f(x)} #[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{invoke(|v|v+1,x)}",
        ),
        (
            "generic item",
            "fn id<T>(x:T)->T{x} #[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let f:fn(u64)->u64=id::<u64>;f(x)}",
        ),
        (
            "foreign ABI",
            "extern \"C\" fn inc(x:u64)->u64{x+1} #[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let f:extern \"C\" fn(u64)->u64=inc;f(x)}",
        ),
        (
            "unsafe pointer",
            "fn inc(x:u64)->u64{x+1} #[chialisp_oxide::puzzle] pub fn puzzle()->u64{let _f:unsafe fn(u64)->u64=inc;7}",
        ),
        (
            "variadic ABI",
            "#[chialisp_oxide::puzzle] pub fn puzzle(_f:unsafe extern \"C\" fn(u64,...)->u64)->u64{7}",
        ),
        (
            "unreviewed borrow",
            "fn read(x:&u64)->u64{*x} #[chialisp_oxide::puzzle] pub fn puzzle()->u64{let f:for<'a> fn(&'a u64)->u64=read;f(&7)}",
        ),
        (
            "comparison",
            "fn inc(x:u64)->u64{x+1} fn twice(x:u64)->u64{x*2} #[allow(unpredictable_function_pointer_comparisons)] #[chialisp_oxide::puzzle] pub fn puzzle()->bool{let f:fn(u64)->u64=inc;f==(twice as fn(u64)->u64)}",
        ),
        (
            "address",
            "fn inc(x:u64)->u64{x+1} #[chialisp_oxide::puzzle] pub fn puzzle()->u64{let f:fn(u64)->u64=inc;f as usize as u64}",
        ),
        (
            "const pointer",
            "fn inc(x:u64)->u64{x+1} const STEP:fn(u64)->u64=inc; #[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{STEP(x)}",
        ),
        (
            "unselected unsupported target",
            "fn inc(x:u64)->u64{x+1} fn bad(x:u64)->u64{vec![x][0]} #[chialisp_oxide::puzzle] pub fn puzzle(c:bool,x:u64)->u64{let f:fn(u64)->u64=if c{inc}else{bad};f(x)}",
        ),
        (
            "external SHA target",
            "#[chialisp_oxide::puzzle] pub fn puzzle(v:&[u8])->[u8;32]{let f:fn(&[u8])->[u8;32]=chialisp_oxide::sha256;f(v)}",
        ),
        (
            "external apply target",
            "use chialisp_oxide::{Tree,apply}; #[chialisp_oxide::puzzle] pub fn puzzle(v:&Tree<'_>)->Tree<'static>{let f:for<'a> fn(&Tree<'a>,&Tree<'a>)->Tree<'static>=apply;f(v,v)}",
        ),
        (
            "external SDK target",
            "use chialisp_oxide::{Tree,raw_clvm}; #[chialisp_oxide::puzzle] pub fn puzzle(v:&Tree<'_>)->Tree<'static>{let f:for<'a> fn(&'a Tree<'a>)->Tree<'static>=raw_clvm::not;f(v)}",
        ),
    ];
    for (label, source) in sources {
        let fixture = fixture(source);
        let native = fixture.cargo(false);
        assert!(
            native.status.success(),
            "{label} is not a native-valid control: {}",
            String::from_utf8_lossy(&native.stderr)
        );
        let extraction = fixture.cargo(true);
        assert!(!extraction.status.success(), "unreviewed {label} accepted");
        let error = String::from_utf8_lossy(&extraction.stderr);
        assert!(
            error.contains("chialisp-oxide") && !error.contains("internal compiler error"),
            "{label}: {error}"
        );
        if label.contains("mutable") || label.contains("shadowed") {
            assert!(
                error.contains("callable source bindings must be immutable"),
                "{label}: {error}"
            );
        }
        println!("native valid / subset rejected: {label}");
        assert!(
            fixture.exports().is_empty(),
            "{label} emitted an artifact after rejecting"
        );
    }
}

#[test]
fn callable_owner_preflights_bound_source_width_and_hir_depth() {
    let prefix = "fn inc(x:u64)->u64{x+1} fn invoke(f:fn(u64)->u64,x:u64)->u64{f(x)} #[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{";
    let wide = format!(
        "{prefix}{}invoke(inc,x)}}",
        "if true {} else {};".repeat(1_500)
    );
    let deep = format!(
        "{prefix}{}invoke(inc,x){}}}",
        "{".repeat(40),
        "}".repeat(40)
    );
    for (label, source) in [("wide", wide), ("deep", deep)] {
        let fixture = fixture(&source);
        let native = fixture.cargo(false);
        assert!(
            native.status.success(),
            "{label}: {}",
            String::from_utf8_lossy(&native.stderr)
        );
        let extraction = fixture.cargo(true);
        let error = String::from_utf8_lossy(&extraction.stderr);
        assert!(
            !extraction.status.success(),
            "{label} owner exceeded HIR bounds but was accepted"
        );
        let expected = if label == "wide" {
            // Existing MIR preflight is stronger for this source shape. Do not
            // raise it merely to reach the later HIR-node cap.
            "owned MIR analysis exceeds 8,192 statements, 10,000 blocks or 2,048 locals"
        } else {
            "callable owner HIR walk exceeds 10,000 nodes or depth 64"
        };
        assert!(error.contains(expected), "{label}: {error}");
        assert!(
            !error.contains("internal compiler error"),
            "{label}: {error}"
        );
        assert!(fixture.exports().is_empty());
        println!("native valid / bounded source-preflight rejection: {label}");
    }
}
