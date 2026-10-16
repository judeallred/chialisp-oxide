---
title: Try the Cargo authoring prototype
description: Use an ordinary Cargo package, the unpublished SDK, native tests, and annotated CLVM exports in the WIP prototype.
---

**WIP prototype. Not ready for use. Do not use for real funds or production puzzles.**

The prototype uses an ordinary Cargo library with a `chialisp-oxide` dependency and
`#[chialisp_oxide::puzzle]` annotations. Cargo handles packages, features, build scripts,
generated code, and compiler arguments. chialisp-oxide additionally checks the reachable
CLVM subset and exports each marked function. This interface remains experimental.

Clone [the repository](https://github.com/judeallred/chialisp-oxide), then run
these commands from its root.

## Install the pinned toolchain

Install [rustup](https://rustup.rs/) if it is not available, then let the checked-in
toolchain file select `nightly-2026-07-14` and its compiler components:

```sh
rustup show
rustup component add rustc-dev llvm-tools-preview rustfmt clippy
cargo build --locked -p cargo-chialisp-oxide -p chialisp-oxide-driver
export PATH="$PWD/target/debug:$PATH"
```

This pin reports Rust 1.99.0-nightly. The driver uses compiler-private rustc
interfaces; no `RUSTC_BOOTSTRAP` is needed. The SDK and user library do not need
compiler-private features. Keep the driver, compiler components, and pin aligned.

## Check and test an ordinary Cargo package

```sh
cargo check --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo test --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo test --manifest-path examples/cargo-puzzles/Cargo.toml --features strict-cap --locked
```

The consumer package has two marked functions: `pay` returns a fixed-destination
payment, and `guarded_add` returns a pure arithmetic result. `build.rs` runs as a
normal host program and generates their limit under `OUT_DIR`; `include!` reads
that generated Rust. The default cap is 1,000; `strict-cap` changes it to 100.
Unannotated native-only code in the same library uses `Vec`, iteration, and
formatting. Those operations remain outside the exported call graph.

## Export the annotated functions to CLVM

```sh
cargo chialisp-oxide build --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo chialisp-oxide check --manifest-path examples/cargo-puzzles/Cargo.toml --features strict-cap --locked
```

`cargo check` proves ordinary Rust compilation, not CLVM compatibility.
`cargo chialisp-oxide check` also validates and lowers the marked reachable bodies.
`cargo chialisp-oxide build` publishes an immutable artifact bundle only after success.

For this example, output defaults to
`examples/cargo-puzzles/target/chialisp-oxide/artifacts/`. Its `latest.json` points to
`<bundle>/manifest.json`, which records the package, compiler, options, and
per-entry IR/CLVM artifacts. A failed build leaves the prior successful bundle
intact. `--output-dir PATH` selects another artifact destination.

No crates are published to crates.io. The example uses a real path dependency
on `crates/chialisp-oxide`, which re-exports the annotation from `chialisp-oxide-macros`.
A separate package can use a reviewed pinned Git dependency. See
[typed SDK authoring](/chialisp-oxide/tutorials/typed-sdk/) and
[compiler commands](/chialisp-oxide/reference/cli/).

This is a Cargo-driven extractor and CLVM backend adapter. It does not implement
`cargo build --target clvm`; target configuration alone does not add a code
generator or establish supported library semantics.

## Build and validate the real Cargo corpus

The laboratory fixtures are modules of the `chialisp-oxide-puzzles` Cargo library.
They use the real SDK dependency and annotated exports, just like the consumer
example. Native tests and CLVM compilation use the same package:

```sh
cargo test --manifest-path puzzles/Cargo.toml --locked
cargo chialisp-oxide build --manifest-path puzzles/Cargo.toml --locked
cargo build --workspace --locked
./scripts/chialisp-oxide validate --output corpus/validation.json
python3 corpus/validate_inventory.py
```

Select an export such as `chialisp_oxide_puzzles::capped_payment::puzzle` from the bundle
manifest under `target/chialisp-oxide/artifacts/`. The
[command reference](/chialisp-oxide/reference/cli/#execute-a-selected-export)
shows how to resolve the current bundle and execute its CLVM.

Validation extracts all declared exports in one Cargo invocation and links the
same library into the native oracle. It checks named adversarial vectors,
generated inputs, and optimized coin spends. `puzzles/authoring.json` declares
the package, entry/source mapping, and local Cargo inputs; validation and proof
reports record their hashes. The site rejects stale records.

Open the [capped-payment report](/chialisp-oxide/lab/capped-payment/) to inspect
the exact sources, vectors, and metrics for the checked-in run. Direct
standalone-file extraction remains available only for compiler regressions and
research, rather than the validated corpus authoring workflow.

## Run the project checks

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/check-proofs.py
```

The proof check requires the external Z3 executable; formal proof tooling has
its own explicitly documented scope. See [formal verification](/chialisp-oxide/verification/formal/).

For the documentation, install Node.js 24 or later and pnpm 10.28.2:

```sh
cd www
pnpm install --frozen-lockfile
pnpm check
pnpm build
pnpm test:site
pnpm preview
```

Search uses the production Pagefind index. Test it in the built preview rather
than assuming development-server search demonstrates a functioning index.

## If compilation rejects your code

Read the reported Rust source location, then check
[supported Rust](/chialisp-oxide/reference/supported-rust/). A Rust program can
type-check successfully and still require an unsupported CLVM lowering. Do not
work around a diagnostic by removing a policy assertion; reduce the program to
a minimal example and add a semantic test when extending the compiler.
