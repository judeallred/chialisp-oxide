# Standalone-harness SDK compatibility module

**WIP prototype. Not ready for use. Do not use for real funds or production puzzles.**

New Cargo consumer experiments use the actual, unpublished
[`oxide-sdk` crate](../crates/oxide-sdk) and `#[oxide_sdk::puzzle]`, as shown in
[`examples/cargo-puzzles`](../examples/cargo-puzzles). See the
[Cargo launcher](../crates/cargo-oxide/README.md) for the current prototype flow.

`oxide.rs` remains a source-only compatibility module for the existing
standalone-file corpus and its historical tests. It defines nominal types and
named condition constructors with the same inner wire representations as the
crate API. Its presence does not mean the paired corpus has migrated to Cargo
dependencies. The two modules' wrappers are distinct Rust types; use the
appropriate interface for the experiment rather than mixing them implicitly.

The `#[repr(transparent)]` wrappers have exactly one field and no custom drop.
The compiler erases validated wrappers to the inner wire type. Rust distinguishes
`PuzzleHash` from `CoinId`; both are 32-byte CLVM atoms. That distinction protects
source-level API calls, not raw solution bytes after compilation. A public-key
wrapper does not validate curve membership or establish signature authorization.
Named constructors bind condition opcodes and argument order; using the raw
underlying tuples remains possible and is not certified by these helpers.
