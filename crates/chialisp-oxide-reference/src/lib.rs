//! Compile independent Chialisp references using Chia Network's Rust compiler.
//!
//! The compiler's CLVM allocator is deliberately confined to this crate. Serialized
//! bytes form the bridge to the project's pinned execution VM; this prevents an
//! accidental comparison against a different execution dialect or VM version.

use std::{collections::HashMap, rc::Rc};

use anyhow::{Context, Result, anyhow};
use chialisp::{
    classic::clvm_tools::{
        binutils::assemble,
        clvmc::compile_clvm_text,
        stages::{stage_0::TRunProgram, stage_2::operators::run_program_for_search_paths},
    },
    compiler::{compiler::DefaultCompilerOpts, comptypes::CompilerOpts},
};
use clvmr_reference::{
    allocator::{Allocator, NodePtr},
    serde::node_to_bytes,
};

/// Exact independently maintained reference compiler used for evidence reports.
pub const COMPILER_VERSION: &str = "chialisp 0.4.6";

/// Compile a self-contained Chialisp module into serialized CLVM.
///
/// Source syntax selects the upstream compiler dialect: traditional `mod` sources
/// use the classic compiler; a standard dialect include selects the modern one.
/// Upstream optimization is enabled. The returned program must still be executed
/// by the same VM and flags as the Rust-generated program before comparing it.
///
/// Only compile trusted fixture source. Upstream includes can access local files;
/// this function is an in-process compiler, not an isolation boundary.
pub fn compile(source: &str) -> Result<Vec<u8>> {
    const NAME: &str = "reference.clsp";
    let mut allocator = Allocator::new();
    let options: Rc<dyn CompilerOpts> = Rc::new(DefaultCompilerOpts::new(NAME));
    let mut symbols = HashMap::new();
    let root = compile_clvm_text(
        &mut allocator,
        options.clone(),
        &mut symbols,
        source,
        NAME,
        true,
    )
    .map_err(|error| anyhow!("{COMPILER_VERSION}: {}", error.format(&allocator, options)))?;
    node_to_bytes(&allocator, root).context("serializing reference CLVM")
}

/// Compile a classic Chialisp expression, without evaluating the resulting code.
///
/// The upstream `run` command executes `(a (opt (com 2)) 3)` against the source
/// and its environment. Here only its `(opt (com 2))` compilation stage runs;
/// the serialized result can then execute against any environment in our VM.
/// This adds no `mod` wrapper or argument-shape restriction to the source.
/// `compile` instead uses upstream's module-oriented API, whose classic path
/// also evaluates a top-level expression and would return its value as code.
///
/// Only use trusted fixture source: compilation is not an isolation boundary.
pub fn compile_expression(source: &str) -> Result<Vec<u8>> {
    const NAME: &str = "reference.clsp";
    let mut allocator = Allocator::new();
    let expression = assemble(&mut allocator, source)
        .map_err(|error| anyhow!("{COMPILER_VERSION}: assembling expression: {error}"))?;
    let compile_stage = assemble(&mut allocator, "(opt (com 2))")
        .map_err(|error| anyhow!("{COMPILER_VERSION}: assembling compilation stage: {error}"))?;
    let input = allocator
        .new_pair(expression, NodePtr::NIL)
        .context("constructing expression compiler input")?;
    let runner = run_program_for_search_paths(NAME, &[], false, 0);
    let program = runner
        .run_program(&mut allocator, compile_stage, input, None)
        .map_err(|error| anyhow!("{COMPILER_VERSION}: compiling expression: {error}"))?
        .1;
    node_to_bytes(&allocator, program).context("serializing expression CLVM")
}

#[cfg(test)]
mod tests {
    use chialisp_oxide_vm::{FailureCode, classify_failure};
    use clvmr::{
        ChiaDialect,
        allocator::{Allocator, NodePtr},
        chia_dialect::MEMPOOL_MODE,
        run_program,
        serde::{node_from_bytes, node_to_bytes},
    };

    use super::{compile, compile_expression};

    /// Run serialized output through the project's VM, not the compiler's VM.
    fn execute(program: &[u8], solution: &[u8]) -> Result<Vec<u8>, String> {
        execute_expected(program, solution, None)
    }

    fn execute_expected(
        program: &[u8],
        solution: &[u8],
        expected_failure: Option<FailureCode>,
    ) -> Result<Vec<u8>, String> {
        let mut allocator = Allocator::new();
        let program =
            node_from_bytes(&mut allocator, program).expect("compiled VM fixture must decode");
        let solution =
            node_from_bytes(&mut allocator, solution).expect("solution fixture must decode");
        match run_program(
            &mut allocator,
            &ChiaDialect::new(MEMPOOL_MODE),
            program,
            solution,
            1_000_000,
        ) {
            Ok(result) => {
                assert!(expected_failure.is_none(), "expected a program rejection");
                Ok(node_to_bytes(&allocator, result.1).expect("VM output must serialize"))
            }
            Err(error) => {
                let failure = classify_failure(&error);
                assert!(
                    failure.is_program_rejection(),
                    "inconclusive VM failure: {failure:?}: {error:?}"
                );
                if let Some(expected) = expected_failure {
                    assert_eq!(
                        failure.code(),
                        expected,
                        "wrong program rejection: {error:?}"
                    );
                }
                Err(error.to_string())
            }
        }
    }

    #[test]
    fn compiles_and_runs_arithmetic_in_project_vm() {
        let program = compile("(mod (value) (* value 2))").unwrap();
        // Proper list (21), with result 42.
        assert_eq!(execute(&program, &[0xff, 21, 0x80]).unwrap(), vec![42]);
    }

    #[test]
    fn canonical_intro_expressions_compile_without_wrappers_and_ignore_environment() {
        // Exact expressions from chialisp-web dcf06c14836c7ad3f486a5bea36c5a812fd9a5e8,
        // docs/chialisp-primer/intro.md lines 48, 54 and 64 (Apache-2.0).
        // No `mod`, argument guard, or source transformation is added here.
        let expressions: [(&str, &[u8]); 3] = [
            ("(list 1 2 3)", &[0xff, 1, 0xff, 2, 0xff, 3, 0x80]),
            ("(+ 2 3)", &[5]),
            ("(* (if (> 3 2) 10 5) 10)", &[100]),
        ];
        let mut byte_string = vec![0xa0];
        byte_string.extend_from_slice(&[0x42; 32]);
        let environments = [
            vec![0x80],                         // nil
            vec![1],                            // atom
            vec![0x81, 0xff],                   // negative integer atom
            vec![0x82, 0, 1],                   // padded numeric atom
            vec![0xff, 1, 2],                   // improper pair
            vec![0xff, 1, 0xff, 2, 0x80],       // proper list
            vec![0xff, 0xff, 1, 2, 0xff, 3, 4], // nested improper pairs
            byte_string,
        ];
        for (source, expected) in expressions {
            let program = compile_expression(source).unwrap();
            for environment in &environments {
                assert_eq!(
                    execute(&program, environment).unwrap(),
                    expected,
                    "{source} changed behavior for environment {environment:02x?}",
                );
            }
        }
    }

    #[test]
    fn expression_compilation_does_not_evaluate_or_freeze_its_environment() {
        let program = compile_expression("(+ (f @) 1)").unwrap();
        assert_eq!(execute(&program, &[0xff, 41, 0x80]).unwrap(), vec![42]);
        assert_eq!(execute(&program, &[0xff, 1, 0x80]).unwrap(), vec![2]);
        assert!(execute(&program, &[0x80]).is_err());
    }

    #[test]
    fn expression_compilation_reports_malformed_and_unknown_source() {
        for source in ["(list 1 2", "(undefined 1)"] {
            let error = compile_expression(source).unwrap_err();
            assert!(error.to_string().contains(super::COMPILER_VERSION));
        }
    }

    #[test]
    fn compiles_a_guarded_condition_puzzle_and_preserves_rejection() {
        let program = compile(
            "(mod (amount) (if (all (> amount 0) (> 101 amount)) \
             (list (list 51 0x01020304 amount)) (x)))",
        )
        .unwrap();
        assert_eq!(
            execute(&program, &[0xff, 42, 0x80]).unwrap(),
            vec![0xff, 0xff, 51, 0xff, 0x84, 1, 2, 3, 4, 0xff, 42, 0x80, 0x80]
        );
        for value in [0, 101, 127] {
            assert!(
                execute_expected(&program, &[0xff, value, 0x80], Some(FailureCode::Raise)).is_err()
            );
        }
        // Missing argument and pair supplied where arithmetic requires an atom.
        assert!(execute(&program, &[0x80]).is_err());
        assert!(execute(&program, &[0xff, 0xff, 1, 0x80, 0x80]).is_err());
    }

    #[test]
    fn modern_dialect_also_crosses_the_serialized_bridge() {
        let program = compile("(mod (value) (include *standard-cl-21*) (+ value 1))").unwrap();
        assert_eq!(execute(&program, &[0xff, 41, 0x80]).unwrap(), vec![42]);
    }

    #[test]
    fn rejects_bad_source_with_compiler_context() {
        let error = compile("(mod (value) (undefined value))").unwrap_err();
        assert!(error.to_string().contains(super::COMPILER_VERSION));
    }

    #[test]
    fn rejects_malformed_source_without_panicking() {
        assert!(compile("(mod (value) (+ value 1)").is_err());
    }

    #[test]
    fn output_is_deterministic() {
        let source = "(mod () (list (list 51 0x01020304 42)))";
        assert_eq!(compile(source).unwrap(), compile(source).unwrap());
    }

    #[test]
    fn unchosen_raise_branch_remains_lazy() {
        let program = compile("(mod (value) (if value 42 (x)))").unwrap();
        assert_eq!(execute(&program, &[0xff, 1, 0x80]).unwrap(), vec![42]);
        assert!(execute_expected(&program, &[0xff, 0x80, 0x80], Some(FailureCode::Raise)).is_err());
        // Assert that nil itself serializes canonically in the VM bridge.
        let allocator = Allocator::new();
        assert_eq!(node_to_bytes(&allocator, NodePtr::NIL).unwrap(), vec![0x80]);
    }
}
