//! Future executed checks call the actual Cargo library, never copied policy.
use bumpalo::Bump;
use chialisp_oxide::Tree;
use chialisp_oxide_cat_genesis_tail::genesis_by_coin_id;
use chialisp_oxide_vm::classify_failure;
use clvmr::{
    Allocator, ChiaDialect, NodePtr, SExp,
    chia_dialect::MEMPOOL_MODE,
    run_program,
    serde::{node_from_bytes, node_to_bytes},
};
use serde_json::{Value, json};

fn borrowed<'a>(
    allocator: &Allocator,
    node: NodePtr,
    arena: &'a Bump,
    depth: usize,
    remaining: &mut usize,
) -> &'a Tree<'a> {
    // Fixture host-codec limits are outside the policy panic adapter. Exceeding
    // them fails the test harness, never confirms policy rejection.
    assert!(
        depth <= 64 && *remaining > 0,
        "bounded native fixture codec"
    );
    *remaining -= 1;
    let tree = match allocator.sexp(node) {
        SExp::Atom => Tree::Atom(arena.alloc_slice_copy(allocator.atom(node).as_ref())),
        SExp::Pair(first, rest) => Tree::Pair(
            borrowed(allocator, first, arena, depth + 1, remaining),
            borrowed(allocator, rest, arena, depth + 1, remaining),
        ),
    };
    arena.alloc(tree)
}

#[test]
fn actual_cargo_export_matches_independent_raw_tree_vectors() {
    let vectors: Value = serde_json::from_str(include_str!("../vectors.json")).unwrap();
    let rows = vectors["cases"].as_array().unwrap();
    assert!(!rows.is_empty() && rows.len() <= 128);
    for row in rows {
        let solution_hex = row["environment_hex"].as_str().unwrap();
        assert!(solution_hex.len() <= 65536, "fixture input byte bound");
        let bytes = hex::decode(solution_hex).unwrap();
        let mut allocator = Allocator::new_limited(8 * 1024 * 1024);
        let root = node_from_bytes(&mut allocator, &bytes).unwrap();
        let arena = Bump::new();
        let environment = borrowed(&allocator, root, &arena, 1, &mut 1024);
        let actual = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            genesis_by_coin_id(environment)
        })) {
            Ok(value) => {
                assert_eq!(value, Tree::Atom(&[]), "TAIL success must return nil");
                json!({"status":"accepted","output_hex":"80"})
            }
            Err(_) => json!({"status":"rejected"}),
        };
        assert_eq!(actual, row["expected"], "{}", row["name"]);
        println!(
            "CAT_GENESIS_NATIVE {}",
            json!({"name":row["name"],"actual":actual})
        );
    }
}

#[test]
fn exact_reference_source_compiles_and_executes_all_raw_tree_vectors() {
    let compiled = chialisp_oxide_reference::compile_with_includes(
        include_str!("../upstream/genesis_by_coin_id.clsp"),
        &[(
            "cat_truths.clib",
            include_str!("../upstream/cat_truths.clib"),
        )],
    )
    .unwrap();
    assert_eq!(
        compiled,
        hex::decode(include_str!("../upstream/genesis_by_coin_id.clsp.hex").trim()).unwrap()
    );
    assert_eq!(compiled.len(), 45);
    let vectors: Value = serde_json::from_str(include_str!("../vectors.json")).unwrap();
    let rows = vectors["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 37);
    for row in rows {
        let solution_hex = row["environment_hex"].as_str().unwrap();
        assert!(solution_hex.len() <= 65536, "fixture input byte bound");
        let solution = hex::decode(solution_hex).unwrap();
        let mut allocator = Allocator::new_limited(8 * 1024 * 1024);
        // Codec and infrastructure failures remain outside the policy result.
        let program = node_from_bytes(&mut allocator, &compiled).unwrap();
        let environment = node_from_bytes(&mut allocator, &solution).unwrap();
        let execution = match run_program(
            &mut allocator,
            &ChiaDialect::new(MEMPOOL_MODE),
            program,
            environment,
            1_000_000,
        ) {
            Ok(result) => {
                let output = node_to_bytes(&allocator, result.1).unwrap();
                let outcome = json!({"status":"accepted","output_hex":hex::encode(output)});
                assert_eq!(outcome, row["expected"], "{}", row["name"]);
                json!({"outcome":outcome,"cost":result.0,"failure":null})
            }
            Err(error) => {
                let failure = classify_failure(&error);
                assert!(
                    failure.is_program_rejection(),
                    "resource/infrastructure failure adds no rejection credit: {error}"
                );
                assert_eq!(row["expected"]["status"], "rejected", "{}", row["name"]);
                let typed = serde_json::to_value(failure).unwrap();
                assert_eq!(
                    typed["kind"], row["reference_failure_specification"]["kind"],
                    "{}",
                    row["name"]
                );
                assert_eq!(
                    typed["code"], row["reference_failure_specification"]["code"],
                    "{}",
                    row["name"]
                );
                json!({"outcome":{"status":"rejected"},"cost":null,"failure":typed})
            }
        };
        println!(
            "CAT_GENESIS_CANONICAL_VM {}",
            json!({"name":row["name"],"protocol":chialisp_oxide_vm::FAILURE_PROTOCOL,"max_cost":1_000_000,"execution":execution})
        );
    }
}
