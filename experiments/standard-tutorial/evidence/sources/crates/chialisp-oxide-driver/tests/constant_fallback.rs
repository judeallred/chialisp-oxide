//! Actual borrowed-constant MIR must not grant runtime-table retry permission.
mod support;
#[path = "support/vm.rs"]
mod vm;

use chialisp_oxide_backend_experiment::Outcome;
use std::process::{Command, Output};
use support::Fixture;

const HELPER_DEPTH_ERROR: &str =
    "recursion and call depths beyond 32 are unsupported in compile-time constant/helper inlining";
const TABLE_ERROR: &str = "runtime call graphs are limited to 64 functions";

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn native_test(fixture: &Fixture) {
    success(
        &Command::new(fixture.compiler().parent().unwrap().join("cargo"))
            .args(["test", "--lib", "--offline", "--target-dir"])
            .arg(fixture.0.join("native-build"))
            .current_dir(&fixture.0)
            .env("RUSTC", fixture.compiler())
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER")
            .env_remove("CHIALISP_OXIDE_CARGO_MODE")
            .output()
            .unwrap(),
    );
}

fn source(constant_depth: usize, runtime_depth: usize, runtime_constant: bool) -> String {
    let mut source = String::from(
        "use chialisp_oxide::Tree;\n\
         #[inline(never)] const fn constant0() -> Tree<'static> { Tree::Atom(&[2]) }\n",
    );
    for depth in 1..=35 {
        source.push_str(&format!(
            "#[inline(never)] const fn constant{depth}() -> Tree<'static> {{ constant{}() }}\n",
            depth - 1
        ));
    }
    source.push_str(&format!(
        "const MARKER: Tree<'static> = constant{constant_depth}();\n\
         #[inline(never)] fn helper0(value: u8) -> u8 {{ value }}\n"
    ));
    for depth in 1..=runtime_depth {
        source.push_str(&format!(
            "#[inline(never)] fn helper{depth}(value: u8) -> u8 {{ helper{}(value) }}\n",
            depth - 1
        ));
    }
    let marker = if runtime_constant {
        "constant35()"
    } else {
        "MARKER"
    };
    source.push_str(&format!(
        "#[chialisp_oxide::puzzle]\n\
         pub fn entry(value: u8) -> Tree<'static> {{\n\
             let marker = {marker};\n\
             let _checked = helper{runtime_depth}(value);\n\
             marker\n\
         }}\n\
         #[test] fn native_constant_and_entry() {{\n\
             assert_eq!(MARKER, Tree::Atom(&[2]));\n\
             for value in 0..=u8::MAX {{ assert_eq!(entry(value), MARKER); }}\n\
         }}\n"
    ));
    source
}

#[test]
fn borrowed_constant_helper_limit_cannot_trigger_outer_table_retry() {
    // Discovery sees 66 ordinary functions, but the initial acyclic route does
    // not apply the table-only 64-function cap. The constant is encountered
    // first during legacy lowering. If its HelperDepth classification escaped
    // the constant boundary, the retry would fail with TABLE_ERROR instead.
    let deep = Fixture::new();
    deep.source(&source(35, 64, false));
    native_test(&deep);
    let output = deep.cargo(true);
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{diagnostic}");
    assert!(diagnostic.contains(HELPER_DEPTH_ERROR), "{diagnostic}");
    assert!(!diagnostic.contains(TABLE_ERROR), "{diagnostic}");
    assert!(deep.exports().is_empty());

    // Actual source mutation: make only the constant shallow. Legacy lowering
    // now reaches the ordinary helper-depth limit, whose retry is eligible and
    // is observably rejected by the existing table cap.
    let shallow = Fixture::new();
    shallow.source(&source(0, 64, false));
    native_test(&shallow);
    let output = shallow.cargo(true);
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{diagnostic}");
    assert!(diagnostic.contains(TABLE_ERROR), "{diagnostic}");
    assert!(!diagnostic.contains(HELPER_DEPTH_ERROR), "{diagnostic}");
    assert!(shallow.exports().is_empty());
}

#[test]
fn runtime_helpers_and_shallow_constants_keep_their_distinct_routes() {
    // The same deep const-fn bodies are supported as ordinary runtime calls.
    // A small materialized borrowed constant retains its old legacy route.
    for (runtime_constant, expected_functions) in [(true, 38), (false, 0)] {
        let fixture = Fixture::new();
        fixture.source(&source(0, 0, runtime_constant));
        native_test(&fixture);
        success(&fixture.cargo(true));
        let programs = fixture.exports();
        assert_eq!(programs.len(), 1);
        assert_eq!(programs[0].functions.len(), expected_functions);
        programs[0].validate().unwrap();
        let reference = chialisp_oxide_reference::compile("(mod (VALUE) 2)").unwrap();
        for optimize in [false, true] {
            let bytes = chialisp_oxide_ir::compile(&programs[0], optimize).unwrap();
            for value in [0, 7, u8::MAX] {
                let input = format!("({value})");
                let expected = Outcome::Accepted {
                    output_hex: "02".into(),
                };
                assert_eq!(
                    vm::execute(&bytes, &input, 10_000_000).unwrap().outcome,
                    expected
                );
                assert_eq!(
                    vm::execute(&reference, &input, 10_000_000).unwrap().outcome,
                    expected
                );
            }
        }
    }
}
