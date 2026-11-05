//! Rue annotations alone do not install this project's strict Rust ABI decoder.
//! These pair inputs deliberately violate Int. No typed Rust ABI bypass is claimed.
use std::path::Path;

use chialisp_oxide_rue::compile_file;
use chialisp_oxide_vm::{FailureCode, classify_failure};
use clvmr::{Allocator, ChiaDialect, MEMPOOL_MODE, run_program, serde::node_from_bytes};

fn check_fixture(fixture: &str, environments: &[&str], expected: FailureCode) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nested_bitwise")
        .join(fixture);
    for optimize in [false, true] {
        let bytes = compile_file(&path, optimize)
            .unwrap_or_else(|error| panic!("{fixture} optimize={optimize}: {error:#}"));
        for environment in environments {
            let mut allocator = Allocator::new();
            let program = node_from_bytes(&mut allocator, &bytes).unwrap();
            let input = rue_clvm::assemble(&mut allocator, environment).unwrap();
            let error = match run_program(
                &mut allocator,
                &ChiaDialect::new(MEMPOOL_MODE),
                program,
                input,
                1_000_000,
            ) {
                Err(error) => error,
                Ok(_) => panic!("{fixture} optimize={optimize}: unexpected acceptance"),
            };
            let failure = classify_failure(&error);
            assert_eq!(
                failure.code(),
                expected,
                "{fixture} optimize={optimize}, input={environment}: {error:?}",
            );
            assert_eq!(failure.kind(), expected.kind());
        }
    }
}

const PAIRS: [&str; 3] = ["(1 . 2)", "(() . ())", "(0x0001 . 0x00)"];

#[test]
fn nested_logand_rejects_malformed_external_pairs_before_recursive_sibling() {
    check_fixture("and.rue", &PAIRS, FailureCode::InvalidOpArg);
}

#[test]
fn nested_logior_rejects_malformed_external_pairs_before_recursive_sibling() {
    check_fixture("or.rue", &PAIRS, FailureCode::InvalidOpArg);
}

#[test]
fn explicit_source_type_guards_still_raise_in_both_modes() {
    for fixture in ["guarded_and.rue", "guarded_or.rue"] {
        check_fixture(fixture, &PAIRS, FailureCode::Raise);
    }
}

#[test]
fn well_typed_atoms_reach_cost_limit_without_becoming_policy_rejections() {
    for fixture in ["and.rue", "or.rue", "guarded_and.rue", "guarded_or.rue"] {
        check_fixture(
            fixture,
            &["()", "0x00", "0x0001", "0xff", "0x0080", "0x80"],
            FailureCode::CostExceeded,
        );
    }
}
