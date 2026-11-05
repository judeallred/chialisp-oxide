#![forbid(unsafe_code)]
//! Dynamic application of an untrusted CLVM program to an untrusted environment.
//!
//! `apply(program, environment)` is CLVM's `a` operator as a Rust function. In a
//! compiled puzzle the reviewed compiler bridge lowers actual calls of this
//! reserved export to opcode 2 after checking its identity, exact signature and
//! implementation fingerprint; the export name grants nothing. Both operands are
//! evaluated once, in order, before the application; a failing applied program
//! fails the whole puzzle, and an unused result is still evaluated.
//!
//! Natively this crate is `no_std` and cannot evaluate CLVM. Without the `vm`
//! feature the function panics, which the compiler bridge fingerprints as the
//! reviewed body. The `vm` feature evaluates through the pinned VM so that host
//! tests and the corpus oracle can run puzzles that apply inner programs.
//! Neither route establishes termination, cost bounds, authorization or
//! compiler preservation for the applied program.
use crate::Tree;

/// Apply `program` to `environment` as CLVM's `a` operator does.
///
/// The applied program's result is an arbitrary tree. Its failure rejects the
/// caller. Host resource exhaustion in the native evaluator is a host limit,
/// reported through [`ResourceExhausted`], never a policy rejection.
#[cfg(not(feature = "vm"))]
pub fn apply<'a>(_program: &Tree<'a>, _environment: &Tree<'a>) -> Tree<'static> {
    // A bare panic lowers to one `panic` lang-item call that the compiler
    // bridge fingerprints; a message would pull in `fmt::Arguments`.
    panic!()
}

/// Apply `program` to `environment` as CLVM's `a` operator does.
///
/// Evaluates through the pinned VM in mempool mode with a fixed native cost
/// ceiling. A program failure panics (a policy rejection); exceeding the native
/// ceiling or a host allocation limit panics with a [`ResourceExhausted`]
/// payload, which oracles must report as inconclusive rather than rejection.
#[cfg(feature = "vm")]
pub fn apply<'a>(program: &Tree<'a>, environment: &Tree<'a>) -> Tree<'static> {
    vm::apply(program, environment)
}

/// Host resource exhaustion inside the native evaluator; not a policy result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceExhausted {
    /// The native cost ceiling or host allocation bound that was exceeded.
    pub limit: u64,
}

#[cfg(feature = "vm")]
mod vm {
    extern crate std;
    use super::ResourceExhausted;
    use crate::Tree;
    use chialisp_oxide_vm::classify_failure;
    use clvmr::{Allocator, ChiaDialect, MEMPOOL_MODE, NodePtr, allocator::SExp, run_program};
    use std::boxed::Box;
    use std::vec::Vec;

    /// Fixed native evaluation ceiling. The VM charges the applied program
    /// against the outer puzzle's budget; a native oracle has no such budget,
    /// so this is deliberately generous and its exhaustion is inconclusive.
    pub const NATIVE_MAX_COST: u64 = 10_000_000_000;
    /// Bound on nodes materialized for inputs and the result.
    pub const MAX_NODES: usize = 4_000_000;

    fn exhausted(limit: u64) -> ! {
        std::panic::panic_any(ResourceExhausted { limit })
    }

    fn encode(a: &mut Allocator, tree: &Tree<'_>, budget: &mut usize) -> NodePtr {
        // Iterative post-order encoding; deep right spines are ordinary data.
        enum Step<'t, 'a> {
            Visit(&'t Tree<'a>),
            Pair,
        }
        let mut steps = Vec::new();
        let mut values: Vec<NodePtr> = Vec::new();
        steps.push(Step::Visit(tree));
        while let Some(step) = steps.pop() {
            match step {
                Step::Visit(Tree::Atom(bytes)) => {
                    *budget = budget
                        .checked_sub(1)
                        .unwrap_or_else(|| exhausted(MAX_NODES as u64));
                    values.push(
                        a.new_atom(bytes)
                            .unwrap_or_else(|_| exhausted(MAX_NODES as u64)),
                    );
                }
                Step::Visit(Tree::Pair(first, rest)) => {
                    *budget = budget
                        .checked_sub(1)
                        .unwrap_or_else(|| exhausted(MAX_NODES as u64));
                    steps.push(Step::Pair);
                    steps.push(Step::Visit(rest));
                    steps.push(Step::Visit(first));
                }
                Step::Pair => {
                    let rest = values.pop().expect("pair rest");
                    let first = values.pop().expect("pair first");
                    values.push(
                        a.new_pair(first, rest)
                            .unwrap_or_else(|_| exhausted(MAX_NODES as u64)),
                    );
                }
            }
        }
        values.pop().expect("encoded root")
    }

    fn decode(a: &Allocator, node: NodePtr, budget: &mut usize) -> Tree<'static> {
        // Iterative decoding into leaked boxes: the result must outlive its
        // inputs, and this evaluator exists only for host tests and oracles.
        enum Step {
            Visit(NodePtr),
            Pair,
        }
        let mut steps = Vec::new();
        let mut values: Vec<Tree<'static>> = Vec::new();
        steps.push(Step::Visit(node));
        while let Some(step) = steps.pop() {
            match step {
                Step::Visit(node) => {
                    *budget = budget
                        .checked_sub(1)
                        .unwrap_or_else(|| exhausted(MAX_NODES as u64));
                    match a.sexp(node) {
                        SExp::Atom => {
                            let bytes: &'static [u8] =
                                Box::leak(a.atom(node).as_ref().to_vec().into_boxed_slice());
                            values.push(Tree::Atom(bytes));
                        }
                        SExp::Pair(first, rest) => {
                            steps.push(Step::Pair);
                            steps.push(Step::Visit(rest));
                            steps.push(Step::Visit(first));
                        }
                    }
                }
                Step::Pair => {
                    let rest = values.pop().expect("pair rest");
                    let first = values.pop().expect("pair first");
                    let first: &'static Tree<'static> = Box::leak(Box::new(first));
                    let rest: &'static Tree<'static> = Box::leak(Box::new(rest));
                    values.push(Tree::Pair(first, rest));
                }
            }
        }
        values.pop().expect("decoded root")
    }

    pub fn apply<'a>(program: &Tree<'a>, environment: &Tree<'a>) -> Tree<'static> {
        let mut a = Allocator::new();
        let mut budget = MAX_NODES;
        let program = encode(&mut a, program, &mut budget);
        let environment = encode(&mut a, environment, &mut budget);
        match run_program(
            &mut a,
            &ChiaDialect::new(MEMPOOL_MODE),
            program,
            environment,
            NATIVE_MAX_COST,
        ) {
            Ok(reduction) => decode(&a, reduction.1, &mut budget),
            Err(error) => {
                let failure = classify_failure(&error);
                if failure.is_program_rejection() {
                    panic!("applied CLVM program rejected: {error:?}")
                }
                exhausted(NATIVE_MAX_COST)
            }
        }
    }
}
