//! Canonical Boolean results from the actual pinned Rue frontend adapter.
use std::path::Path;

use chialisp_oxide_rue::compile_file;
use chialisp_oxide_vm::classify_failure;
use clvmr::{
    Allocator, ChiaDialect, MEMPOOL_MODE, run_program,
    serde::{node_from_bytes, node_to_bytes},
};

fn execute(fixture: &str, optimize: bool, input: i16) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/canonical_bool")
        .join(fixture);
    let bytes = compile_file(&path, optimize)
        .unwrap_or_else(|error| panic!("{fixture} optimize={optimize}: {error:#}"));
    let mut allocator = Allocator::new();
    let program = node_from_bytes(&mut allocator, &bytes).unwrap();
    let environment = allocator.new_number(input.into()).unwrap();
    let result = match run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        environment,
        1_000_000,
    ) {
        Ok(result) => result,
        Err(error) => panic!(
            "{fixture} optimize={optimize}, input={input}: unexpected {:?}: {error:?}",
            classify_failure(&error)
        ),
    };
    node_to_bytes(&allocator, result.1).unwrap()
}

fn bool_bytes(value: bool) -> Vec<u8> {
    vec![if value { 1 } else { 0x80 }]
}

#[test]
fn returned_nonzero_comparisons_are_canonical_booleans() {
    for value in [-129, -128, -2, -1, 0, 1, 2, 3, 6, 127, 128, 255, 256] {
        for optimize in [false, true] {
            assert_eq!(
                execute("nonzero.rue", optimize, value),
                bool_bytes(value != 0),
                "input={value},optimize={optimize}"
            );
        }
    }
}

#[test]
fn comparing_a_nonzero_result_to_true_preserves_the_boolean_answer() {
    for value in [-2, -1, 0, 1, 2, 3, 6, 128] {
        for optimize in [false, true] {
            assert_eq!(
                execute("equal_true.rue", optimize, value),
                bool_bytes(value != 0),
                "input={value},optimize={optimize}"
            );
        }
    }
}

#[test]
fn integer_masks_return_boolean_values() {
    for value in -2..=8 {
        for optimize in [false, true] {
            assert_eq!(
                execute("mask.rue", optimize, value),
                bool_bytes(value & 6 != 0)
            );
        }
    }
}

#[test]
fn tuple_fields_and_eager_boolean_operations_preserve_their_contracts() {
    for value in 0..=7 {
        for optimize in [false, true] {
            let mut expected = vec![0xff];
            expected.extend(bool_bytes(value & 2 != 0));
            expected.extend(bool_bytes(value & 1 != 0));
            assert_eq!(execute("tuple.rue", optimize, value), expected);
            assert_eq!(
                execute("nested.rue", optimize, value),
                bool_bytes(value & 2 != 0 && value & 1 == 0)
            );
        }
    }
}

#[test]
fn condition_only_uses_keep_the_same_branch() {
    for value in [-2, -1, 0, 1, 2, 3, 128] {
        for optimize in [false, true] {
            assert_eq!(
                execute("condition.rue", optimize, value),
                vec![if value == 0 { 17 } else { 42 }]
            );
        }
    }
}
