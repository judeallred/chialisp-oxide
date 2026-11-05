//! Backend calling-convention experiment, not Rust frontend recursion support.
use clvmr::{
    Allocator, ChiaDialect, MEMPOOL_MODE, NodePtr, error::EvalErr, run_program,
    serde::node_to_bytes,
};
use id_arena::Arena;
use num_bigint::BigUint;
use rue_lir::{CodegenOptions, Lir, LirId, first_path, rest_path};

const TABLE_PATH: u8 = 2;
const FIRST_ARGUMENT_PATH: u8 = 5;
const MAX_COST: u64 = 100_000_000;

#[derive(Clone, Copy, Debug)]
enum Recursion {
    SelfRecursive,
    Mutual,
}

fn list(arena: &mut Arena<Lir>, values: &[LirId]) -> LirId {
    let mut tail = arena.alloc(Lir::Atom(vec![]));
    for value in values.iter().rev() {
        tail = arena.alloc(Lir::Cons(*value, tail));
    }
    tail
}

fn function_path(index: usize) -> BigUint {
    let mut path = BigUint::from(TABLE_PATH);
    for _ in 0..index {
        path = rest_path(&path);
    }
    first_path(&path)
}

fn call(arena: &mut Arena<Lir>, function: usize, argument: LirId) -> LirId {
    // Helpers see (compiler-generated function table . proper argument list).
    // The callee comes from that closed table, never from the raw input tree.
    let table = arena.alloc(Lir::Path(TABLE_PATH.into()));
    let callee = arena.alloc(Lir::Path(function_path(function)));
    let arguments = list(arena, &[argument]);
    let environment = arena.alloc(Lir::Cons(table, arguments));
    arena.alloc(Lir::Run(callee, environment))
}

fn rightmost_body(arena: &mut Arena<Lir>, next_function: usize) -> LirId {
    let value = arena.alloc(Lir::Path(FIRST_ARGUMENT_PATH.into()));
    let rest = arena.alloc(Lir::Rest(value));
    let recurse = call(arena, next_function, rest);
    let is_pair = arena.alloc(Lir::Listp(value, true));
    arena.alloc(Lir::If(is_pair, recurse, value, false))
}

fn bootstrap(arena: &mut Arena<Lir>, functions: &[LirId]) -> LirId {
    // Bodies refer to table paths, not to each other's arena IDs. The resulting
    // LIR graph and serialized program are finite even for mutual recursion.
    let quoted = functions
        .iter()
        .map(|body| arena.alloc(Lir::Quote(*body)))
        .collect::<Vec<_>>();
    let table = list(arena, &quoted);
    let raw_environment = arena.alloc(Lir::Path(1u8.into()));
    let arguments = list(arena, &[raw_environment]);
    let frame = arena.alloc(Lir::Cons(table, arguments));
    let entry = arena.alloc(Lir::Quote(functions[0]));
    arena.alloc(Lir::Run(entry, frame))
}

fn compile(arena: &mut Arena<Lir>, mut root: LirId, optimize: bool) -> (Allocator, NodePtr) {
    if optimize {
        root = rue_lir::optimize(arena, root);
    }
    let mut allocator = Allocator::new();
    let code = rue_lir::codegen(
        arena,
        &mut allocator,
        root,
        CodegenOptions {
            optimize_static_pairs: false,
        },
    )
    .unwrap();
    (allocator, code)
}

#[test]
fn closed_runtime_calls_traverse_input_dependent_depth_without_unrolling_code() {
    for recursion in [Recursion::SelfRecursive, Recursion::Mutual] {
        for optimize in [false, true] {
            let mut arena = Arena::new();
            let functions = match recursion {
                Recursion::SelfRecursive => vec![rightmost_body(&mut arena, 0)],
                Recursion::Mutual => {
                    vec![rightmost_body(&mut arena, 1), rightmost_body(&mut arena, 0)]
                }
            };
            let root = bootstrap(&mut arena, &functions);
            let (mut allocator, code) = compile(&mut arena, root, optimize);
            let code_bytes = node_to_bytes(&allocator, code).unwrap();
            assert!(
                code_bytes.len() < 512,
                "the finite call graph must stay compact"
            );
            // 2 call graphs × 2 optimizer modes × 6 depths = 24 VM executions.
            for depth in [0, 1, 2, 8, 128, 2_048] {
                let leaf = allocator.new_atom(&[0, 0, 1, 255]).unwrap();
                let first = allocator.new_atom(&[128, 0]).unwrap();
                let mut environment = leaf;
                for _ in 0..depth {
                    environment = allocator.new_pair(first, environment).unwrap();
                }
                let result = run_program(
                    &mut allocator,
                    &ChiaDialect::new(MEMPOOL_MODE),
                    code,
                    environment,
                    MAX_COST,
                )
                .unwrap();
                assert_eq!(
                    node_to_bytes(&allocator, result.1).unwrap(),
                    [0x84, 0, 0, 1, 255],
                    "{recursion:?}, optimize={optimize}, depth={depth}: preserve every leaf byte",
                );
                eprintln!(
                    "{recursion:?}, optimize={optimize}, depth={depth}, bytes={}, cost={}",
                    code_bytes.len(),
                    result.0,
                );
            }
        }
    }
}

#[test]
fn nonterminating_runtime_calls_stop_at_the_vm_cost_limit() {
    for optimize in [false, true] {
        let mut arena = Arena::new();
        let argument = arena.alloc(Lir::Path(FIRST_ARGUMENT_PATH.into()));
        let body = call(&mut arena, 0, argument);
        let root = bootstrap(&mut arena, &[body]);
        let (mut allocator, code) = compile(&mut arena, root, optimize);
        let environment = allocator.new_atom(&[42]).unwrap();
        // Recursion occurs inside the VM. No native Rust function recurses.
        let error = run_program(
            &mut allocator,
            &ChiaDialect::new(MEMPOOL_MODE),
            code,
            environment,
            100_000,
        )
        .unwrap_err();
        assert!(
            matches!(error, EvalErr::CostExceeded),
            "optimize={optimize}: expected cost exhaustion, got {error:?}",
        );
    }
}
