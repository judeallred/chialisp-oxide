use std::path::{Path, PathBuf};

use chialisp_oxide_rue::compile_file;
use chialisp_oxide_vm::classify_failure;
use clvmr::{
    Allocator, ChiaDialect, MEMPOOL_MODE, run_program,
    serde::{node_from_bytes, node_to_bytes},
};
use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn execute(program: &[u8], solution: &str) -> Value {
    let mut allocator = Allocator::new();
    let program = node_from_bytes(&mut allocator, program).unwrap();
    let environment = rue_clvm::assemble(&mut allocator, solution).unwrap();
    match run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        environment,
        1_000_000,
    ) {
        Ok(output) => json!({
            "status": "accepted",
            "output_hex": hex::encode(node_to_bytes(&allocator, output.1).unwrap()),
        }),
        Err(error) => {
            let failure = classify_failure(&error);
            assert!(
                failure.is_program_rejection(),
                "inconclusive VM failure: {failure:?}: {error:?}"
            );
            json!({ "status": "rejected" })
        }
    }
}

#[test]
fn all_authored_rue_sources_match_recorded_native_outputs_and_rejections() {
    let report: Value =
        serde_json::from_slice(&std::fs::read(root().join("corpus/validation.json")).unwrap())
            .unwrap();
    let puzzles = report["puzzles"].as_array().unwrap();
    assert!(puzzles.len() >= 12);
    let mut executions = 0;
    for puzzle in puzzles {
        let slug = puzzle["slug"].as_str().unwrap();
        let path = root().join("puzzles").join(slug).join("puzzle.rue");
        for optimize in [false, true] {
            let program = compile_file(&path, optimize)
                .unwrap_or_else(|error| panic!("{slug} optimize={optimize}: {error:#}"));
            for case in puzzle["cases"].as_array().unwrap() {
                let solution = case["solution"].as_str().unwrap();
                // A declared divergence binds the authored Rue translation to an
                // independently specified original result instead of native Rust.
                let divergence = &case["declared_divergence"];
                let bound_to_original = divergence["backends"]
                    .as_array()
                    .is_some_and(|backends| backends.iter().any(|b| b == "rue_source"));
                let expected = if bound_to_original {
                    &divergence["outcome"]
                } else {
                    &case["native"]
                };
                assert_eq!(
                    &execute(&program, solution),
                    expected,
                    "{slug} optimize={optimize}, {}: {solution}",
                    case["label"],
                );
                executions += 1;
            }
        }
    }
    assert!(
        executions >= 194,
        "All 97 existing named cases in both modes"
    );
}

#[test]
fn actual_rue_lir_toggle_changes_a_nonconstant_policy() {
    let path = root().join("puzzles/capped-payment/puzzle.rue");
    let unoptimized = compile_file(&path, false).unwrap();
    let optimized = compile_file(&path, true).unwrap();
    assert_ne!(unoptimized, optimized);
    for solution in ["(128)", "(0x0080)", "(0x0001)", "(0x0000)", "(1 . 2)"] {
        assert_eq!(
            execute(&unoptimized, solution),
            execute(&optimized, solution),
            "LIR mode changed {solution}",
        );
    }
}
