//! Run small-stack controls in a child process so a regression reports a failed
//! assertion rather than aborting every unrelated library test.
use std::process::Command;

use clvmr::{Allocator, serde::node_to_bytes};
use id_arena::Arena;
use rue_lir::{CodegenOptions, Error, Lir};

fn child(mode: &str) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "codegen_stack_child", "--nocapture"])
        .env("RUE_CODEGEN_STACK_CHILD", mode)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{mode} child {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn moderately_deep_codegen_does_not_overflow_a_default_sized_thread() {
    child("not80");
}

#[test]
fn deep_program_and_static_data_use_heap_worklists() {
    child("not4096");
    child("static4096");
}

#[test]
fn codegen_stack_child() {
    let Some(mode) = std::env::var_os("RUE_CODEGEN_STACK_CHILD") else {
        return;
    };
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            let mut arena = Arena::new();
            if mode == "static4096" {
                let head = arena.alloc(Lir::Atom(vec![0x61]));
                let mut root = arena.alloc(Lir::Atom(vec![]));
                for _ in 0..4096 {
                    root = arena.alloc(Lir::Cons(head, root));
                }
                let mut allocator = Allocator::new();
                let program = rue_lir::codegen(
                    &arena,
                    &mut allocator,
                    root,
                    CodegenOptions {
                        optimize_static_pairs: true,
                    },
                )
                .unwrap();
                let mut expected = vec![0xff, 1];
                for _ in 0..4096 {
                    expected.extend([0xff, 0x61]);
                }
                expected.push(0x80);
                assert_eq!(node_to_bytes(&allocator, program).unwrap(), expected);
            } else {
                let depth = match mode.to_str().unwrap() {
                    "not80" => 80,
                    "not4096" => 4096,
                    other => panic!("unexpected child mode {other}"),
                };
                let mut root = arena.alloc(Lir::Path(1_u8.into()));
                for _ in 0..depth {
                    root = arena.alloc(Lir::Not(root));
                }
                // Independently build the serialization of nested (not ...) over
                // path 1. No optimizer or VM assembler supplies the expected bytes.
                let mut expected = Vec::new();
                for _ in 0..depth {
                    expected.extend([0xff, 0x20, 0xff]);
                }
                expected.push(1);
                expected.extend(std::iter::repeat_n(0x80, depth));
                for optimize_static_pairs in [false, true] {
                    let mut allocator = Allocator::new();
                    let program = rue_lir::codegen(
                        &arena,
                        &mut allocator,
                        root,
                        CodegenOptions {
                            optimize_static_pairs,
                        },
                    )
                    .unwrap();
                    assert_eq!(node_to_bytes(&allocator, program).unwrap(), expected);
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn repeated_dag_edges_keep_original_serialization_and_pair_allocation() {
    let mut arena = Arena::new();
    let path = arena.alloc(Lir::Path(1_u8.into()));
    let shared = arena.alloc(Lir::Not(path));
    let root = arena.alloc(Lir::Cons(shared, shared));
    for optimize_static_pairs in [false, true] {
        let mut allocator = Allocator::new();
        let program = rue_lir::codegen(
            &arena,
            &mut allocator,
            root,
            CodegenOptions {
                optimize_static_pairs,
            },
        )
        .unwrap();
        assert_eq!(
            node_to_bytes(&allocator, program).unwrap(),
            [
                0xff, 4, 0xff, 0xff, 32, 0xff, 1, 0x80, 0xff, 0xff, 32, 0xff, 1, 0x80, 0x80
            ]
        );
        // Two independent two-pair (not 1) programs and one three-pair cons
        // program. Memoizing emitted DAG nodes would reduce this to five.
        assert_eq!(allocator.pair_count(), 7);
    }
}

#[test]
fn cyclic_arenas_fail_with_a_controlled_codegen_error() {
    for static_cycle in [false, true] {
        let mut arena = Arena::new();
        let nil = arena.alloc(Lir::Atom(vec![]));
        let root = arena.alloc(Lir::Atom(vec![]));
        arena[root] = if static_cycle {
            Lir::Cons(nil, root)
        } else {
            Lir::Not(root)
        };
        for optimize_static_pairs in [false, true] {
            let error = rue_lir::codegen(
                &arena,
                &mut Allocator::new(),
                root,
                CodegenOptions {
                    optimize_static_pairs,
                },
            )
            .unwrap_err();
            assert!(matches!(error, Error::CodegenCycle));
        }
    }
}

#[test]
fn closure_initialization_and_each_wrapper_precede_the_next_capture() {
    for two_captures in [false, true] {
        let mut arena = Arena::new();
        let function = arena.alloc(Lir::Path(1_u8.into()));
        let large = arena.alloc(Lir::Atom(vec![0x61; 1024]));
        let captures = if two_captures {
            let small = arena.alloc(Lir::Atom(vec![0x62; 64]));
            vec![large, small] // The small last capture is emitted first.
        } else {
            vec![large]
        };
        let closure = arena.alloc(Lir::Closure(function, captures, true));
        let mut allocator = Allocator::new_limited(256);
        let error = rue_lir::codegen(&arena, &mut allocator, closure, CodegenOptions::default())
            .unwrap_err();
        assert!(matches!(
            error,
            Error::Clvm(clvmr::error::EvalErr::OutOfMemory)
        ));
        // Before the large atom fails: q1 has one pair. In the two-capture
        // case the small quoted atom and its complete fourteen-pair wrapper
        // have also been built. Emitting all children first violates this.
        assert_eq!(allocator.pair_count(), if two_captures { 16 } else { 1 });
        assert_eq!(
            allocator.allocated_heap_size(),
            if two_captures { 64 } else { 0 }
        );
    }
}
