---
title: Real Rust, deliberate semantics
description: What Chialisp Oxide compiles, why Rue is the backend, and how to read the evidence.
---

**WIP prototype. Not ready for use. Do not use for real funds or production puzzles.**

Chialisp Oxide experiments with compiling a restricted subset of real Rust into
CLVM for Chia puzzles. Write an ordinary Cargo library, depend on the unpublished
`chialisp-oxide`, and mark entry functions with `#[chialisp_oxide::puzzle]`. Native Cargo
checking and tests remain normal Rust; `cargo chialisp-oxide build` additionally validates
the reachable subset and lowers checked MIR through a pure IR into Rue's
optimizer and CLVM code generator.

The restriction is intentional. CLVM evaluates immutable trees; ordinary Rust
programs can depend on memory, I/O, allocation, target configuration, and
unwinding. A supported language feature needs an explicit encoding and tested
semantics before it belongs in a puzzle compiler.

## Follow one example all the way through

1. [Try the Cargo prototype](/chialisp-oxide/start/quickstart/) with two annotated functions, features, and generated code.
2. Read [the ABI](/chialisp-oxide/reference/abi/) before constructing a solution.
3. Open [the puzzle laboratory](/chialisp-oxide/lab/) to compare native Rust,
   independently compiled Chialisp, and optimized/unoptimized CLVM.
4. Inspect ledger checks for signatures, conservation, locks, and rollback.
5. Read [the security scope](/chialisp-oxide/verification/method/) before extending a policy.

## A function result is not a transaction

A puzzle returns a list of conditions. The consensus engine interprets those
conditions in the context of a spend bundle and the coin set. A CLVM evaluation
can succeed while the transaction fails because a signature is absent, a lock
has not matured, or more value is created than consumed.

This repository exercises both levels. The pure `bounded-counter` example
and canonical Intro expressions return language values and are explicitly not
coin-spending policies. A caller-controlled
boolean gate demonstrates rejection but supplies no authentication. Each lab
entry states its policy and the obligations outside it.

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
[source inventory](/chialisp-oxide/coverage/) tracks example-level evidence and
the much larger tutorial and CHIP migration goal separately.

Chialisp's [primer](https://chialisp.com/chialisp-primer/),
[conditions](https://chialisp.com/conditions/), and
[CLVM reference](https://chialisp.com/clvm/) are the underlying language and coin
model references. Rue supplies compiler infrastructure; its type system already
prevents many mistakes that would be possible in untyped Chialisp.
