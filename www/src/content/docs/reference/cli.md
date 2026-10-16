---
title: Compiler commands
description: Use cargo oxide for annotated Cargo libraries and the standalone harness for corpus comparison and VM execution.
---

## Cargo authoring prototype

**WIP prototype. Not ready for use. Do not use for real funds or production puzzles.**

Build the two local binaries with the repository's pinned nightly, then put
them on `PATH`. Cargo discovers `cargo-oxide` as the `cargo oxide` subcommand:

```sh
cargo build --locked -p cargo-oxide -p oxide-driver
export PATH="$PWD/target/debug:$PATH"
cargo oxide check --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo oxide build --manifest-path examples/cargo-puzzles/Cargo.toml --locked
```

`./scripts/cargo-oxide` builds and invokes the same local command without the
`PATH` setup. The SDK and binaries are unpublished. Local installation is
possible with `cargo install --path crates/cargo-oxide --locked` and
`cargo install --path crates/oxide-driver --locked`; keep the pinned compiler
components installed. See the [quickstart](/chialisp-oxide/start/quickstart/).

Ordinary `cargo check` and `cargo test` still check and test the native Rust
library. `cargo oxide check` additionally checks every marked export's reachable
CLVM subset and backend lowering. It removes its temporary artifacts rather
than publishing a bundle. `cargo oxide build` publishes a successful bundle.

The compiler analyzes a normal Cargo library with `#[oxide_sdk::puzzle]`
functions. Cargo supplies dependencies, features, edition, build-script output,
and compiler arguments. This follows Cargo's
[external subcommand convention](https://doc.rust-lang.org/cargo/reference/external-tools.html).
There is no implemented `--target clvm` target.

### Package and build options

The prototype selects one library package at a time. `--manifest-path` selects
a manifest; `-p` selects a package and `--lib` explicitly selects its library.
Feature options include `--features`, `--no-default-features`, and
`--all-features`. Cargo's `--locked`, `--offline`, `--frozen`, `--release`,
`--profile`, `--jobs`, verbosity, and quiet options are forwarded by the wrapper.

```sh
cargo oxide check --manifest-path examples/cargo-puzzles/Cargo.toml --locked --features strict-cap
cargo oxide build --manifest-path examples/cargo-puzzles/Cargo.toml --locked --no-optimize
```

`--no-optimize` disables Rue LIR optimization; it does not disable Rust checks,
ABI validation, or bounded arithmetic. Existing compiler wrappers are rejected
rather than silently composed with the extraction wrapper.

### Output bundles

The default output root is the selected package workspace's
`target/oxide/artifacts/`. `--output-dir PATH` chooses another root. Each
successful build publishes a new immutable bundle containing `manifest.json`
and each export's typed IR, serialized CLVM hex, disassembly, and artifact
metadata. `latest.json` atomically points to the new bundle's manifest. Failed
builds leave the previous successful bundle intact.

The manifest records package selection, compiler and driver identity, Cargo
options, Rue revision, and paths to the entries. Follow those recorded paths;
do not rely on a particular generated entry filename. This is build metadata,
not a complete hash of all source files and transitive inputs.

The separate extraction directory is keyed by the driver binary's SHA-256.
The selected library is re-extracted even when Cargo would otherwise consider
it fresh; dependency builds remain cached for that driver identity. An explicit
host analysis target separates library extraction from native build scripts
and procedural macros. It is not a CLVM machine target.

## Internal standalone comparison harness

The nine-puzzle paired corpus still uses `./scripts/oxide`. This command builds
the pinned driver and comparison CLI before invoking it. Run it from the
repository root. After a successful build, `target/debug/oxide` invokes that
CLI directly. This harness remains useful for compiler and ABI experiments;
it is separate from the Cargo authoring flow above.

### Build a fixture

```sh
./scripts/oxide build puzzles/capped-payment/puzzle.rs --output target/capped
./scripts/oxide build puzzles/capped-payment/puzzle.rs --output target/capped-unoptimized --no-optimize
```

Here `--output` is a file prefix. The command emits `.ir.json`, `.clvm.hex`,
`.clvm.txt`, and `.artifact.json`. The artifact includes byte count, tree hash,
backend options, hex, and disassembly, and is printed to standard output.

### Execute serialized CLVM

```sh
./scripts/oxide run target/capped.clvm.hex --solution '(500)' --max-cost 1000000
./scripts/oxide run target/capped.clvm.hex --solution '(1001)' --max-cost 1000000
```

The first input satisfies the cap; the second rejects. `run` prints a JSON
execution report with outcome, successful cost when available, and diagnostics.
Rejection exits with status 2. The default cost ceiling is 1,000,000; set it
explicitly in reproducible experiments.

A CLVM result is not a ledger transaction. `run` cannot establish that
signatures, amount assertions, or time conditions hold in a spend bundle.
Use the ledger simulator for those obligations.

### Regenerate the registered corpus

```sh
./scripts/oxide validate --output corpus/validation.json
```

This extracts each registered fixture, compiles both Rust optimizer modes and
an independent Chialisp reference, compares named and generated inputs against
native execution, and tests the optimized coin puzzles in the ledger simulator.
A divergence fails the run. These fixtures still use the standalone harness;
the Cargo consumer tests are additional evidence rather than a migration of
this corpus.

The report embeds exact sources, declared source dependencies, and runtime
versions. The documentation gate rejects stale source strings or dependency
hashes, changed manifest cases, disagreed outcomes, failed ledger checks, or
stale formal inputs.

### Inspect frontend IR directly

The standalone driver selects a free function named `puzzle` by default:

```sh
cargo build -p oxide-driver --locked
target/debug/oxide-driver puzzles/bounded-counter/puzzle.rs --output target/counter.ir.json
```

The driver accepts `--entry NAME` for another function. That standalone option
is not an `oxide build` flag; Cargo mode instead discovers annotated exports.
Keep compiler-private APIs isolated in the driver when investigating diagnostics
or extending the subset.
