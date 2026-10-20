# Roadmap history

A dated log of closed transitions and their measurements. Each entry records
what the named commits established and the finite numbers measured at the
time; later entries supersede earlier ones. The short current summary is
[roadmap.md](roadmap.md). Nothing here is a compiler-preservation proof.

## 2026-10-20 — apply bridge, currying, error-study expansion

Commits `67f4d0c`, `4bca10f`, `8d334a4`, `e86564a`, `b92dca0`.

- A reviewed `apply` bridge admits the SDK's reserved export by exact
  signature and per-host typed-MIR fingerprint and lowers to opcode 2 with
  strict bindings; the `delegated-puzzle` fixture witnesses it and every proof
  model rejects applications.
- `chialisp_oxide::curry` ports the pinned `curry-and-treehash.clib` hash
  arithmetic and adds an owned-tree constructor. The `curry-hash` fixture is
  checked against the include itself; the canonical `currying-multiply`
  module is ported with page-argument instances built independently,
  cross-checked against the SDK and executed in five builds; `curry-factory`
  compiles the constructor as a program factory with 210 second-stage
  applications. Optimized Rust for the multiply module is 1,831 bytes against
  7 for Chialisp and Rue: the strict i64 decoder dominates.
- The error study grew from five to eleven studies and names the layer that
  catches each mistake.
- The capture now covers 30 exports, 571 named and 25,222 generated inputs.

## 2026-10-19 — Using Modules ports and the generic append root

Commits `805fe10`, `0da055d`.

- The three Using Modules programs compile unchanged beside signed 64-bit
  typed adapters whose every difference (padding, zero bytes, extra or
  improper arguments, overflow, range) is declared per case and checked
  against an independently specified result; the Rust decoder makes those
  artifacts far larger than the originals.
- The generic corpus append root replaced per-port migration modules. The
  retained 23-export capture was executed twice with exact reports, fresh
  ON/OFF extraction in both repeats, all 23 retained IR programs recompiled in
  both modes, and a separate five-build replay of 1,000 VM evaluations over
  the same 200 inputs.
- Canonical delegated conditions preserves two required raw projections and
  the ignored outer tail. `Cons<First, Rest>` owns its signature digest and
  borrows the conditions tree. Its dedicated suite records 200 native and
  1,000 five-build VM observations plus 1,000 retained-byte replays; the
  ownership harness submits 60 actual SDK transactions (20 accepted, 40 exact
  typed rejections with full simulator rollback); three policy observations and
  12 VM source-control runs detect compiling policy mistakes. The optimized
  module is 629 bytes (Rust off 1,534) against canonical Chialisp's 137 and
  Rue's 293 off / 251 on.
- Guarded projection forwarding and runtime tree optimization: the composed
  guarded/Atom/unary emitter produced the 22-export composition report
  (`227ce911…`), 439 named and 22,983 generated cases, 110 pay-to-singleton
  submissions and fifteen conditions-helper measurements. Pay-to-singleton
  became 1,334 bytes, 57,925 ordinary VM cost and 41,090,749 measured joint
  cost; seven smaller optimized modules had no accepted-case cost regressions
  and all 22 unoptimized and 66 independent-language artifacts stayed exact.
  Compiler verification carried the 578-test workspace and all-target Clippy
  for the same 327 Rust/Cargo/config inputs.

## 2026-10-18 — helper inlining, integer refinement, pay-to-singleton, acyclic proofs

Commits `c87957e`, `bb2c851`, `93e50c8`, `e20f6d1`.

- Bounded helper inlining, entry-wrapper removal and unreachable-function
  pruning, with strict argument bindings, alpha-renamed locals, recursive SCCs
  and the runtime serialization boundary kept explicit. Rue codegen moved to
  worklists; the reproduced cases pass on a 2 MiB native thread stack with an
  upstream regression and fix. Rows live in
  `corpus/helper-inlining-baseline.json`.
- Branch-local intervals remove proven-total repeated checked Add/Sub
  initializers without removing the final checked operation; the integer
  decoder uses a lazy nil case and unsigned byte bounds `[0080, ff80)`.
  Against the signed `c87957e2` milestone, optimized capped-payment went
  1,275 → 539 bytes, split-payment 2,575 → 839 and counter 1,885 → 567; eight
  of 247 accepted named inputs cost less, 239 unchanged, none more. Retained in
  `corpus/runtime-refinement-baseline.json`. A separate Seq8 model proves the
  signed-encoding law with source-hash metadata and mutation controls; it does
  not translate the decoder or prove opcodes, ABI admission or preservation.
- Canonical pay-to-singleton preserves its raw environment and ignored tail.
  Its five-build composition submits 110 actual SDK transactions: 15 accepted,
  95 exact typed consensus rejections with in-memory rollback, through
  independently reproduced launcher, singleton top-layer and StandardLayer
  counterparts (Eve, joint spend, one lineage spend). Source hash sharing
  reduced optimized Rust from 1,972 to 1,685 bytes against canonical
  Chialisp's 403; in the recorded joint bundle total conditions cost fell
  7.06% to 45,309,974 while ordinary VM cost rose by 1,254. The source-only
  report `1a19cca4…` is an unsigned reviewed checkpoint anchored to the
  signed `e20f6d12…` checkpoint. Three actual Rust policy mutations compile
  and have native/VM counterexamples.
- Large acyclic helper graphs fall back to checked version-4 tables after a
  typed legacy expansion limit; an explicit acyclic scalar model admits bounded
  nonrecursive scalar/tuple tables after ABI decoding; default models still
  reject runtime IR.

## 2026-10-17 — trees, bytes, SHA-256, password module, cost iteration

Commits `4f7da20`, `c0085c3`, `3a61698`, `d594842`, `4057921`.

- Primitive immutable byte-slice and fixed-array equality and ordering use
  resolved standard `PartialEq`/`PartialOrd` identities; the published
  inequality and insertion-sort helpers witness them. Z3 4.15.4 pinned for
  verification.
- The SHA-256 bridge admits one reviewed native implementation by signature
  and complete typed-MIR closure fingerprint. The raw-domain `sha256tree`
  adapter and singleton launcher were ported; the launcher retains its
  published 175-byte artifact and identity, reproduced by independent
  Chialisp, while optimized Rust was 478 bytes with a different hash.
  Hash-helper comparisons use a ten-million cost ceiling with five deep inputs
  as separate typed resource observations; the earlier hash gate's 168
  corruption controls and 22,285 retained-artifact VM replays are historical.
- The exact password tutorial module preserves its raw environment and
  returns arbitrary condition trees. Fixed-hash curry instances record 48 case
  groups in five builds, three varying-hash controls and 21 actual ledger
  submissions; unsigned destination retargeting succeeds, which currying does
  not repair. Optimized Rust was 407 bytes against Chialisp's 31 and Rue's
  213; the named valid curried spend cost 7,208 VM units against 1,321 and
  3,937. A later guarded experiment reduced the module to 325 bytes and the
  completed composition to 271.
- The cost iteration added checked constant folding, forward
  execution-success facts, total alias frame elision and exact constructor
  checks; all three closed Intro expressions match the original Chialisp bytes
  and 20-unit VM cost. The signed 21-puzzle baseline lives in
  `corpus/cost-optimization-baseline.json`. A constant-SHA experiment was not
  adopted: folding the tiny `"abc"` hash changed 22 → 47 module bytes while
  cutting primary VM cost 699 → 151, and the isolated `p2_singleton` prototype
  grew 1,972 → 2,158 bytes while each of 136 accepted inputs saved 5,454 VM
  units; complete transaction costs were not measured.

## 2026-10-16 — pipeline, SDK, three languages, trees, recursion

Commits `15be638` through `6f1e197`.

- The checked Rust-to-CLVM pipeline, nominal typed SDK, verified artifact
  diffs, Cargo-native annotated compilation and the real Cargo corpus library.
- Three-language source comparisons (Chialisp, Rust, Rue), the first
  executable error studies and the three canonical Intro expressions with
  explicit environment semantics.
- Borrowed CLVM trees and the canonical `p2_conditions` factory whose
  optimized Rust reproduces the published bytes and hash, with a structural
  model equivalence check for its guard rewrite.
- Typed VM failures and exact consensus rejections; checked Rust recursion
  through closed runtime functions; the published `last_inner` helper through
  checked empty-atom patterns. Frame elision later reduced the optimized
  `last_inner` adaptation from 743 to 369 bytes against the 71-byte Chialisp
  helper adapter and 125-byte Rue; on `(1 2 3)` optimized Rust cost fell from
  25,933 to 11,568 against Chialisp's 2,236 and Rue's 3,814, pinned by
  `scripts/check-runtime-frames.py`.
- The opt-in `OwnedTree` and the published `prefix` adapter with by-value
  recursive outputs; bitwise integer and strict Boolean operations with
  separate scalar SMT checks.
