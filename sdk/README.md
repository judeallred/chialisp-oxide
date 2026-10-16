# Standalone-harness SDK compatibility module

**WIP prototype. Not ready for use. Do not use for real funds or production puzzles.**

New Cargo consumer experiments use the actual, unpublished
[`chialisp-oxide` crate](../crates/chialisp-oxide) and `#[chialisp_oxide::puzzle]`, as shown in
[`examples/cargo-puzzles`](../examples/cargo-puzzles). See the
[Cargo launcher](../crates/cargo-chialisp-oxide/README.md) for the current prototype flow.

`chialisp_oxide.rs` remains a source-only compatibility module for standalone compiler
regressions. The paired corpus is now the real
[`chialisp-oxide-puzzles` Cargo library](../puzzles), with annotated exports and a direct
dependency on `chialisp-oxide`. Native tests, differential validation, and proof
extraction use that library. The compatibility module imports the same domain
implementation, but defines distinct Rust types in its local module; it is not
the corpus authoring interface.

The `#[repr(transparent)]` wrappers have exactly one field and no custom drop.
The compiler erases validated wrappers to the inner wire type. Rust distinguishes
`PuzzleHash` from `CoinId`; both are 32-byte CLVM atoms. That distinction protects
source-level API calls, not raw solution bytes after compilation. A public-key
wrapper does not validate curve membership or establish signature authorization.
Named constructors bind condition opcodes and argument order; using the raw
underlying tuples remains possible and is not certified by these helpers.
