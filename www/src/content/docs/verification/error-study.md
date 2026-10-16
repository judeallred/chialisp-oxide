---
title: Errors and the guarantees we actually have
description: Compare Rust, Chialisp, and Rue protections without attributing Rue's existing type checks uniquely to Rust.
---

The useful question is which check rules out a concrete failure, and under
which assumptions. “Written in Rust” is not a security property by itself.
Rue already has types and diagnostics, so benefits shared with Rue should not
be described as unique Rust advantages.

The primitive regression `type_error.rs` evaluates `amount + true` for a
`u64` amount. Rust rejects it with `E0277`, “cannot add `bool` to `u64`,” before
the driver can emit IR. This is distinct from a malformed solution, which is
constructed after compilation and must reject at runtime.

## Checked before CLVM generation

Rust's own analysis rejects incompatible source types and invalid borrows. The
frontend's regression fixtures include those errors. Unsupported MIR is
rejected with a source location; unsafe code is forbidden. These checks help
prevent invalid programs from reaching the backend within the documented
subset.

The current compiler does not yet support references in accepted puzzle code.
A borrow-check failure demonstrates that rustc analysis ran; it is not a claim
of a broad ownership-based on-chain API.

Rue also rejects many ordinary type mistakes. Chialisp's untyped atoms and
pairs make more representation errors possible unless explicit checks are
written. Side-by-side references show the runtime checks needed for an
equivalent ABI, rather than presenting a shorter unsafe reference as equivalent.

## Domain swaps rejected by the typed SDK

The actual `chialisp-oxide` crate distinguishes `CoinId` from `PuzzleHash` and
`Amount` from `Timestamp` with single-field transparent nominal wrappers.
The Cargo consumer `examples/cargo-invalid`, enabled with `type-error`, fails
normal `cargo check` because `create_coin` receives a `CoinId` where it requires
`PuzzleHash`. Its unconfigured division example is valid native Rust but fails
the additional CLVM subset check. The two boundaries are tested separately.

The standalone compatibility regression `newtype_swap.rs` fails with `E0308`, expected `PuzzleHash`,
found `CoinId`; `newtype_time_swap.rs` fails with expected `Timestamp`, found
`Amount`. The driver test requires both names in the Rust diagnostic and no
emitted IR. The corpus itself now imports the real SDK crate from the
`chialisp-oxide-puzzles` Cargo library; standalone compatibility regressions remain
separate. The Cargo integration also compares actual generated outputs
with and without optimization and under the changed feature cap.

These direct API mixups are compile-time errors when those typed helpers are
used. Public fields still allow deliberate unwrap/rewrap, and raw tuple
construction remains available. Rue's nominal struct types provide analogous
domain distinctions; this is not a uniquely Rust protection. See the
[SDK tutorial](/chialisp-oxide/tutorials/typed-sdk/) for the actual source and
reproduction commands.

## Rejected at the puzzle boundary

The generated ABI checks list arity, proper tails, scalar-versus-pair shape,
canonical booleans, integer bounds, numeric canonicalization, and fixed byte
lengths. Malformed solutions are runtime rejections, not compile-time
impossibilities. The caller constructs the raw tree after compilation.

The cap and split manifests contain negative, padded, oversized, missing,
extra, and nested inputs. The lab displays their exact outcomes in all
compared execution paths.

## Preserved during lowering

Checked Rust arithmetic must not silently become arbitrary-precision CLVM
arithmetic. Overflow rejects. Explicit sequencing preserves an unused failing
operation. Lazy branches must not evaluate an unchosen rejection. Semantic
regressions test these obligations in both optimizer modes.

The corpus also exposed an upstream Rue LIR identity-product optimizer panic.
The focused imported-source fix and regression are tracked in
[Rue issue 108](https://github.com/xch-dev/rue/issues/108). A compiler dependency
can contain defects; the test boundary applies to upstream optimizations too.

A second reproduced regression shows why strictness matters. A typed Rue list
containing `1 / 0` correctly rejects without LIR optimization. With automatic
inlining and optimization, folding the constructed list's truthiness could
erase evaluation of the failing field and return one instead. The focused fix
preserves rejection when folding would discard a potentially failing operand.
The upstream report is [Rue issue 109](https://github.com/xch-dev/rue/issues/109).
This is a measured compiler-behavior defect with regression cases; no deployed
puzzle exploit was assessed.

## Still possible to get wrong

The named SDK helpers prevent direct source-level domain swaps, but their
condition aliases still expose tuple representations. A wrong manually written
condition number, deliberate byte rewrap, wrong key, destination, fee,
timelock, or signature commitment can compile. The teaching fixtures now name
their conditions and policy constants, but aliases still expose tuple layouts.

Transparent wrappers add no runtime domain tags. ABI decoding checks the inner
encoding and does not invoke constructors or validate their custom invariants.
An attacker can supply any correctly sized bytes at a `PuzzleHash` boundary;
Rust's source type does not authenticate their meaning.

A caller-controlled boolean can gate rejection but cannot authenticate a
caller. A fixed recipient can constrain payment but cannot require ownership.
A time condition can mature while the intended authorization is absent.
The tutorials call out these differences and ledger scenarios exercise the
specific policies they claim.

Replay resistance, conservation, atomic compositions, and economic safety need
explicit specifications and transaction-level tests. The current counter
model establishes a small typed-scalar invariant; it does not certify an
arbitrary payment or protocol. Read [the method](/chialisp-oxide/verification/method/)
and [formal scope](/chialisp-oxide/verification/formal/) with any result.
