# Negative Cargo consumer fixture

**WIP prototype. Not ready for use. Do not use for real funds or production puzzles.**

Package `oxide-cargo-invalid` is a separate local workspace with a real
`oxide-sdk` path dependency. It supplies two distinct failure boundaries:

- Default: `unsupported_divide` is valid Rust and its ordinary native test
  passes. Extracting its marked body for CLVM must fail because division has
  no accepted lowering. The Cargo integration must not publish a new successful
  artifact bundle after that failure; any prior successful bundle is preserved.
- `--features type-error`: `wrong_domain` passes a `CoinId` to `create_coin`,
  which requires a `PuzzleHash`. Ordinary Rust checking must fail with `E0308`
  before lowering; the integration must not hide that compiler failure.

From the repository root:

```sh
cargo test --manifest-path examples/cargo-invalid/Cargo.toml --locked
cargo oxide check --manifest-path examples/cargo-invalid/Cargo.toml --locked
cargo check --manifest-path examples/cargo-invalid/Cargo.toml --locked --features type-error
```

The second and third commands intentionally fail at different boundaries.
Do not run this package's invalid
feature as though it were expected to be a green native test configuration.
The SDK is a local unpublished crate; no crates.io version is implied.
