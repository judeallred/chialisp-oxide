---
title: Compiler architecture and Rue
description: The current direct MIR-to-IR-to-Rue-LIR pipeline and its independent validation boundaries.
---

The working compiler integrates Rue at LIR. It does not need to emit Rue or
Chialisp source as an intermediate representation.

```text
Cargo library + chialisp-oxide + #[chialisp_oxide::puzzle]
  → Cargo dependencies, features, build scripts, generated code
  → rustc nightly-2026-07-14 analysis + borrow checking
  → typed MIR, supported-feature validation
  → chialisp-oxide-ir: typed expressions + optional closed function table
  → optional typed IR tree-guard pass
  → Rue LIR
  → optional Rue LIR optimizer
  → Rue CLVM code generation
  → serialized CLVM
```

## Keep the boundaries narrow

`cargo-chialisp-oxide` selects one Cargo library package and orchestrates extraction.
`chialisp-oxide` supplies real dependency types/helpers and re-exports the
metadata-only annotation from `chialisp-oxide-macros`. The attribute preserves the
original function tokens; rustc remains responsible for their semantics.
`chialisp-oxide-driver` owns compiler-private rustc APIs and source diagnostics.
`chialisp-oxide-ir` owns the serializable typed expression contract, whole-program
validation, ABI checks, and Rue lowering. `chialisp-oxide-reference` owns the independent
Chialisp reference compiler. `chialisp-oxide-simulator` bridges serialized outputs into
actual ledger validation. Corpus tooling compares these paths.

## One package family

Puzzle authors add the `chialisp-oxide` package and import `chialisp_oxide`.
Its re-exported `#[chialisp_oxide::puzzle]` attribute comes from the separate
`chialisp-oxide-macros` crate; a second consumer dependency is unnecessary.
Cargo discovers the `cargo-chialisp-oxide` executable as `cargo chialisp-oxide`.
The standalone research/validation binary is `chialisp-oxide`, and internal
packages use the same `chialisp-oxide-*` prefix. The existing upstream
`chialisp` crate remains the independent reference compiler dependency.
See the [researched naming rationale](https://github.com/judeallred/chialisp-oxide/blob/main/docs/rust-authoring-research.md#package-import-and-command-names).
These crates remain unpublished; use checkout path dependencies or a reviewed
pinned Git revision.

The IR represents arguments, tuples, fixed bytes, fixed-width integers,
branches, raw atom/pair operations, explicit rejection, sequencing, and typed
function calls/lexical bindings. Its validator infers
expression types and rejects mismatches or unsupported operations before code
generation. Serialization is a boundary contract, not a reason to trust a
handwritten IR document.

The driver checks nominal transparent wrappers as Rust types, then erases them
to the supported inner type before emitting IR. The IR and CLVM wire ABI do not
carry `CoinId` or `PuzzleHash` runtime tags. Concrete vetted SDK calls can cross
the crate boundary; arbitrary dependency bodies cannot. Constructor validation
is not executed during solution decoding.

Entry metadata can explicitly mark a parameterless export's environment as
ignored. The IR/backend then omit argument decoding for that export, preserving
the behavior of canonical closed Chialisp expressions. The raw ABI passes the whole
environment to one structurally checked borrowed-tree parameter. Runtime-free
ignored/raw entries use version-2/version-3 wire envelopes; runtime constructs
use version 4 with the same input policy. Default exports retain the exact typed
argument-list ABI.

The corpus is the real `chialisp-oxide-puzzles` Cargo library, with annotated modules
registered in `puzzles/lib.rs` and a dependency on `chialisp-oxide`. Validation
extracts all declared exports in one Cargo build; its native oracle links that
same library. A separate consumer fixture additionally exercises feature flags,
generated source, and native-only allocation code. `sdk/chialisp_oxide.rs` remains only
for standalone compiler regressions, outside the corpus authoring path.

## Cargo extraction is a compiler integration, not a target name

The launcher drives Cargo through its normal CLI and metadata rather than
reimplementing package resolution. A rustc wrapper forwards host build scripts,
procedural macros, and compiler probes. The selected library's actual edition,
features, `--extern` arguments, generated source, and build-script environment
reach the pinned compiler. An explicit host analysis target separates those
units; it is not a CLVM target specification.

`cargo chialisp-oxide build` publishes an immutable bundle only after the entire Cargo
invocation and CLVM lowering succeed. `latest.json` advances to that successful
manifest. Extraction uses a separate target directory, preserving ordinary
native Cargo artifacts; selected-package extraction is repeated even when
Cargo could otherwise call the package fresh.

The pin is `nightly-2026-07-14` (`rustc 1.99.0-nightly`, commit `daf2e5e18`),
with `rustc-dev` and LLVM compiler components. No bootstrap environment override
is needed. The driver checks compiler identity before reading compiler-private
metadata.

Rust's [custom target specification](https://doc.rust-lang.org/rustc/targets/custom.html)
configures a target; a real CLVM backend would additionally need code generation,
runtime layouts, linking, and an appropriate core/SDK strategy. The current
implementation is a Cargo-driven MIR adapter into Rue, with no `--target clvm`
workflow. The repository's authoring research documents the wider backend and
proof-tool comparisons.

`puzzles/authoring.json` explicitly declares the Cargo package, library,
export/source mapping, and local input files. Both validation and proof reports
record their hashes; the proof also identifies its selected counter export.
The site checks both sets against current files and requires their agreement.
Per-fixture dependency records additionally retain complete source text and
hashes. These are declared local inputs, not an automatically discovered
complete transitive dependency graph. CI also
recompiles the corpus and compares the committed report before Pages deploys
the successful commit.

## Closed runtime functions

The driver discovers supported calls by actual Rust function identities. Acyclic
source graphs retain expression inlining. If a reachable graph contains a cycle,
each function is emitted once into a dense table and the exported body calls
entry zero. Version-4 IR adds `Function`, `Call`, typed `Let`, and `Local` while
retaining strict, ignored-nullary, or raw-tree input validation. There is no new
recursion annotation or public environment mode.

A runtime frame contains a compiler-built table and value slots. Calls select
quoted code only from that table and start fresh argument slots; arguments and
locals cannot supply a callee. Quoted continuations implement strict once-only,
left-to-right operand evaluation and rebase lexical paths when binding values.
This avoids cyclic compiler-arena references, compile-time recursive unrolling,
and repeated execution of substituted call results. Self, mutual, and non-tail
calls are exercised by actual Rust extraction and direct IR regressions.

All signatures, bodies, lexical scopes and global budgets are checked, including
unused functions and dead branches. Runtime lowering bounds new frame bindings
and copied types before scope growth; codegen additionally checks serialized
expansion. These checks bound compilation, not input-dependent execution.
Projected writes in runtime functions, CFG loops, allocation, closures, and
solution-selected application remain unsupported. Constants/promoted MIR stays
on its bounded acyclic route. Current scalar/tree SMT APIs reject runtime IR;
finite VM replay does not prove termination or compiler preservation. See
[the ABI version boundary](/chialisp-oxide/reference/abi/#ir-version-4-and-export-markers)
and [runtime design and tests](https://github.com/judeallred/chialisp-oxide/blob/main/docs/runtime-calls.md).

## Conservative tree-guard optimization

The owned typed IR pass can remove a pair-shape guard with a rejecting alternative
when every successful evaluation of the retained branch requires a checked
first/rest projection of that same tree argument. The projection remains in
the program, so atom inputs still reject. The analysis respects strict tuple
fields and sequencing, lazy branch selection, and distinct argument identities.
It does not remove environment ABI validation.

For the canonical `p2_conditions` port, this exposes the simple quote construction
to Rue LIR and produces the exact published nine-byte module. Unoptimized Rust
still has a different artifact. The [factory walkthrough](/chialisp-oxide/tutorials/p2-conditions/)
separates this measured module identity from broader compiler preservation.

The owned guard pass skips every runtime program: successful-output facts
cannot establish that a checked projection happens before a preceding call
diverges. Runtime compilation still runs Rue LIR optimization when enabled.
For runtime-free Rust, optimization on runs both passes. Off skips
both; `--no-optimize` selects that baseline. Rue source comparisons use only
their frontend's LIR optimization switch. The report records both pipelines,
including the legacy `optimize_lir` field name. Execution costs and rejection
diagnostics need not be identical across modes.

Independent bounded interpreter/VM tests exercise original, rewritten, and
repeatedly rewritten IR. The structural proof experiment additionally compares
the source-derived and rewritten factory models. These checks do not establish
compiler preservation or a universal proof of the pass for every program.

## Why the Rue optimizer

The project pins Rue revision
[`245c107a8e795229ea08097142f005a4c0d91075`](https://github.com/xch-dev/rue/tree/245c107a8e795229ea08097142f005a4c0d91075).
Its public LIR optimizer and code generator are reusable Rust components.
Direct emission avoids generating and reparsing Rue text, while retaining a
well-established CLVM-oriented backend.

The focused `vendor/rue-lir` source import includes its Apache-2.0 license and
provenance. A local identity-product fix prevents an optimizer panic for
`1 * 1`; the regression and upstream report are tracked in
[Rue issue 108](https://github.com/xch-dev/rue/issues/108). Cargo's source patch
applies the fix to both direct LIR emission and the Rue-source comparison path.
The import also fixes a strict-evaluation regression in truthiness folding,
tracked in [Rue issue 109](https://github.com/xch-dev/rue/issues/109). These are
recorded local changes to the pinned revision. Further checked-in fixes retain
required pair-projection failures during reconstruction
([issue 110](https://github.com/xch-dev/rue/issues/110)) and the explicit nil
environment of quoted execution ([issue 111](https://github.com/xch-dev/rue/issues/111)).
Each has an execution-based regression and a standalone upstream patch.

Entering at LIR also bypasses Rue frontend and earlier lowering work. chialisp-oxide is
responsible for correct environments, paths, representations, evaluation
strictness, and input validation. Reusing an optimizer does not prove those
decisions correct. Both optimized and unoptimized output must satisfy the same
semantic corpus.

Rue source emission remains useful as a debugging or comparison experiment.
Importing or adapting upstream compiler passes remains acceptable when it
improves semantics or maintainability; preserve upstream revision, licenses,
attribution, and relevant tests.

## Independent comparisons

The Chialisp reference compiler is Chia Network's pinned `chialisp 0.4.6`.
Its module API handles `mod` sources. Raw canonical expressions use a separate
adapter for the `(opt (com 2))` compilation stage of upstream `run`; it stops
before execution and adds no module wrapper. Both routes serialize actual
upstream-generated program code, which then runs in the project's executor.
It has its own allocator and internal VM version; serialized programs cross
into the same `clvmr 0.21.0` executor used for Rust outputs. Execution reports
record `MEMPOOL_MODE` and explicit cost limits.

Ledger simulation uses a separately pinned stack. Its constants and VM version
are shown in [verification](/chialisp-oxide/verification/method/) and the report.
Do not merge these two version labels into a single misleading runtime claim.
