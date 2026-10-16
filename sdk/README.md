# Source-only puzzle SDK

`oxide.rs` defines nominal types and named condition constructors for the compiler's
supported subset. Include it with a relative module path in a standalone Rust
puzzle. It is not yet a published Cargo crate or a promise of general dependency
support.

The `#[repr(transparent)]` wrappers have exactly one field and no custom drop.
The compiler erases validated wrappers to the inner wire type. Rust distinguishes
`PuzzleHash` from `CoinId`; both are 32-byte CLVM atoms. That distinction protects
source-level API calls, not raw solution bytes after compilation. A public-key
wrapper does not validate curve membership or establish signature authorization.
Named constructors bind condition opcodes and argument order; using the raw
underlying tuples remains possible and is not certified by these helpers.
