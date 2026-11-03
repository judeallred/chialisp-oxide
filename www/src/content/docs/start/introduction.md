---
title: Learn to build Chia puzzles in Rust
description: Start with the Chia coin model, build and test a Cargo puzzle, then understand funding, signing and spending.
---

**WIP prototype. Not ready for use. Do not use for real funds or production puzzles.**

Chialisp Oxide lets you write experimental Chia puzzle policies in a restricted
subset of real Rust. This guide starts with the coin being protected, then
builds a Cargo library, tests its behavior, and explains the separate funding
and spending workflow. You do not need a wallet for the first lessons.

## Choose your starting point

If Chia is new to you, follow this order:

1. [Understand a Chia coin](/chialisp-oxide/start/coin-model/): puzzle, solution,
   conditions, address and coin ID; what changes when a coin is spent.
2. [Try the Cargo prototype](/chialisp-oxide/start/quickstart/): install the
   pinned tools, run native tests and compile annotated exports.
3. [Read your first fixed payment](/chialisp-oxide/tutorials/first-payment/):
   identify its committed recipient, exact amount and permissionless policy.
4. [Use typed helpers](/chialisp-oxide/tutorials/typed-sdk/), then
   [validate inputs](/chialisp-oxide/tutorials/input-validation/) and
   [require signatures](/chialisp-oxide/tutorials/signatures/). Encoding,
   application rules and authorization are separate responsibilities.
5. [Follow build to coin spend](/chialisp-oxide/start/build-to-spend/): complete a
   wallet-free simulator lifecycle, then locate the supported Testnet11 workflow.

If you already know Chialisp, start with the [Cargo quickstart](/chialisp-oxide/start/quickstart/)
and [ABI](/chialisp-oxide/reference/abi/), then compare the
[canonical Intro](/chialisp-oxide/tutorials/canonical-intro/) and
[Using Modules](/chialisp-oxide/tutorials/canonical-modules/) ports. Pure language
examples can return numbers or trees; a coin-spending policy must return valid
conditions. These are different kinds of example.

## What the compiler contributes

Write an ordinary Cargo library, depend on the unpublished `chialisp-oxide`,
and mark entry functions with `#[chialisp_oxide::puzzle]`. Native Cargo checking
and tests remain normal Rust. `cargo chialisp-oxide build` additionally validates
the reachable subset and lowers checked MIR through typed IR and Rue's CLVM
backend. It produces artifacts, not a deployed coin.

The compiler checks supported Rust semantics and the typed input encoding.
It cannot choose the right recipient, supply authorization that your policy
omits, or prove a successful VM result will be a valid transaction. Consensus
checks the returned conditions in the context of the complete spend.

CLVM evaluates immutable trees. Ordinary Rust can also depend on memory, I/O,
allocation, target configuration and unwinding; those features need explicit
encodings and semantics before they belong in a puzzle compiler. See the
[current functionality map](/chialisp-oxide/reference/chialisp-functionality/)
for supported operations and remaining gaps.

## What the current examples represent

The consumer example and paired laboratory fixtures are ordinary Cargo packages
with real SDK dependencies and annotated exports. The corpus lives in the
`chialisp-oxide-puzzles` workspace library; both CLVM extraction and the native oracle
use that same library. Cargo support does not mean arbitrary cross-crate Rust libraries are
accepted for CLVM, or that a native Cargo test validates a coin spend.

The laboratory separates original teaching implementations with authored
Chialisp references from [canonical Intro expression ports](/chialisp-oxide/tutorials/canonical-intro/)
that preserve the pinned originals. Neither group is an audited production
module or a completed replacement for a canonical Chia protocol. The
[published list helper](/chialisp-oxide/tutorials/last-list-element/) separately
retains an exact helper span through an explicit module adapter; that adapter
does not complete its source library or claim a published module hash. The
[source inventory](/chialisp-oxide/coverage/) tracks example-level evidence and
the much larger tutorial and CHIP migration goal separately.

Chialisp's [primer](https://chialisp.com/chialisp-primer/),
[conditions](https://chialisp.com/conditions/), and
[CLVM reference](https://chialisp.com/clvm/) are the underlying language and coin
model references. Rue supplies compiler infrastructure; its type system already
prevents many mistakes that would be possible in untyped Chialisp.
