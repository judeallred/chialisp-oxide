//! Bounded VM executions, with explicitly separate classifier-injection controls.
//!
//! The allocator caps and timeout interval here are from pinned clvmr 0.21.0.
//! Ghost reservations exercise its real limit checks without allocating millions
//! of nodes; they do not simulate the resulting EvalErr in the VM tests.
use std::time::Duration;

use chialisp_oxide_vm::{FailureCode, FailureKind, classify_failure};
use clvmr::{
    Allocator, ChiaDialect, MEMPOOL_MODE, NodePtr, SExp, error::EvalErr, run_program,
    run_program_with_timeout,
};

const NODE_LIMIT: usize = 62_500_000;
const ORDINARY_COST_LIMIT: u64 = 1_000_000;

fn list(allocator: &mut Allocator, values: &[NodePtr]) -> NodePtr {
    values.iter().rev().fold(NodePtr::NIL, |rest, first| {
        allocator.new_pair(*first, rest).unwrap()
    })
}

fn quote(allocator: &mut Allocator, value: NodePtr) -> NodePtr {
    allocator.new_pair(allocator.one(), value).unwrap()
}

fn call(allocator: &mut Allocator, opcode: u32, arguments: &[NodePtr]) -> NodePtr {
    let opcode = allocator.new_small_number(opcode).unwrap();
    let arguments = list(allocator, arguments);
    allocator.new_pair(opcode, arguments).unwrap()
}

fn assert_failure(error: &EvalErr, kind: FailureKind, code: FailureCode) {
    let failure = classify_failure(error);
    assert_eq!(failure.code(), code, "actual error: {error:?}");
    assert_eq!(failure.kind(), kind, "actual error: {error:?}");
    assert_eq!(failure.is_program_rejection(), kind == FailureKind::Program);
}

#[test]
fn executed_raise_and_checked_projection_are_program_failures() {
    let mut allocator = Allocator::new();
    let raise = call(&mut allocator, 8, &[]);
    let error = run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        raise,
        NodePtr::NIL,
        ORDINARY_COST_LIMIT,
    )
    .unwrap_err();
    assert_failure(&error, FailureKind::Program, FailureCode::Raise);

    // A path is checked against the environment, not treated as a free value.
    let first_path = allocator.new_small_number(2).unwrap();
    let error = run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        first_path,
        NodePtr::NIL,
        ORDINARY_COST_LIMIT,
    )
    .unwrap_err();
    assert_failure(&error, FailureKind::Program, FailureCode::PathIntoAtom);
    let pair = allocator.new_pair(allocator.one(), NodePtr::NIL).unwrap();
    let result = run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        first_path,
        pair,
        ORDINARY_COST_LIMIT,
    )
    .unwrap();
    assert_eq!(result.1, allocator.one());
}

#[test]
fn executed_operand_validation_is_distinct_from_resource_exhaustion() {
    let mut allocator = Allocator::new();
    let text = allocator.new_atom(b"a").unwrap();
    let text = quote(&mut allocator, text);
    let two = allocator.new_small_number(2).unwrap();
    let two = quote(&mut allocator, two);
    // (substr (q . a) (q . 2)) has a start beyond the one-byte string.
    // op_substr detects this before calling the allocator's bounds checker.
    let substr = call(&mut allocator, 12, &[text, two]);
    let error = run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        substr,
        NodePtr::NIL,
        ORDINARY_COST_LIMIT,
    )
    .unwrap_err();
    assert_failure(&error, FailureKind::Program, FailureCode::InvalidOpArg);

    let one = allocator.one();
    let one = quote(&mut allocator, one);
    let excessive_shift = allocator.new_small_number(65_536).unwrap();
    let excessive_shift = quote(&mut allocator, excessive_shift);
    let shift = call(&mut allocator, 22, &[one, excessive_shift]);
    let error = run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        shift,
        NodePtr::NIL,
        ORDINARY_COST_LIMIT,
    )
    .unwrap_err();
    assert_failure(&error, FailureKind::Program, FailureCode::ShiftTooLarge);

    // An actual allocator bounds error is also operand validation in this pin.
    // This is an allocator API control, not a claimed VM-reachable substr path.
    let atom = allocator.new_atom(b"a").unwrap();
    let error = allocator.new_substr(atom, 2, 2).unwrap_err();
    assert_failure(&error, FailureKind::Program, FailureCode::InvalidAllocArg);
}

fn concat_program(allocator: &mut Allocator) -> NodePtr {
    let value = allocator.new_atom(&[0x80; 16]).unwrap();
    let value = quote(allocator, value);
    call(allocator, 14, &[value, value])
}

#[test]
fn executed_concat_runs_out_of_the_configured_heap_not_policy() {
    let mut limited = Allocator::new_limited(32);
    let program = concat_program(&mut limited);
    assert!(
        limited.heap_size() < 32,
        "program construction fits the heap"
    );
    let error = run_program(
        &mut limited,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        NodePtr::NIL,
        ORDINARY_COST_LIMIT,
    )
    .unwrap_err();
    assert_failure(&error, FailureKind::Resource, FailureCode::OutOfMemory);

    let mut sufficient = Allocator::new_limited(256);
    let program = concat_program(&mut sufficient);
    let result = run_program(
        &mut sufficient,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        NodePtr::NIL,
        ORDINARY_COST_LIMIT,
    )
    .unwrap();
    assert_eq!(sufficient.atom(result.1).as_ref(), &[0x80; 32]);
    eprintln!(
        "concat: heap=32 -> OutOfMemory; heap=256 -> 32-byte output, cost={}",
        result.0,
    );
}

#[test]
fn executed_pair_cap_uses_ghost_accounting_and_recovers_at_a_checkpoint() {
    let mut allocator = Allocator::new();
    let one = allocator.one();
    let one = quote(&mut allocator, one);
    let nil = quote(&mut allocator, NodePtr::NIL);
    let program = call(&mut allocator, 4, &[one, nil]);
    let checkpoint = allocator.checkpoint();
    allocator
        .add_ghost_pair(NODE_LIMIT - allocator.pair_count())
        .unwrap();
    assert_eq!(allocator.pair_count(), NODE_LIMIT);
    assert!(allocator.allocated_pair_count() < 16);
    let physical_pairs = allocator.allocated_pair_count();
    let error = run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        NodePtr::NIL,
        ORDINARY_COST_LIMIT,
    )
    .unwrap_err();
    assert_failure(&error, FailureKind::Resource, FailureCode::TooManyPairs);

    allocator.restore_checkpoint(&checkpoint);
    let result = run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        NodePtr::NIL,
        ORDINARY_COST_LIMIT,
    )
    .unwrap();
    assert!(matches!(allocator.sexp(result.1), SExp::Pair(first, rest)
        if first == allocator.one() && rest == NodePtr::NIL));
    eprintln!(
        "pair cap: logical={NODE_LIMIT}, physical={physical_pairs} -> TooManyPairs; restored cost={}",
        result.0,
    );
}

#[test]
fn executed_atom_cap_allows_vm_start_then_rejects_a_new_atom() {
    let mut allocator = Allocator::new();
    let program = concat_program(&mut allocator);
    let checkpoint = allocator.checkpoint();
    // run_program itself reserves one ghost atom for its historical max-cost
    // allocation. Leave that slot free so execution reaches concat's check.
    allocator
        .add_ghost_atom(NODE_LIMIT - allocator.atom_count() - 1)
        .unwrap();
    assert_eq!(allocator.atom_count(), NODE_LIMIT - 1);
    assert!(allocator.allocated_atom_count() < 16);
    let physical_atoms = allocator.allocated_atom_count();
    let error = run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        NodePtr::NIL,
        ORDINARY_COST_LIMIT,
    )
    .unwrap_err();
    assert_failure(&error, FailureKind::Resource, FailureCode::TooManyAtoms);
    assert_eq!(allocator.atom_count(), NODE_LIMIT);

    allocator.restore_checkpoint(&checkpoint);
    let result = run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        NodePtr::NIL,
        ORDINARY_COST_LIMIT,
    )
    .unwrap();
    assert_eq!(allocator.atom(result.1).as_ref(), &[0x80; 32]);
    eprintln!(
        "atom cap: logical={NODE_LIMIT}, physical={physical_atoms} -> TooManyAtoms; restored cost={}",
        result.0,
    );
}

fn self_application(allocator: &mut Allocator) -> NodePtr {
    // (a 1 1) applied to itself repeats inside the iterative CLVM evaluator.
    // No native function recursively calls itself.
    let identity_path = allocator.one();
    call(allocator, 2, &[identity_path, identity_path])
}

#[test]
fn executed_nontermination_is_bounded_by_cost_or_wall_clock() {
    let mut allocator = Allocator::new();
    let program = self_application(&mut allocator);
    let error = run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        program,
        25_000,
    )
    .unwrap_err();
    assert_failure(&error, FailureKind::Resource, FailureCode::CostExceeded);

    let mut allocator = Allocator::new();
    let program = self_application(&mut allocator);
    // Timeout is checked every 1M cost units in this pin. The independent 5M
    // budget bounds execution even if the timeout mechanism regresses.
    let error = run_program_with_timeout(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        program,
        5_000_000,
        Duration::ZERO,
    )
    .unwrap_err();
    assert_failure(&error, FailureKind::Resource, FailureCode::Timeout);
    assert!(allocator.allocated_pair_count() < 100_000);
    eprintln!(
        "self-application: cost budget=25000 -> CostExceeded; timeout=0, cost budget=5000000 -> Timeout, physical pairs={}",
        allocator.allocated_pair_count(),
    );
}

#[test]
fn explicit_injection_controls_cover_stack_limits_without_huge_stacks() {
    // These are classifier controls, not executed VM stack-exhaustion tests.
    // The pinned value/environment limits require 20M entries to reach.
    for (error, code) in [
        (
            EvalErr::ValueStackLimitReached(NodePtr::NIL),
            FailureCode::ValueStackLimitReached,
        ),
        (
            EvalErr::EnvironmentStackLimitReached(NodePtr::NIL),
            FailureCode::EnvironmentStackLimitReached,
        ),
        (
            EvalErr::SoftforkStackDepthExceeded,
            FailureCode::SoftforkStackDepthExceeded,
        ),
    ] {
        assert_failure(&error, FailureKind::Resource, code);
    }
}

#[test]
fn explicit_injection_controls_keep_codec_and_internal_errors_infrastructure() {
    for (error, code) in [
        (EvalErr::SerializationError, FailureCode::SerializationError),
        (
            EvalErr::NonCanonicalSerialization,
            FailureCode::NonCanonicalSerialization,
        ),
        (
            EvalErr::SerializationBackreferenceError,
            FailureCode::SerializationBackreferenceError,
        ),
        (
            EvalErr::InternalError(NodePtr::NIL, "injected internal failure".into()),
            FailureCode::InternalError,
        ),
    ] {
        assert_failure(&error, FailureKind::Infrastructure, code);
    }
}
