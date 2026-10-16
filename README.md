# chialisp-oxide

> **WIP PROTOTYPE — NOT READY FOR USE.** This is an experimental compiler and
> validation research project. **Do not use it for real funds, production
> puzzles, wallets, or deployed contracts.** Tests and model-level checks do not
> establish production safety.

Compile a supported subset of real Rust into CLVM using Rue's optimizer and
backend. Test puzzles against native Rust, independently compiled Chialisp, and
the chia-wallet-sdk transaction simulator.

## Explore the Cargo prototype

Use an ordinary Rust library with the real, unpublished `chialisp-oxide` dependency.
Mark exported functions with `#[chialisp_oxide::puzzle]`. Ordinary `cargo check` and
`cargo test` provide Rust checking and native tests; `cargo chialisp-oxide check` and
`cargo chialisp-oxide build` additionally validate the reachable CLVM subset and compile
the marked functions through Rue.

From this checkout, with rustup installed:

```sh
rustup show
cargo build --locked -p cargo-chialisp-oxide -p chialisp-oxide-driver
export PATH="$PWD/target/debug:$PATH"
cargo check --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo test --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo chialisp-oxide build --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo chialisp-oxide check --manifest-path examples/cargo-puzzles/Cargo.toml --features strict-cap --locked
```

The [consumer package](examples/cargo-puzzles) has two annotated entries, a real
SDK path dependency, feature-controlled limits, a host build script generating
code through `OUT_DIR`/`include!`, and unannotated native-only allocation code.
Neither `chialisp-oxide`, `chialisp-oxide-macros`, nor `cargo-chialisp-oxide` is published to crates.io.
Use the checkout's path dependencies or a reviewed pinned Git revision. Local
installation and output options are documented in [cargo-chialisp-oxide](crates/cargo-chialisp-oxide/README.md).
The package name is `chialisp-oxide`, its Rust import is `chialisp_oxide`, and
the Cargo command is `cargo chialisp-oxide`; see the
[naming rationale](docs/rust-authoring-research.md#package-import-and-command-names).

The compiler is pinned to **nightly-2026-07-14**, reporting Rust 1.99.0-nightly,
with `rustc-dev` and `llvm-tools-preview`. No `RUSTC_BOOTSTRAP` is required.
Cargo supplies features, dependencies, generated code, and actual rustc
arguments. Build scripts and procedural macros remain normal host programs.

Each successful export build creates an immutable bundle under the consumer
workspace's `target/chialisp-oxide/artifacts/<bundle>/manifest.json`, with typed IR,
serialized CLVM, disassembly, sizes, and tree hashes. `latest.json` points to the
most recent successful bundle. Failed builds preserve the previous successful
bundle instead of publishing partial artifacts.

This is a Cargo-driven MIR extractor and CLVM backend adapter, **not** a rustc
`--target clvm` implementation. A target configuration alone does not provide
code generation, runtime layouts, linking, or a CLVM SDK. See the
[authoring research and backend rationale](docs/rust-authoring-research.md).
General cross-crate Rust libraries remain outside the supported subset; the
initial external calls are restricted to vetted concrete SDK helpers.

## Build and validate the Cargo corpus

The fixtures are modules of the workspace's `chialisp-oxide-puzzles` library.
Every entry uses the real SDK's puzzle annotation. Native tests,
the differential oracle, and CLVM extraction use that same library:

```sh
cargo test --manifest-path puzzles/Cargo.toml --locked
cargo chialisp-oxide build --manifest-path puzzles/Cargo.toml --locked
cargo build --workspace --locked
./scripts/chialisp-oxide validate --output corpus/validation.json
```

Follow `target/chialisp-oxide/artifacts/latest.json` to the bundle manifest, then select
a full entry name such as `chialisp_oxide_puzzles::capped_payment::puzzle` and its artifact
paths. [puzzles/authoring.json](puzzles/authoring.json) declares Cargo inputs and
entry/source mappings; both validation and proof reports include their hashes.
The site checks those records against current files. This declared input set is
not a complete transitive dependency fingerprint.

Standalone file extraction remains a compiler-regression and research tool;
it is no longer the corpus's authoring or native-oracle route.

Z3 is required for the formal experiments (`brew install z3` on macOS,
`apt install z3` on Ubuntu). The backend directly integrates Rue compiler
source; generated Rue text is optional.

## What is validated

Twelve Chialisp/Rust/Rue source comparisons: nine original teaching fixtures covering
payments, timelocks, signatures, recovery, a boolean gate, and a bounded counter;
plus three canonical closed expressions from the pinned Chialisp Intro. The
Intro ports preserve the exact original expressions and ignored environments
through explicit parameterless annotations. The corpus checks native execution,
97 explicit vectors and 3,831 generated integer/tree cases against five CLVM
builds: Chialisp, Rust off/on, and independently compiled Rue off/on. Thirty
complete-spend scenarios run separately in all three languages (90 ledger executions). Only the eight condition-producing
teaching fixtures have ledger scenarios; pure language results are not spends.
Some examples are deliberately permissionless. The boolean gate is not
identity/authentication. See each puzzle's security scope before using it.

```sh
cargo build --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 corpus/validate_inventory.py
python3 corpus/validate_tutorial_examples.py
python3 corpus/validate_published_puzzles.py --self-test
python3 scripts/check-error-study.py
python3 scripts/check-proofs.py
python3 scripts/check-ir-proofs.py --output corpus/proofs.json
```

The verifier generates SMT from actual source-derived IR, checks independently
specified bounds and acceptance, detects a removed guard, and replays witnesses
through native Rust and both CLVM optimization modes. These are **IR-model
proofs**, not proofs of compiler preservation or absence of all security flaws.

## Functionality and source coverage

[The functionality map](docs/chialisp-functionality.md) distinguishes Rust’s
ability to express an operation, support in this compiler, executable witnesses,
and missing preservation proofs. Full Chialisp parity is **not yet established**.
Dynamic trees/lists, arbitrary-precision integers, recursion, hashing and crypto,
currying, and runtime program evaluation remain compiler work; primitive Rust
integers must keep their checked finite semantics.

[The published-source catalog](corpus/published-puzzles.json) enumerates pinned
repository trees and records its exact discovery scope. It includes Chialisp,
Rue, Rust builders, bindings and host tooling with separate classifications;
source discovery does not mean a file has been ported, executed, or audited.

The dedicated [error study](https://judeallred.github.io/chialisp-oxide/verification/error-study/)
executes limited Chialisp test suites that miss concrete bugs, then records the
Rust diagnostics that reject the corresponding source mistakes. It also keeps a
policy bug that Rust still compiles as a negative control.

## Documentation

The searchable Astro/Starlight site is in [www](www). It includes tutorials,
three-language source comparisons, a puzzle laboratory, measured bytecode/cost differences,
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
- [Rust frontend and limitations](crates/chialisp-oxide-driver/README.md)
- [Source-derived verifier](crates/chialisp-oxide-verifier/README.md)
- [Simulator and consensus scope](crates/chialisp-oxide-simulator/README.md)
- [Initial backend experiment](experiments/backend/README.md)
- [Chialisp tutorials/reference](https://chialisp.com/)

The compiler supports restricted single-field transparent newtypes, allowing
nominal domain checks through the real [SDK crate](crates/chialisp-oxide). The
[compatibility module](sdk/README.md) remains for standalone regression fixtures. General structs
and enums, loops, recursion, allocation, arbitrary external calls, references, casts,
division/remainder, and other unsupported MIR remain rejected.
The 98-entry tutorial/CHIP inventory is a roadmap; it is not a claim that all
standard puzzles have been reimplemented. A separate
[example inventory](corpus/tutorial-examples.json) snapshots 12 concrete tutorial
programs, their source spans/license, a naming variant, and supporting workflow
gaps. Three Intro expressions now have finite differential evidence; nine
other programs, the naming variant, and supporting workflows remain unrun.
The parent Intro page remains partial, and no CHIP migration is complete.
See the [canonical Intro walkthrough](https://judeallred.github.io/chialisp-oxide/tutorials/canonical-intro/)
for exact sources, environment contracts, and measured compiler gaps. No production security audit has been
completed. The vendored Rue identity-product fix is reported upstream as
[xch-dev/rue#108](https://github.com/xch-dev/rue/issues/108), with a regression test
and patch.
