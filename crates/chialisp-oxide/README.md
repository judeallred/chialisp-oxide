# chialisp-oxide

Work-in-progress Rust authoring crate for `cargo chialisp-oxide`. It provides the
`#[chialisp_oxide::puzzle]` export annotation, transparent domain types, and pure
condition constructors that remain ordinary Rust functions in native tests.
It has not been published to crates.io; use a local path dependency from this
repository. See `examples/cargo-puzzles` for a complete Cargo consumer.

```rust
use chialisp_oxide::{Amount, CreateCoin, PuzzleHash, create_coin};

const PAYMENT_CAP: Amount = Amount::from_mojos(1000);

#[chialisp_oxide::puzzle]
pub fn pay(destination: PuzzleHash, amount: Amount) -> CreateCoin {
    assert!(amount.is_at_most(PAYMENT_CAP));
    create_coin(destination, amount)
}
```

`PuzzleHash`, `CoinId`, `PublicKey`, `Message32`, `Amount`, and `Timestamp` are
distinct Rust types. Passing a `CoinId` where a `PuzzleHash` is required fails
normal Rust type checking. The CLVM ABI erases their wrappers to fixed bytes or
a bounded integer; these are source-level domains, not runtime nominal tags.
Their public constructors establish no cryptographic validity or application
invariant. Explicit puzzle conditions and runtime checks remain necessary.

These small value types are `Copy`. `Amount::from_mojos` and `.mojos()` make
monetary units explicit; `.is_at_most` compares two amounts. Use ordinary
checked Rust arithmetic on `.mojos()` when calculating a new amount.
`Timestamp::from_unix_seconds`, `.unix_seconds()`, and `.is_at_or_after` use
absolute Unix time in seconds. These methods take values, keeping references
and trait dispatch outside the current puzzle subset.

Name policy and fixture values with ordinary typed Rust constants, including
`const RECIPIENT: PuzzleHash = PuzzleHash([0x42; 32]);`. The frontend evaluates
bounded fixed-byte constants with rustc and verifies their initialized,
provenance-free representation. It does not support arbitrary aggregate or
pointer constants. `create_coin`, `agg_sig_me`, `assert_my_amount`, and
`assert_seconds_absolute` supply the wire opcodes; puzzle bodies need not spell
those numbers. The tuple condition aliases preserve their existing CLVM ABI.

The crate and standalone compatibility module `sdk/chialisp_oxide.rs` both use the
ordinary Rust module source `crates/chialisp-oxide/src/domain.rs`, so their domain
types and constructors cannot drift into separate implementations.

The crate is `no_std`; the annotation macro uses `syn` only to validate that its
input is a function, preserves its tokens, and adds export metadata. Rustc
performs type and borrow checking before typed MIR extraction. Only the
compiler's documented subset can be reached from an annotated entrypoint.
The current compiler uses a pinned nightly, not a custom Rust target or a
stable rustc extension API.

For programs that must manipulate arbitrary CLVM data, the facade also provides
immutable `Tree<'a>::Atom(&'a [u8])`, `Tree::Pair(&'a Tree, &'a Tree)`, and
`quote(&Tree)`. These are ordinary Rust declarations and helper bodies; the
compiler lowers their real MIR. A separately declared
`#[chialisp_oxide::puzzle(environment = "raw")]` passes the complete environment
to exactly one tree parameter. The default annotation keeps strict typed-list
decoding. Raw atom bytes carry no implicit amount, hash, condition, or program
validation. See the actual [canonical factory source](../../puzzles/p2-conditions/puzzle.rs)
and its [walkthrough](https://judeallred.github.io/chialisp-oxide/tutorials/p2-conditions/).
This narrow representation does not support general enums, mutable references,
allocation, recursive Rust calls, or runtime program evaluation.
