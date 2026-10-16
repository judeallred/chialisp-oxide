---
title: Install and compile a puzzle
description: Reproduce the pinned Rust toolchain, compiler artifacts, and corpus validation report.
---

Clone [the repository](https://github.com/judeallred/chialisp-oxide), then run
these commands from its root.

## Install the pinned toolchain

Install [rustup](https://rustup.rs/) if it is not available, then let the checked-in
toolchain file select Rust 1.97.1 and its components:

```sh
rustup show
rustup component add rustc-dev llvm-tools-preview rustfmt clippy
cargo build --workspace --locked
```

The driver uses compiler-private rustc interfaces. `.cargo/config.toml` scopes
`RUSTC_BOOTSTRAP=oxide_driver` to that crate. Other Rust crates use stable APIs.
This pinned integration is not a stable rustc plugin interface; changing the
toolchain requires compiler and semantic regression tests.

## Build the capped payment

```sh
./scripts/oxide build puzzles/capped-payment/puzzle.rs --output target/capped
```

The source accepts a canonical `u64` amount no greater than 1,000 and returns a
condition that checks the input coin's amount plus a payment to a fixed puzzle
hash. Any remainder becomes a fee. This policy is permissionless; it has no
owner key.

This writes `target/capped.ir.json`, `target/capped.clvm.hex`,
`target/capped.clvm.txt`, and `target/capped.artifact.json`. Run a valid input:

```sh
./scripts/oxide run target/capped.clvm.hex --solution '(500)' --max-cost 1000000
```

Inspect the emitted IR and compiled artifacts. Keep
compiler versions, optimization settings, serialized bytes, and puzzle hashes
with a reproducible build; a source-text hash is not a CLVM tree hash.
See [compiler commands](/chialisp-oxide/reference/cli/) for the unoptimized
build mode, output formats, and rejection exit status.

## Reproduce the evidence

```sh
./scripts/oxide validate --output corpus/validation.json
python3 corpus/validate_inventory.py
```

Validation compiles each paired source, checks named adversarial vectors and
generated inputs, and exercises the optimized Rust output in a coin simulator.
The command fails if the compared outcomes diverge. The documentation imports
the generated report; it does not maintain independent hand-entered benchmarks.

Open the [capped-payment report](/chialisp-oxide/lab/capped-payment/) to inspect
the exact vectors and metrics for the checked-in run.

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
