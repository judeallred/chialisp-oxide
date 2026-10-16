---
title: Supported Rust
description: The implemented subset, rejected features, arithmetic behavior, and host-configuration limits.
---

The source language is edition-2024 Rust, checked by rustc 1.97.1. The frontend
selects a unique free function named `puzzle`; `--entry NAME` selects another
name. It performs complete analysis, explicitly forces local borrow checks,
checks diagnostics, and then lowers typed MIR. Printed MIR is not parsed.

## Implemented values and control flow

- `bool`, `u8`–`u128`, `i8`–`i128`, unit, tuples, and fixed arrays of up to 256 items.
- Local values, tuple fields, comparisons, boolean negation, `if`, integer and
  boolean `match`, and acyclic control flow.
- Checked `+`, `-`, and `*`; overflow rejects the puzzle.
- Plain `assert!` as puzzle rejection. Formatted assertions and panic messages
  are rejected because their formatting calls are outside the current subset.
- Local, non-generic helper functions, inlined into pure expressions.
- Byte-array parameters and constant byte arrays, with a byte-string ABI.
- Local, nongeneric, single-field `#[repr(transparent)]` nominal wrappers over
  supported nonzero-size values. Rust checks their identity; lowering erases
  them to the inner ABI type. See [typed puzzle helpers](/chialisp-oxide/tutorials/typed-sdk/).
- Supported MIR constant array projections. Ordinary indexing that needs an
  index local or reference is rejected.

Byte arrays and tuple-like products use different encodings. The compiler does
not use Rust's native in-memory layout as an on-chain ABI.

## Rejected features

References, generic functions, external function calls, recursion, loops,
allocation, general structs/enums, casts, floating point, shifts, bitwise arithmetic,
division, and remainder are not implemented in the accepted subset. `usize`
and `isize` are rejected as ABI types. Unsafe code is forbidden by rustc's
`-Funsafe-code`, including unsafe code that MIR might otherwise eliminate.

The transparent-wrapper exception requires a local, nongeneric declaration
with exactly one supported nonzero-size field, no custom `Drop`, and no
macro-generated declaration. Extra fields, even zero-size ones, are rejected.
This support does not make ordinary structs, enums, or external wrapper types
available on-chain.

The internal IR enumerates some future operations. An enum variant is not a
supported language feature: validation rejects operations without implemented,
tested lowering.

## Arithmetic and evaluation

Overflow checks and debug assertions are enabled independently of the host
Cargo profile. `u64::MAX + 1` must not become an unbounded CLVM integer.
Explicit wrapping and overflowing arithmetic methods are rejected.

Unused failing computations are still significant. If a helper rejects and its
returned value is unused, strict Rust evaluation requires rejection. The IR's
sequence nodes preserve this obligation through optimization. A failure in an
unchosen branch remains lazy.

## Bounded compilation

The frontend limits helper nesting to 32, active control-flow blocks across
calls to 64, and total visited blocks to 10,000. Expressions are limited to
8,192 nodes and depth 64, with 500,000 cumulative expression nodes of lowering
work. Types are limited to 4,096 nodes and depth 32; functions have at most
64 parameters and 2,048 MIR locals. Aggregate construction and cloning are
bounded before replication.

The IR independently validates types, expression depths, parameter counts, and
tuple shapes. These bounds constrain compiler work; they do not establish that
the emitted CLVM is affordable for every input.

## Host-checked source, not a CLVM rustc target

Source expansion currently uses the pinned host compiler configuration. That
includes `cfg(target_*)`, `cfg(debug_assertions)`, pointer-size expressions, and
host standard-library availability. Avoid target-dependent source in
reproducible puzzles. Ordinary `std` or `no_std` crates are not automatically
compatible with the on-chain subset.

Compile-fail fixtures exercise unsupported features, Rust type/borrow failures,
and unsafe code. They are useful regression evidence; they are not an argument
that a general Rust-to-CLVM translation has been proved correct.
