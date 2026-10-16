# cargo-oxide (WIP prototype — not ready for use)

A Cargo subcommand that compiles `#[oxide_sdk::puzzle]` functions from one normal
Rust library to CLVM. No crates are published. Do not use with real funds.

From this checkout:

```sh
cargo build --locked -p cargo-oxide -p oxide-driver
export PATH="$PWD/target/debug:$PATH"
cargo test --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo oxide build --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo oxide check --manifest-path examples/cargo-puzzles/Cargo.toml --features strict-cap --locked
```

`./scripts/cargo-oxide` also builds and runs the subcommand from this checkout.
To install locally, use `cargo install --path crates/cargo-oxide --locked` and
`cargo install --path crates/oxide-driver --locked`. Keep the pinned nightly and
its compiler components installed. Add `oxide-sdk` as a path dependency as the
example does; a Git dependency is another option for a separate project.

The attribute preserves the function body and marks an export. Ordinary
`cargo check` and `cargo test` provide Rust checking and native tests.
`cargo oxide check` additionally validates the reachable CLVM subset and lowers
it through Rue. Cargo controls dependencies, features, editions, build scripts,
generated code, and `--extern` arguments. No `--target clvm` target is implemented.

Build output lives under the package workspace's `target/oxide/artifacts` by
default. `latest.json` points to an immutable bundle containing a manifest,
typed IR, CLVM hex, disassembly, and tree hashes for every exported function.
`--output-dir PATH` changes that destination. A failed build leaves the previous
successful bundle intact; it never publishes a partial new bundle.

Extraction uses separate Cargo output/intermediate directories keyed by the driver
binary's SHA-256, and forces just the selected package to rerun, including when
Cargo would otherwise call it fresh. Dependencies remain cached under that driver
identity. Builds are serialized per extraction directory. The explicit host
analysis target separates puzzle-library compilation from native build scripts
and procedural macros; it is not a CLVM target specification.

The manifest records compiler/driver identity, package selection, Cargo options,
and compiled artifacts. It does not yet fingerprint every source, generated file,
or transitive dependency; it is build metadata, not a complete reproducibility proof.

Current limits: one library package at a time; no existing compiler wrappers;
only concrete vetted SDK calls and local helpers; no arbitrary Rust dependency
bodies, generics, allocation, references, recursion, loops, or unsupported integer
operations in exported code. Normal native code outside an export's reachable
call graph can use broader Rust. See the [research rationale](../../docs/rust-authoring-research.md)
and [frontend limitations](../oxide-driver/README.md).
