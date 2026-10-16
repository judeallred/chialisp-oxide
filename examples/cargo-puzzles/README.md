# Ordinary Cargo consumer fixture

**WIP prototype. Not ready for use. Do not use for real funds or production puzzles.**

This is an independent Cargo package named `oxide-cargo-puzzles`. Its local
`[workspace]` keeps the consumer fixture out of the compiler's workspace.
`oxide-sdk` is an actual path dependency, and its `#[oxide_sdk::puzzle]`
attribute marks two ordinary functions: `pay` and `guarded_add`.

From the repository root:

```sh
cargo test --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo test --manifest-path examples/cargo-puzzles/Cargo.toml --locked --features strict-cap
cargo oxide check --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo oxide build --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo oxide build --manifest-path examples/cargo-puzzles/Cargo.toml --locked --features strict-cap
```

`build.rs` executes with normal host `std`, writes a Rust constant under Cargo's
`OUT_DIR`, and `include!` brings it into the library. The default limit is 1,000;
feature `strict-cap` changes it to 100. Input 101 therefore distinguishes the
two feature configurations for both marked functions. The source cannot be
compiled as a standalone file without Cargo's generated environment.

`pay` returns `ASSERT_MY_AMOUNT 1000` plus a fixed-destination `CREATE_COIN`.
Its amount choice is permissionless; an unpaid remainder becomes a fee.
`guarded_add` requires each input at most the generated limit and returns their
sum; it is a pure function, not a spendable coin puzzle.
`native_description` deliberately uses allocation, formatting, references, and
iteration. It remains ordinary native Rust because it is unannotated and
unreachable from the marked functions.

Build `cargo-oxide` and `oxide-driver` from the repository root and put
`target/debug` on `PATH` before invoking `cargo oxide`. The native tests verify
feature/build-script behavior and function results. The additional Cargo
integration checks the supported CLVM subset and compiles both marked exports;
a successful `cargo test` alone does not prove lowering or ledger validity.

Successful builds create an immutable bundle beneath this consumer workspace's
`target/oxide/artifacts/`. Follow `latest.json` to the manifest and each export's
artifacts. Failed builds preserve the prior successful bundle. The integration
regressions execute the exact payment-condition and arithmetic outputs in both
optimizer modes, including the feature-specific cap; these are prototype tests,
not a production security review.

The SDK is not published to crates.io. This fixture uses
`oxide-sdk = { path = "../../crates/oxide-sdk" }`. A separate checkout can use a
Git dependency on `https://github.com/judeallred/chialisp-oxide` pinned to a
reviewed full commit containing the SDK. Do not substitute an invented registry
version or follow an unreviewed moving branch for reproducible experiments.
