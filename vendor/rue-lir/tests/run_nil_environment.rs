use clvmr::{Allocator, ChiaDialect, MEMPOOL_MODE, run_program, serde::node_to_bytes};
use id_arena::Arena;
use rue_lir::{CodegenOptions, Lir};

fn run(
    path: Option<u8>,
    nil_environment: bool,
    optimize: bool,
    outer_environment: &str,
) -> Option<Vec<u8>> {
    let mut arena = Arena::new();
    let body = arena.alloc(match path {
        Some(path) => Lir::Path(path.into()),
        None => Lir::Atom(vec![0, 1]),
    });
    let quoted = arena.alloc(Lir::Quote(body));
    let environment = arena.alloc(if nil_environment {
        Lir::Atom(vec![])
    } else {
        Lir::Path(1_u8.into())
    });
    let mut root = arena.alloc(Lir::Run(quoted, environment));
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
    let environment = rue_clvm::assemble(&mut allocator, outer_environment).unwrap();
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

fn serialized(tree: &str) -> Vec<u8> {
    let mut allocator = Allocator::new();
    let tree = rue_clvm::assemble(&mut allocator, tree).unwrap();
    node_to_bytes(&allocator, tree).unwrap()
}

const ENVIRONMENTS: [&str; 7] = [
    "()",
    "1",
    "0x00",
    "0x0001",
    "(1 . 2)",
    "((0x0001 . 2) . (3 . 4))",
    "(() . ())",
];

#[test]
fn quoted_identity_applied_to_nil_cannot_read_the_outer_environment() {
    for environment in ENVIRONMENTS {
        for optimize in [false, true] {
            assert_eq!(
                run(Some(1), true, optimize, environment),
                Some(serialized("()")),
                "optimize={optimize}, outer environment={environment}",
            );
        }
    }
}

#[test]
fn quoted_projections_applied_to_nil_keep_their_required_failure() {
    for path in [2, 3, 4, 5] {
        for environment in ENVIRONMENTS {
            for optimize in [false, true] {
                assert!(
                    run(Some(path), true, optimize, environment).is_none(),
                    "path={path}, optimize={optimize}, outer environment={environment} replaced nil with outer data",
                );
            }
        }
    }
}

#[test]
fn same_environment_and_constant_program_controls() {
    for environment in ENVIRONMENTS {
        for optimize in [false, true] {
            assert_eq!(
                run(Some(1), false, optimize, environment),
                Some(serialized(environment)),
            );
            for nil_environment in [false, true] {
                assert_eq!(
                    run(None, nil_environment, optimize, environment),
                    Some(serialized("0x0001")),
                );
            }
        }
    }
}
