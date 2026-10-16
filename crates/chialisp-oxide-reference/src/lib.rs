//! Compile independent Chialisp references using Chia Network's Rust compiler.
//!
//! The compiler's CLVM allocator is deliberately confined to this crate. Serialized
//! bytes form the bridge to the project's pinned execution VM; this prevents an
//! accidental comparison against a different execution dialect or VM version.

use std::{collections::HashMap, rc::Rc};

use anyhow::{Context, Result, anyhow};
use chialisp::{
    classic::clvm_tools::clvmc::compile_clvm_text,
    compiler::{compiler::DefaultCompilerOpts, comptypes::CompilerOpts},
};
use clvmr_reference::{allocator::Allocator, serde::node_to_bytes};

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

#[cfg(test)]
mod tests {
    use clvmr::{
        ChiaDialect,
        allocator::{Allocator, NodePtr},
        chia_dialect::MEMPOOL_MODE,
        run_program,
        serde::{node_from_bytes, node_to_bytes},
    };

    use super::compile;

    /// Run serialized output through the project's VM, not the compiler's VM.
    fn execute(program: &[u8], solution: &[u8]) -> Result<Vec<u8>, String> {
        let mut allocator = Allocator::new();
        let program = node_from_bytes(&mut allocator, program).map_err(|e| e.to_string())?;
        let solution = node_from_bytes(&mut allocator, solution).map_err(|e| e.to_string())?;
        let result = run_program(
            &mut allocator,
            &ChiaDialect::new(MEMPOOL_MODE),
            program,
            solution,
            1_000_000,
        )
        .map_err(|e| e.to_string())?;
        node_to_bytes(&allocator, result.1).map_err(|e| e.to_string())
    }

    #[test]
    fn compiles_and_runs_arithmetic_in_project_vm() {
        let program = compile("(mod (value) (* value 2))").unwrap();
        // Proper list (21), with result 42.
        assert_eq!(execute(&program, &[0xff, 21, 0x80]).unwrap(), vec![42]);
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
            assert!(execute(&program, &[0xff, value, 0x80]).is_err());
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
        assert!(execute(&program, &[0xff, 0x80, 0x80]).is_err());
        // Assert that nil itself serializes canonically in the VM bridge.
        let allocator = Allocator::new();
        assert_eq!(node_to_bytes(&allocator, NodePtr::NIL).unwrap(), vec![0x80]);
    }
}
