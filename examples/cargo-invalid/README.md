# Negative Cargo consumer fixture

**WIP prototype. Not ready for use. Do not use for real funds or production puzzles.**

Package `chialisp-oxide-cargo-invalid` is a separate local workspace with a real
`chialisp-oxide` path dependency. It supplies two distinct failure boundaries:

- Default: `unsupported_bool_cast` converts a comparison's `bool` result to
  `u8`. It is valid Rust and its ordinary native test passes. The CLVM subset
  accepts casts between the ten fixed-width integer types (`i8`–`i128` and
  `u8`–`u128`), but does not admit a Boolean source to that integer-cast route.
  Extraction must reject this body with a subset diagnostic after native Rust
  checking succeeds. The Cargo integration must not publish a new successful
  artifact bundle after that failure; any prior successful bundle is preserved.
- `--features type-error`: `wrong_domain` passes a `CoinId` to `create_coin`,
  which requires a `PuzzleHash`. Ordinary Rust checking must fail with `E0308`
  before lowering; the integration must not hide that compiler failure.

From the repository root:

```sh
cargo test --manifest-path examples/cargo-invalid/Cargo.toml --locked
cargo chialisp-oxide check --manifest-path examples/cargo-invalid/Cargo.toml --locked
cargo check --manifest-path examples/cargo-invalid/Cargo.toml --locked --features type-error
```

The second and third commands intentionally fail at different boundaries.
Do not run this package's invalid
feature as though it were expected to be a green native test configuration.
The SDK is a local unpublished crate; no crates.io version is implied.
