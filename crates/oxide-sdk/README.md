# oxide-sdk

Work-in-progress Rust authoring crate for `cargo oxide`. It provides the
`#[oxide_sdk::puzzle]` export annotation, transparent domain types, and pure
condition constructors that remain ordinary Rust functions in native tests.
It has not been published to crates.io; use a local path dependency from this
repository. See `examples/cargo-puzzles` for a complete Cargo consumer.

```rust
use oxide_sdk::{Amount, CreateCoin, PuzzleHash, create_coin};

#[oxide_sdk::puzzle]
pub fn pay(destination: PuzzleHash, amount: Amount) -> CreateCoin {
    assert!(amount.0 <= 1000);
    create_coin(destination, amount)
}
```

`PuzzleHash`, `CoinId`, `PublicKey`, `Message32`, `Amount`, and `Timestamp` are
distinct Rust types. Passing a `CoinId` where a `PuzzleHash` is required fails
normal Rust type checking. The CLVM ABI erases their wrappers to fixed bytes or
a bounded integer; these are source-level domains, not runtime nominal tags.
Their public constructors establish no cryptographic validity or application
invariant. Explicit puzzle conditions and runtime checks remain necessary.

The crate is `no_std`; the annotation macro uses `syn` only to validate that its
input is a function, preserves its tokens, and adds export metadata. Rustc
performs type and borrow checking before typed MIR extraction. Only the
compiler's documented subset can be reached from an annotated entrypoint.
The current compiler uses a pinned nightly, not a custom Rust target or a
stable rustc extension API.
