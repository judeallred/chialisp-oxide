---
title: Compiler architecture and Rue
description: The current direct MIR-to-IR-to-Rue-LIR pipeline and its independent validation boundaries.
---

The working compiler integrates Rue at LIR. It does not need to emit Rue or
Chialisp source as an intermediate representation.

```text
Cargo library + oxide-sdk + #[oxide_sdk::puzzle]
  → Cargo dependencies, features, build scripts, generated code
  → rustc nightly-2026-07-14 analysis + borrow checking
  → typed MIR, supported-feature validation
  → oxide-ir: pure expressions + explicit logical types
  → Rue LIR
  → optional Rue LIR optimizer
  → Rue CLVM code generation
  → serialized CLVM
```

## Keep the boundaries narrow

`cargo-oxide` selects one Cargo library package and orchestrates extraction.
`oxide-sdk` supplies real dependency types/helpers and re-exports the
metadata-only annotation from `oxide-macros`. The attribute preserves the
original function tokens; rustc remains responsible for their semantics.
`oxide-driver` owns compiler-private rustc APIs and source diagnostics.
`oxide-ir` owns the serializable typed expression contract, whole-program
validation, ABI checks, and Rue lowering. `oxide-chialisp` owns the independent
Chialisp reference compiler. `oxide-simulator` bridges serialized outputs into
actual ledger validation. Corpus tooling compares these paths.

The IR represents arguments, tuples, fixed bytes, fixed-width integers,
branches, operations, explicit rejection, and sequencing. Its validator infers
expression types and rejects mismatches or unsupported operations before code
generation. Serialization is a boundary contract, not a reason to trust a
handwritten IR document.

The driver checks nominal transparent wrappers as Rust types, then erases them
to the supported inner type before emitting IR. The IR and CLVM wire ABI do not
carry `CoinId` or `PuzzleHash` runtime tags. Concrete vetted SDK calls can cross
the crate boundary; arbitrary dependency bodies cannot. Constructor validation
is not executed during solution decoding.

The corpus is the real `oxide-puzzles` Cargo library, with annotated modules
registered in `puzzles/lib.rs` and a dependency on `oxide-sdk`. Validation
extracts all declared exports in one Cargo build; its native oracle links that
same library. A separate consumer fixture additionally exercises feature flags,
generated source, and native-only allocation code. `sdk/oxide.rs` remains only
for standalone compiler regressions, outside the corpus authoring path.

## Cargo extraction is a compiler integration, not a target name

The launcher drives Cargo through its normal CLI and metadata rather than
reimplementing package resolution. A rustc wrapper forwards host build scripts,
procedural macros, and compiler probes. The selected library's actual edition,
features, `--extern` arguments, generated source, and build-script environment
reach the pinned compiler. An explicit host analysis target separates those
units; it is not a CLVM target specification.

`cargo oxide build` publishes an immutable bundle only after the entire Cargo
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
recorded local changes to the pinned revision, not unmodified upstream.

Entering at LIR also bypasses Rue frontend and earlier lowering work. Oxide is
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
It has its own allocator and internal VM version; serialized programs cross
into the same `clvmr 0.21.0` executor used for Rust outputs. Execution reports
record `MEMPOOL_MODE` and explicit cost limits.

Ledger simulation uses a separately pinned stack. Its constants and VM version
are shown in [verification](/chialisp-oxide/verification/method/) and the report.
Do not merge these two version labels into a single misleading runtime claim.
