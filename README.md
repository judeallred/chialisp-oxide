# chialisp-oxide

> **WIP PROTOTYPE — NOT READY FOR USE.** This is an experimental compiler and
> validation research project. **Do not use it for real funds, production
> puzzles, wallets, or deployed contracts.** Tests and model-level checks do not
> establish production safety.

Compile a supported subset of real Rust into CLVM using Rue's optimizer and
backend. Test puzzles against native Rust, independently compiled Chialisp, and
the chia-wallet-sdk transaction simulator.

## Explore the Cargo prototype

Use an ordinary Rust library with the real, unpublished `oxide-sdk` dependency.
Mark exported functions with `#[oxide_sdk::puzzle]`. Ordinary `cargo check` and
`cargo test` provide Rust checking and native tests; `cargo oxide check` and
`cargo oxide build` additionally validate the reachable CLVM subset and compile
the marked functions through Rue.

From this checkout, with rustup installed:

```sh
rustup show
cargo build --locked -p cargo-oxide -p oxide-driver
export PATH="$PWD/target/debug:$PATH"
cargo check --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo test --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo oxide build --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo oxide check --manifest-path examples/cargo-puzzles/Cargo.toml --features strict-cap --locked
```

The [consumer package](examples/cargo-puzzles) has two annotated entries, a real
SDK path dependency, feature-controlled limits, a host build script generating
code through `OUT_DIR`/`include!`, and unannotated native-only allocation code.
Neither `oxide-sdk`, `oxide-macros`, nor `cargo-oxide` is published to crates.io.
Use the checkout's path dependencies or a reviewed pinned Git revision. Local
installation and output options are documented in [cargo-oxide](crates/cargo-oxide/README.md).

The compiler is pinned to **nightly-2026-07-14**, reporting Rust 1.99.0-nightly,
with `rustc-dev` and `llvm-tools-preview`. No `RUSTC_BOOTSTRAP` is required.
Cargo supplies features, dependencies, generated code, and actual rustc
arguments. Build scripts and procedural macros remain normal host programs.

Each successful export build creates an immutable bundle under the consumer
workspace's `target/oxide/artifacts/<bundle>/manifest.json`, with typed IR,
serialized CLVM, disassembly, sizes, and tree hashes. `latest.json` points to the
most recent successful bundle. Failed builds preserve the previous successful
bundle instead of publishing partial artifacts.

This is a Cargo-driven MIR extractor and CLVM backend adapter, **not** a rustc
`--target clvm` implementation. A target configuration alone does not provide
code generation, runtime layouts, linking, or a CLVM SDK. See the
[authoring research and backend rationale](docs/rust-authoring-research.md).
General cross-crate Rust libraries remain outside the supported subset; the
initial external calls are restricted to vetted concrete SDK helpers.

## Reproduce the paired corpus harness

The existing nine-puzzle corpus still uses the standalone-file research harness.
Its coin fixtures use the compatibility path in `sdk/oxide.rs` to the same
ordinary domain definitions as the real SDK crate.
This evidence does not imply that the corpus has migrated to the Cargo flow.

```sh
./scripts/oxide build puzzles/capped-payment/puzzle.rs --output target/capped
./scripts/oxide run target/capped.clvm.hex --solution '(500)'
./scripts/oxide validate --output corpus/validation.json
```

Z3 is required for the formal experiments (`brew install z3` on macOS,
`apt install z3` on Ubuntu). The backend directly integrates Rue compiler
source; generated Rue text is optional.

## What is validated

Nine paired Rust/Chialisp examples: fixed and capped payments, split payments,
timelocks, one/two signatures, owner/recovery branches, a boolean gate, and a pure
bounded counter. The corpus checks native execution, optimized/unoptimized CLVM,
73 explicit vectors, 3,063 generated integer cases, and 30 complete-spend scenarios.
Some examples are deliberately permissionless. The boolean gate is not
identity/authentication. See each puzzle's security scope before using it.

```sh
cargo build --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 corpus/validate_inventory.py
python3 scripts/check-proofs.py
python3 scripts/check-ir-proofs.py --output corpus/proofs.json
```

The verifier generates SMT from actual source-derived IR, checks independently
specified bounds and acceptance, detects a removed guard, and replays witnesses
through native Rust and both CLVM optimization modes. These are **IR-model
proofs**, not proofs of compiler preservation or absence of all security flaws.

## Documentation

The searchable Astro/Starlight site is in [www](www). It includes tutorials,
source comparisons, a puzzle laboratory, measured bytecode/cost differences,
verification results, supported Rust/ABI references, and a pinned coverage
inventory. Run `pnpm --dir www install && pnpm --dir www dev` for a local preview.
GitHub Pages deployment builds www with pnpm and deploys www/dist through GitHub
Actions at https://judeallred.github.io/chialisp-oxide/.

Puzzle bodies use named condition constructors, return aliases, policy
constants, and explicit amount/time units. Public test-key bytes live in a
separate fixture module; documentation keeps their complete recorded source in
expandable drawers. See the [readability rationale](docs/readability.md).

- [Project goals](goals.md) and [approach](docs/compiler-approach.md)
- [Verification strategy](docs/verification.md)
- [Rust frontend and limitations](crates/oxide-driver/README.md)
- [Source-derived verifier](crates/oxide-verifier/README.md)
- [Simulator and consensus scope](crates/oxide-simulator/README.md)
- [Initial backend experiment](experiments/backend/README.md)
- [Chialisp tutorials/reference](https://chialisp.com/)

The compiler supports restricted single-field transparent newtypes, allowing
nominal domain checks through the real [SDK crate](crates/oxide-sdk) and the
corpus's [compatibility module](sdk/README.md). General structs
and enums, loops, recursion, allocation, arbitrary external calls, references, casts,
division/remainder, and other unsupported MIR remain rejected.
The 98-entry tutorial/CHIP inventory is a roadmap; it is not a claim that all
standard puzzles have been reimplemented. No production security audit has been
completed. The vendored Rue identity-product fix is reported upstream as
[xch-dev/rue#108](https://github.com/xch-dev/rue/issues/108), with a regression test
and patch.
