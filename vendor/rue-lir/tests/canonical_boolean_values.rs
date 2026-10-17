use clvmr::{
    Allocator, ChiaDialect, MEMPOOL_MODE, error::EvalErr, run_program, serde::node_to_bytes,
};
use id_arena::Arena;
use rue_lir::{CodegenOptions, Lir, LirId};

#[derive(Clone, Copy, Debug)]
enum Producer {
    Path,
    Mask,
    Not,
    Eq,
    Gt,
    GtBytes,
    Listp,
    All,
    Any,
    Raise,
    NotRaise,
    ListpRaise,
    AllRaise,
    AnyRaise,
}

#[derive(Clone, Copy)]
enum Context {
    Value,
    EqualsTrue,
    Condition,
}

#[derive(Debug, PartialEq, Eq)]
enum Observation {
    Accepted(Vec<u8>),
    Raise,
    InvalidOpArg,
}

fn producer(arena: &mut Arena<Lir>, kind: Producer) -> LirId {
    let path = arena.alloc(Lir::Path(1_u8.into()));
    let nil = arena.alloc(Lir::Atom(vec![]));
    let one = arena.alloc(Lir::Atom(vec![1]));
    let six = arena.alloc(Lir::Atom(vec![6]));
    match kind {
        Producer::Path => path,
        Producer::Mask => arena.alloc(Lir::Logand(vec![path, six])),
        Producer::Not => arena.alloc(Lir::Not(path)),
        Producer::Eq => arena.alloc(Lir::Eq(path, one)),
        Producer::Gt => arena.alloc(Lir::Gt(path, one)),
        Producer::GtBytes => arena.alloc(Lir::GtBytes(path, one)),
        Producer::Listp => arena.alloc(Lir::Listp(path, true)),
        Producer::All => arena.alloc(Lir::All(vec![path, one])),
        Producer::Any => arena.alloc(Lir::Any(vec![path, nil])),
        Producer::Raise => arena.alloc(Lir::Raise(vec![])),
        Producer::NotRaise => {
            let failure = arena.alloc(Lir::Raise(vec![]));
            arena.alloc(Lir::Not(failure))
        }
        Producer::ListpRaise => {
            let failure = arena.alloc(Lir::Raise(vec![]));
            arena.alloc(Lir::Listp(failure, true))
        }
        Producer::AllRaise => {
            let failure = arena.alloc(Lir::Raise(vec![]));
            arena.alloc(Lir::All(vec![failure, one]))
        }
        Producer::AnyRaise => {
            let failure = arena.alloc(Lir::Raise(vec![]));
            arena.alloc(Lir::Any(vec![one, failure]))
        }
    }
}

fn compile(kind: Producer, double_not: bool, context: Context, optimize: bool) -> Vec<u8> {
    let mut arena = Arena::new();
    let mut root = producer(&mut arena, kind);
    if double_not {
        let inner = arena.alloc(Lir::Not(root));
        root = arena.alloc(Lir::Not(inner));
    }
    root = match context {
        Context::Value => root,
        Context::EqualsTrue => {
            let one = arena.alloc(Lir::Atom(vec![1]));
            arena.alloc(Lir::Eq(root, one))
        }
        Context::Condition => {
            let yes = arena.alloc(Lir::Atom(vec![42]));
            let no = arena.alloc(Lir::Atom(vec![17]));
            arena.alloc(Lir::If(root, yes, no, false))
        }
    };
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
    node_to_bytes(&allocator, program).unwrap()
}

fn run(bytes: &[u8], environment: &str) -> Observation {
    let mut allocator = Allocator::new();
    let program = clvmr::serde::node_from_bytes(&mut allocator, bytes).unwrap();
    let environment = rue_clvm::assemble(&mut allocator, environment).unwrap();
    match run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        environment,
        1_000_000,
    ) {
        Ok(output) => Observation::Accepted(node_to_bytes(&allocator, output.1).unwrap()),
        Err(EvalErr::Raise(_)) => Observation::Raise,
        Err(EvalErr::InvalidOpArg(_, _)) => Observation::InvalidOpArg,
        Err(error) => panic!("unexpected VM failure: {error:?}"),
    }
}

#[test]
fn double_not_returns_canonical_values_for_unknown_atoms_and_trees() {
    for context in [Context::Value, Context::EqualsTrue] {
        for environment in [
            "()", "1", "2", "3", "0x00", "0x0001", "0xff", "(1 . 2)", "(1 2)",
        ] {
            let expected = if environment == "()" {
                vec![0x80]
            } else {
                vec![1]
            };
            for optimize in [false, true] {
                assert_eq!(
                    run(
                        &compile(Producer::Path, true, context, optimize),
                        environment
                    ),
                    Observation::Accepted(expected.clone()),
                    "optimize={optimize}, environment={environment}"
                );
            }
        }
    }
}

#[test]
fn double_not_normalizes_integer_masks_instead_of_returning_the_mask() {
    for (environment, expected) in [
        ("0", 0x80),
        ("1", 0x80),
        ("2", 1),
        ("3", 1),
        ("4", 1),
        ("6", 1),
        ("-1", 1),
    ] {
        for optimize in [false, true] {
            assert_eq!(
                run(
                    &compile(Producer::Mask, true, Context::Value, optimize),
                    environment
                ),
                Observation::Accepted(vec![expected])
            );
        }
    }
}

#[test]
fn canonical_boolean_producers_keep_exact_values_and_safe_double_not_elision() {
    for kind in [
        Producer::Not,
        Producer::Eq,
        Producer::Gt,
        Producer::GtBytes,
        Producer::Listp,
        Producer::All,
        Producer::Any,
    ] {
        for environment in ["()", "1", "2", "0x00", "0x0001"] {
            let expected = run(&compile(kind, false, Context::Value, false), environment);
            for optimize in [false, true] {
                assert_eq!(
                    run(&compile(kind, true, Context::Value, optimize), environment),
                    expected,
                    "producer={kind:?},optimize={optimize}"
                );
            }
        }
        assert_eq!(
            compile(kind, true, Context::Value, true),
            compile(kind, false, Context::Value, true),
            "producer={kind:?}: safe elision was lost"
        );
    }
}

#[test]
fn truthiness_contexts_can_still_remove_boolean_normalization() {
    let normalized = compile(Producer::Path, true, Context::Condition, true);
    let direct = compile(Producer::Path, false, Context::Condition, true);
    assert_eq!(normalized, direct);
    for environment in ["()", "1", "2", "0x00", "(1 . 2)"] {
        let expected = if environment == "()" { 17 } else { 42 };
        for optimize in [false, true] {
            assert_eq!(
                run(
                    &compile(Producer::Path, true, Context::Condition, optimize),
                    environment
                ),
                Observation::Accepted(vec![expected])
            );
        }
    }
}

#[test]
fn normalization_keeps_required_program_failures() {
    for context in [Context::Value, Context::EqualsTrue, Context::Condition] {
        for optimize in [false, true] {
            for kind in [
                Producer::Raise,
                Producer::NotRaise,
                Producer::ListpRaise,
                Producer::AllRaise,
                Producer::AnyRaise,
            ] {
                assert_eq!(
                    run(&compile(kind, true, context, optimize), "()"),
                    Observation::Raise,
                    "producer={kind:?}"
                );
            }
            for kind in [
                Producer::Eq,
                Producer::Gt,
                Producer::GtBytes,
                Producer::Mask,
            ] {
                assert_eq!(
                    run(&compile(kind, true, context, optimize), "(1 . 2)"),
                    Observation::InvalidOpArg,
                    "producer={kind:?}"
                );
            }
        }
    }
}
