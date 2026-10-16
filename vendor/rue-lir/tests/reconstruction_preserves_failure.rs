use clvmr::{Allocator, ChiaDialect, MEMPOOL_MODE, run_program, serde::node_to_bytes};
use id_arena::Arena;
use rue_lir::{CodegenOptions, Lir, first_path, rest_path};

fn reconstruct(
    parent: u8,
    projections: bool,
    optimize: bool,
    environment: &str,
) -> Option<Vec<u8>> {
    let mut arena = Arena::new();
    let path = parent.into();
    let (first, rest) = if projections {
        let value = arena.alloc(Lir::Path(path));
        (
            arena.alloc(Lir::First(value)),
            arena.alloc(Lir::Rest(value)),
        )
    } else {
        (
            arena.alloc(Lir::Path(first_path(&path))),
            arena.alloc(Lir::Path(rest_path(&path))),
        )
    };
    let mut root = arena.alloc(Lir::Cons(first, rest));
    if optimize {
        root = rue_lir::optimize(&mut arena, root);
    }
    let mut allocator = Allocator::new();
    let program = rue_lir::codegen(
        &arena,
        &mut allocator,
        root,
        CodegenOptions {
            optimize_static_pairs: false,
        },
    )
    .unwrap();
    let environment = rue_clvm::assemble(&mut allocator, environment).unwrap();
    run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        environment,
        1_000_000,
    )
    .ok()
    .map(|result| node_to_bytes(&allocator, result.1).unwrap())
}

#[test]
fn reconstructing_a_pair_preserves_projection_failure_on_atoms() {
    for parent in [1, 2, 3] {
        for projections in [false, true] {
            for selected in ["()", "1", "0x00", "0x0001", "0xff"] {
                let environment = match parent {
                    1 => selected.into(),
                    2 => format!("({selected} . 7)"),
                    3 => format!("(7 . {selected})"),
                    _ => unreachable!(),
                };
                for optimize in [false, true] {
                    assert!(
                        reconstruct(parent, projections, optimize, &environment).is_none(),
                        "parent={parent}, projections={projections}, optimize={optimize}, environment={environment} discarded a required pair check",
                    );
                }
            }
        }
    }
}

#[test]
fn reconstructing_a_valid_pair_preserves_its_exact_atom_bytes_and_tail() {
    for parent in [1, 2, 3] {
        for projections in [false, true] {
            for selected in [
                "(1 . 2)",
                "(0x0001 . 0x00)",
                "((1 . 2) . (3 . 4))",
                "(() . ())",
            ] {
                let environment = match parent {
                    1 => selected.into(),
                    2 => format!("({selected} . 7)"),
                    3 => format!("(7 . {selected})"),
                    _ => unreachable!(),
                };
                let mut allocator = Allocator::new();
                let expected = rue_clvm::assemble(&mut allocator, selected).unwrap();
                let expected = node_to_bytes(&allocator, expected).unwrap();
                for optimize in [false, true] {
                    assert_eq!(
                        reconstruct(parent, projections, optimize, &environment),
                        Some(expected.clone())
                    );
                }
            }
        }
    }
}
