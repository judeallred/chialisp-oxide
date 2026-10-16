---
title: Compiler architecture and Rue
description: The current direct MIR-to-IR-to-Rue-LIR pipeline and its independent validation boundaries.
---

The working compiler integrates Rue at LIR. It does not need to emit Rue or
Chialisp source as an intermediate representation.

```text
puzzle.rs
  → rustc 1.97.1 analysis + borrow checking
  → typed MIR, supported-feature validation
  → oxide-ir: pure expressions + explicit logical types
  → Rue LIR
  → optional Rue LIR optimizer
  → Rue CLVM code generation
  → serialized CLVM
```

## Keep the boundaries narrow

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
