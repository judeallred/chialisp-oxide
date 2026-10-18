//! Host-only allocation and executed fallback controls. The frozen artifact is
//! synthetic IR compiled with signed commit 4057921; it is not a puzzle port or
//! a compiler-preservation proof. The counting allocator belongs only to this
//! test binary, never to a Rust puzzle's admitted crate graph.
#[path = "runtime_evaluation/support.rs"]
#[allow(dead_code, reason = "Shared independent IR and VM test fixtures.")]
mod support;

use chialisp_oxide_ir::{Environment, Expr, Program, Type, compile, optimize};
use chialisp_oxide_vm::FailureCode;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
use support::*;

thread_local! {
    static LARGEST_ALLOCATION: Cell<Option<usize>> = const { Cell::new(None) };
}

fn record(size: usize) {
    // Allocations can occur during TLS destruction, when measurement is over.
    let _ = LARGEST_ALLOCATION.try_with(|slot| {
        if let Some(previous) = slot.get() {
            slot.set(Some(previous.max(size)));
        }
    });
}

struct CountingAllocator;
// SAFETY: Every allocation operation delegates unchanged to System. The
// thread-local counter never allocates, alters pointers or changes layouts.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: The caller supplies the GlobalAlloc-required layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: Delegation preserves the allocation's layout and zeroing.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: The caller supplies the original System pointer and layout.
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        // SAFETY: Pointer, original layout and new size pass through unchanged.
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct Measurement;
impl Drop for Measurement {
    fn drop(&mut self) {
        let _ = LARGEST_ALLOCATION.try_with(|slot| slot.set(None));
    }
}
fn measured<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    LARGEST_ALLOCATION.with(|slot| {
        assert!(slot.get().is_none(), "allocation measurements cannot nest");
        slot.set(Some(0));
    });
    let guard = Measurement;
    let result = operation();
    let largest = LARGEST_ALLOCATION.with(|slot| slot.get().unwrap());
    drop(guard);
    (result, largest)
}

#[test]
fn invalid_wide_ir_is_validated_before_the_runtime_worklist_is_allocated() {
    // An unvalidated uses_runtime scan previously copied these 32,768 child
    // references into a 256 KiB worklist before detecting the invalid width.
    let source = program(
        Environment::Ignored,
        vec![],
        Type::Unit,
        Expr::Tuple {
            items: vec![Expr::Bool { value: true }; 32_768],
        },
        vec![],
    );
    // Warm diagnostic/backtrace machinery outside the measured operation.
    assert!(
        source
            .validate()
            .unwrap_err()
            .to_string()
            .contains("tuple too large")
    );
    let (_, control) = measured(|| std::hint::black_box(vec![0_u8; 256 * 1024]));
    assert!(control >= 256 * 1024, "allocation control was not observed");
    for mode in [false, true] {
        let (result, largest) = measured(|| compile(&source, mode));
        assert!(result.unwrap_err().to_string().contains("tuple too large"));
        assert!(
            largest < 128 * 1024,
            "optimize={mode}: allocated {largest} bytes before rejecting invalid IR"
        );
    }
}

fn checked_casts(first: usize, count: usize, mut body: Expr) -> Expr {
    for id in (first..first + count).rev() {
        body = binding(
            id,
            Type::Atom,
            Expr::TreeAtomValue {
                value: Box::new(arg(0)),
            },
            body,
        );
    }
    body
}

fn near_expansion_limit() -> Program {
    program(
        Environment::Raw,
        vec![Type::Tree],
        Type::Unit,
        checked_casts(0, 44, call(0, vec![arg(0)])),
        vec![function(
            0,
            vec![Type::Tree],
            Type::Unit,
            checked_casts(100, 4, Expr::Unit),
        )],
    )
}

#[test]
fn excessive_inlined_continuations_retry_the_exact_previous_optimized_program() {
    let source = near_expansion_limit();
    let candidate = optimize(&source).unwrap();
    assert!(
        candidate.functions.is_empty(),
        "control must actually inline and prune"
    );
    // This input is already inlined: its new plan has no previous function
    // table to recover. The same compiler must expose the real resource limit.
    let error = compile(&candidate, true).unwrap_err();
    assert!(error.to_string().contains("CLVM expansion nesting limit"));

    let on = compile(&source, true).expect("optional inlining must not lose the previous route");
    let previous = hex::decode(include_str!("fixtures/inlining-limit-405.hex").trim()).unwrap();
    assert_eq!(on, previous, "fallback must retain exact frozen-405 bytes");
    let off = compile(&source, false).unwrap();
    for (mode, code) in [(false, off), (true, on)] {
        for input in [atom(&[]), atom(&[0]), atom(&[0, 0, 1])] {
            let cost = accepted(run(&code, &input, FINITE_LIMIT), &atom(&[]));
            assert_eq!(cost, if mode { 39_457 } else { 53_338 });
        }
        failed(
            run(&code, &pair(&atom(&[]), &atom(&[])), LIMIT),
            FailureCode::Raise,
        );
    }
}

#[test]
fn invalid_ir_is_never_rescued_by_a_previous_optimized_route() {
    let mut source = near_expansion_limit();
    source.functions[0].body = arg(1);
    for mode in [false, true] {
        let message = compile(&source, mode).unwrap_err().to_string();
        assert!(message.contains("invalid argument 1"), "{message}");
        assert!(!message.contains("expansion"));
    }
}
