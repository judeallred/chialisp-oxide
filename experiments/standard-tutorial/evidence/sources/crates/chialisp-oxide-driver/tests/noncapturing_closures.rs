//! Finite actual-source tests for safe zero-environment closure-to-fn coercions.
//! Direct closure invocation, captured environments and generic Fn remain closed.
#[path = "support/callable_fixture.rs"]
mod callable_fixture;
mod support;
#[path = "support/vm.rs"]
mod vm;
use callable_fixture::{expected, fixture, native};
use chialisp_oxide_backend_experiment::{FailureCode, Outcome};

const SOURCE: &str = include_str!("fixtures/noncapturing_closures.rs");

#[test]
fn actual_closure_coercions_match_native_and_independent_sources_both_modes() {
    let fixture = fixture(SOURCE);
    native(&fixture, 3);
    let extraction = fixture.cargo(true);
    assert!(
        extraction.status.success(),
        "{}",
        String::from_utf8_lossy(&extraction.stderr)
    );
    let programs = fixture.exports();
    assert_eq!(programs.len(), 7);
    // This exact pinned module is selected in the modern-cl24 provider contract;
    // the Rust export preserves its raw (Z . ignored-tail) environment behavior.
    let lambda = chialisp_oxide_reference::compile_modern_cl24(include_str!(
        "../../../puzzles/modern-lambda-map/reference.clsp"
    ))
    .unwrap();
    let rue = fixture.0.join("independent-closure-map.rue");
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
        program.validate().unwrap();
        let name = program.name.rsplit("::").next().unwrap();
        assert!(program.uses_callables(), "{name}");
        assert_eq!(
            serde_json::to_value(&program).unwrap()["ir_version"],
            9,
            "{name}"
        );
        let (vectors, reference): (&[(&str, &str)], Option<&str>) = match name {
            "shapes" => (
                &[
                    ("(1 7 3)", "(7 8 10 10)"),
                    ("(() 7 3)", "(7 14 10 10)"),
                    ("(1 0 255)", "(7 1 255 255)"),
                ],
                Some("(mod (C N B) (list 7 (if C (+ N 1) (* N 2)) (+ N B) (+ N B)))"),
            ),
            "copied" => (
                &[("(7)", "(8 9)"), ("(0)", "(1 2)")],
                Some("(mod (N) (list (+ N 1) (+ N 2)))"),
            ),
            "borrowed" => (
                &[
                    ("(())", "()"),
                    ("(0x00)", "0x00"),
                    ("(0x0001)", "0x0001"),
                    ("((1 . 2))", "(1 . 2)"),
                ],
                Some("(mod (V) V)"),
            ),
            "literal_borrowed" => (
                &[
                    ("(())", "0x0001"),
                    ("(0x00)", "0x00"),
                    ("((1 . 2))", "(1 . 2)"),
                ],
                Some("(mod (V) (if (l V) V (if (= V ()) (q . 0x0001) V)))"),
            ),
            "map_add" => (
                &[
                    ("(())", "()"),
                    ("((1 2 3))", "(2 3 4)"),
                    ("((0x0001 -1 127))", "(2 () 128)"),
                    (
                        "((340282366920938463463374607431768211456))",
                        "(340282366920938463463374607431768211457)",
                    ),
                    ("((1 2) . 0x00)", "(2 3)"),
                ],
                None,
            ),
            "strict_initialization" => (&[], None),
            "divergence" => (
                &[("(() 7)", "8")],
                Some("(mod (C N) (defun forever (N) (forever N)) (if C (forever N) (+ N 1)))"),
            ),
            _ => panic!("unexpected export {name}"),
        };
        let reference = reference.map(|text| chialisp_oxide_reference::compile(text).unwrap());
        for optimize in [false, true] {
            let code = chialisp_oxide_ir::compile(&program, optimize).unwrap();
            for &(solution, value) in vectors {
                let expected = expected(value);
                assert_eq!(
                    vm::execute(&code, solution, 20_000_000).unwrap().outcome,
                    expected,
                    "Rust/{name}/{optimize}/{solution}"
                );
                if let Some(reference) = &reference {
                    assert_eq!(
                        vm::execute(reference, solution, 20_000_000)
                            .unwrap()
                            .outcome,
                        expected,
                        "Chialisp/{name}/{solution}"
                    );
                }
                if name == "map_add" {
                    for independent in [&lambda, &independent_rue[0], &independent_rue[1]] {
                        assert_eq!(
                            vm::execute(independent, solution, 20_000_000)
                                .unwrap()
                                .outcome,
                            expected,
                            "independent-map/{solution}"
                        );
                    }
                }
            }
            if name == "strict_initialization" {
                for (solution, failure) in [
                    ("(())", FailureCode::Raise),
                    ("(1)", FailureCode::InvalidOpArg),
                ] {
                    let execution = vm::execute(&code, solution, 20_000_000).unwrap();
                    assert_eq!(
                        execution.outcome,
                        Outcome::Rejected,
                        "strict/{optimize}/{solution}"
                    );
                    assert_eq!(
                        execution.failure.unwrap().code(),
                        failure,
                        "strict/{optimize}/{solution}"
                    );
                }
            }
            if name == "divergence" {
                let execution =
                    chialisp_oxide_backend_experiment::execute(&code, "(1 7)", 200_000).unwrap();
                assert_eq!(execution.outcome, Outcome::Inconclusive);
                assert_eq!(execution.failure.unwrap().code(), FailureCode::CostExceeded);
            }
            if name == "copied" {
                let execution = vm::execute(&code, "(18446744073709551615)", 20_000_000).unwrap();
                assert_eq!(execution.outcome, Outcome::Rejected);
                assert_eq!(execution.failure.unwrap().code(), FailureCode::Raise);
            }
            if name == "shapes" {
                for malformed in [
                    "()",
                    "(1 7)",
                    "(1 7 3 4)",
                    "(0x00 7 3)",
                    "(1 0x0001 3)",
                    "(1 7 256)",
                    "(1 7 . 3)",
                ] {
                    assert_eq!(
                        vm::execute(&code, malformed, 20_000_000).unwrap().outcome,
                        Outcome::Rejected,
                        "strictABI/{optimize}/{malformed}"
                    );
                }
            }
            if name == "map_add" {
                // Typed failures remain measured per independently compiled
                // program; path optimization may change the exact raw f error.
                for malformed in [
                    "()",
                    "7",
                    "(0x00)",
                    "(0x0000)",
                    "((1 . 0x00))",
                    "(((1 . 2)))",
                ] {
                    for independent in [&code, &lambda, &independent_rue[0], &independent_rue[1]] {
                        let execution = vm::execute(independent, malformed, 20_000_000).unwrap();
                        assert_eq!(
                            execution.outcome,
                            Outcome::Rejected,
                            "raw-map/{optimize}/{malformed}"
                        );
                        assert!(execution.failure.is_some(), "typed failure required");
                    }
                }
            }
        }
    }
}

#[test]
fn native_valid_closure_extensions_stay_outside_zero_environment_profile() {
    // Capturing closures cannot natively coerce to fn pointers; these controls
    // use native-valid direct closure calls and claim no forged MIR coverage.
    let sources = [
        (
            "projected scalar capture",
            "#[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let values=(x,2u8);let f=move |n:u64|values.0+n;f(1)}",
        ),
        (
            "zero-sized unit capture",
            "#[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let unit=();let f=move |n:u64|{let _=&unit;n+1};f(x)}",
        ),
        (
            "owned capture",
            "#[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let owned=String::from(\"x\");let f=move |n:u64|n+owned.len() as u64;f(x)}",
        ),
        (
            "Vec body",
            "#[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let f:fn(u64)->u64=|n|vec![n][0];f(x)}",
        ),
        (
            "unused unsupported closure body",
            "#[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let _unused:fn(u64)->u64=|n|vec![n][0];x}",
        ),
        (
            "unselected unsupported closure body",
            "fn good(x:u64)->u64{x+1} #[chialisp_oxide::puzzle] pub fn puzzle(c:bool,x:u64)->u64{let f:fn(u64)->u64=if c{good}else{|n|vec![n][0]};f(x)}",
        ),
        (
            "direct Fn call",
            "#[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let f=|n:u64|n+1;f(x)}",
        ),
        (
            "mutable closure intermediate",
            "#[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let mut closure=|n:u64|n+1;let f:fn(u64)->u64=closure;f(x)}",
        ),
        (
            "unreachable mutable closure",
            "#[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{if false{let mut never=|n:u64|n+1;let _=never;}let f:fn(u64)->u64=|n|n+2;f(x)}",
        ),
        (
            "closure tuple storage",
            "#[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let storage=(|n:u64|n+1,);let f:fn(u64)->u64=storage.0;f(x)}",
        ),
        (
            "closure Option storage",
            "#[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let storage=Some(|n:u64|n+1);match storage{Some(c)=>{let f:fn(u64)->u64=c;f(x)},None=>0}}",
        ),
        (
            "returned closure",
            "fn make()->impl Fn(u64)->u64{|n|n+1} #[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let closure=make();closure(x)}",
        ),
        (
            "const pointer closure",
            "const STEP:fn(u64)->u64=|n|n+1; #[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{STEP(x)}",
        ),
        (
            "static pointer closure",
            "static STEP:fn(u64)->u64=|n|n+1; #[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{STEP(x)}",
        ),
        (
            "generic Fn",
            "fn invoke<F:Fn(u64)->u64>(f:F,x:u64)->u64{f(x)} #[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{invoke(|n|n+1,x)}",
        ),
        (
            "trait object",
            "#[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let f:&dyn Fn(u64)->u64=&|n|n+1;f(x)}",
        ),
        (
            "borrowed closure intermediate",
            "#[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let closure=|n:u64|n+1;let f=&closure;f(x)}",
        ),
        (
            "same-name custom call metadata",
            "struct Closure{value:u64} impl Closure{fn call(&self,n:u64)->u64{self.value+n}} #[chialisp_oxide::puzzle] pub fn puzzle(x:u64)->u64{let f:fn(u64)->u64=|n|{let c=Closure{value:2};c.call(n)};f(x)}",
        ),
    ];
    for (label, source) in sources {
        let fixture = fixture(source);
        let native = fixture.cargo(false);
        assert!(
            native.status.success(),
            "{label} must be native valid: {}",
            String::from_utf8_lossy(&native.stderr)
        );
        let extraction = fixture.cargo(true);
        assert!(!extraction.status.success(), "excluded {label} accepted");
        let error = String::from_utf8_lossy(&extraction.stderr);
        assert!(
            error.contains("chialisp-oxide") && !error.contains("internal compiler error"),
            "{label}: {error}"
        );
        assert!(
            fixture.exports().is_empty(),
            "{label} emitted an artifact after rejection"
        );
        println!("native valid / subset rejected: {label}");
    }
}

#[test]
fn closure_receiver_still_counts_toward_existing_body_argument_bound() {
    let types = vec!["u8"; 64].join(",");
    let parameters = (0..64)
        .map(|n| format!("a{n}"))
        .collect::<Vec<_>>()
        .join(",");
    let values = vec!["7"; 64].join(",");
    let source = format!(
        "#[chialisp_oxide::puzzle] pub fn puzzle()->u8{{let f:fn({types})->u8=|{parameters}|a0;f({values})}}"
    );
    let fixture = fixture(&source);
    let native = fixture.cargo(false);
    assert!(
        native.status.success(),
        "native64 userarguments: {}",
        String::from_utf8_lossy(&native.stderr)
    );
    let extraction = fixture.cargo(true);
    assert!(
        !extraction.status.success(),
        "65 actual MIR arguments accepted"
    );
    let error = String::from_utf8_lossy(&extraction.stderr);
    assert!(
        error.contains("function exceeds 64 parameters or 2,048 MIR locals"),
        "{error}"
    );
    assert!(!error.contains("internal compiler error"), "{error}");
    assert!(fixture.exports().is_empty());
}
