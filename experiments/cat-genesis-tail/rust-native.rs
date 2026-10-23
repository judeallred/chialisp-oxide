//! Native actual-source control observations; independent vector expectations.
use bumpalo::Bump;
use chialisp_oxide::Tree;
use clvmr::{Allocator, NodePtr, SExp, serde::node_from_bytes};
use serde_json::{Value, json};

fn borrowed<'a>(
    a: &Allocator,
    n: NodePtr,
    arena: &'a Bump,
    depth: usize,
    remaining: &mut usize,
) -> &'a Tree<'a> {
    assert!(
        depth <= 64 && *remaining > 0,
        "host fixture bound outside policy adapter"
    );
    *remaining -= 1;
    let value = match a.sexp(n) {
        SExp::Atom => Tree::Atom(arena.alloc_slice_copy(a.atom(n).as_ref())),
        SExp::Pair(f, r) => Tree::Pair(
            borrowed(a, f, arena, depth + 1, remaining),
            borrowed(a, r, arena, depth + 1, remaining),
        ),
    };
    arena.alloc(value)
}

#[test]
fn actual_source_native_vectors_and_mutant_witnesses() {
    let vectors: Value = serde_json::from_str(include_str!("../vectors.json")).unwrap();
    let rows = vectors["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 37);
    for control in ["original", "drop_delta_guard", "drop_genesis_equality"] {
        for row in rows {
            let name = row["name"].as_str().unwrap();
            if control != "original"
                && name
                    != if control == "drop_delta_guard" {
                        "non-nil-delta-one"
                    } else {
                        "mismatched-parent"
                    }
            {
                continue;
            }
            let hex_input = row["environment_hex"].as_str().unwrap();
            assert!(hex_input.len() <= 65536);
            let bytes = hex::decode(hex_input).unwrap();
            let mut a = Allocator::new_limited(8 * 1024 * 1024);
            let node = node_from_bytes(&mut a, &bytes).unwrap();
            let arena = Bump::new();
            let environment = borrowed(&a, node, &arena, 1, &mut 1024);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match control {
                "original" => chialisp_oxide_cat_genesis_controls::original::puzzle(environment),
                "drop_delta_guard" => {
                    chialisp_oxide_cat_genesis_controls::drop_delta_guard::puzzle(environment)
                }
                _ => {
                    chialisp_oxide_cat_genesis_controls::drop_genesis_equality::puzzle(environment)
                }
            }));
            let actual = match result {
                Ok(value) => {
                    assert_eq!(value, Tree::Atom(&[]));
                    json!({"status":"accepted","output_hex":"80"})
                }
                Err(_) => json!({"status":"rejected"}),
            };
            let expected = if control == "original" {
                row["expected"].clone()
            } else {
                json!({"status":"accepted","output_hex":"80"})
            };
            assert_eq!(actual, expected, "{control}/{name}");
            println!(
                "CAT_GENESIS_RUST_NATIVE {}",
                json!({"control":control,"name":name,"actual":actual})
            );
        }
    }
}
