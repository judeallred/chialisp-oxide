# Rust frontend

`oxide-driver` uses the compiler-private API of **rustc 1.97.1**, with its
`rustc-dev` and `llvm-tools-preview` components. The workspace scopes
`RUSTC_BOOTSTRAP=oxide_driver` to building this driver; this is a pinned stable
compiler integration, not a stable public rustc API. Updating rustc requires
reviewing its API and rerunning the semantic and compile-fail corpus.

```sh
cargo build -p oxide-driver
target/debug/oxide-driver crates/oxide-driver/tests/fixtures/guarded_payment.rs --output program.json
```

The driver compiles an edition-2024 library and selects its unique free function
named `puzzle` (`--entry NAME` changes that name). It runs rustc's complete
analysis, explicitly forces every local body's borrow checker, checks for
diagnostics, then lowers typed MIR. It does not parse printed MIR or translate
Rust syntax with `syn`. No JSON is emitted when Rust checking or lowering fails.

The initial subset supports:

- `bool`; `u8` through `u128`; `i8` through `i128`; unit; tuples and fixed arrays
  of at most 256 items. Byte arrays have a distinct byte-string ABI.
- Local, non-generic `#[repr(transparent)]` structs with exactly one field,
  no custom `Drop`, a nonzero layout, and a recursively supported inner type.
  Rustc checks nominal distinctions such as `PuzzleHash` versus `CoinId` before
  lowering; wrapper construction and field projection erase to the inner ABI.
- Local values, tuple fields, comparisons, `!bool`, `if`, integer/boolean
  `match`, and acyclic control flow.
- Checked `+`, `-`, `*`, with overflow becoming puzzle rejection. Overflow
  checking and debug assertions are always enabled, independently of the host
  Cargo profile. Explicit wrapping/overflowing methods are rejected.
- Plain `assert!` as puzzle rejection, and local non-generic helper functions
  inlined into expressions. Unused failing operations and helper calls are
  preserved through explicit sequence expressions.
- Byte arrays passed through as parameters and constant byte-array construction.
  Tuple fields and MIR constant array projections are supported; ordinary Rust
  indexing is rejected when it requires an index local or reference.

References, generic functions, external calls, recursion, loops, allocation,
ordinary structs/enums, casts, floats, shifts, bitwise arithmetic, and division/remainder
are rejected. Panic messages and formatted assertions currently require
unsupported Rust formatting calls and are rejected. Unsafe Rust is forbidden by rustc's `-Funsafe-code`, including
code that MIR might otherwise eliminate. Each unsupported MIR diagnostic carries
its Rust source location. Expansion is limited to 32 nested helpers, 64 active
control-flow blocks, 10,000 total visited blocks, 8,192 nodes per expression,
expression depth 64, and 500,000 cumulative expression nodes of lowering work.
Argument/environment cloning and aggregate construction have separate size
checks. Type expansion is limited to 4,096 nodes and depth 32 before replicating
nested arrays. Functions are limited to 64 parameters and 2,048 MIR
locals. Accepting a function does not establish an affordable CLVM cost.

This is **host-checked Rust**, not a rustc CLVM target. Source expansion currently
uses the pinned compiler's host configuration, including `cfg(target_*)`,
`cfg(debug_assertions)`, pointer size in compile-time expressions, and standard
library availability. Avoid target-dependent source in reproducible puzzles;
`usize`/`isize` are rejected in the accepted ABI. The supported on-chain subset
does not imply that ordinary Rust libraries can run on CLVM.

The output is the typed `oxide-ir` JSON contract. The CLVM backend validates
the whole contract again and validates untrusted solution encodings before
evaluating the puzzle body.

Transparent wrappers carry **source-level nominal identity**, not wire tags.
`PuzzleHash([u8; 32])` and `CoinId([u8; 32])` both decode as exactly 32 bytes.
Decoding does not run a wrapper constructor or establish user-defined constructor
invariants. Explicit puzzle checks must enforce those invariants at runtime.
Zero-sized wrappers, multiple-field transparent structs (including extra
zero-sized fields), generic wrappers, external wrappers such as `NonZeroU64`,
custom destructors, and macro-generated wrapper declarations are rejected.
Named aggregate constants and borrowed wrapper methods remain unsupported.
