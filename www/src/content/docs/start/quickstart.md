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

Before installing anything, read the [coin model](/chialisp-oxide/start/coin-model/)
if puzzle hashes, solutions and conditions are unfamiliar. This page ends with
a local build; it does not deploy or spend a coin.

## Install the pinned toolchain

Install [rustup](https://rustup.rs/) if it is not available, then let the checked-in
toolchain file select `nightly-2026-07-14` and its compiler components:

```sh
rustup show
rustup component add rustc-dev llvm-tools-preview rustfmt clippy
cargo build --locked -p cargo-chialisp-oxide -p chialisp-oxide-driver -p chialisp-oxide-cli
export PATH="$PWD/target/debug:$PATH"
```

The three packages provide the Cargo subcommand, compiler driver and
`chialisp-oxide` execution CLI used to replay bundle artifacts in the command
reference.

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

Default builds also attach a bounded receipt of selected-library file reads
and named launcher snapshots, including generated Rust. The
[offline package registry](/chialisp-oxide/reference/package-registry/)
can import those exact declared bytes without the original checkout or compiler.
The receipt does not capture the complete build graph or prove source/IR/CLVM
equivalence. `build --artifact-only` explicitly skips source capture and emits
the older bundle format; `check` records no receipt.

No crates are published to crates.io. The example uses a real path dependency
on `crates/chialisp-oxide`, which re-exports the annotation from `chialisp-oxide-macros`.
A separate package can use a reviewed pinned Git dependency. See
[typed SDK authoring](/chialisp-oxide/tutorials/typed-sdk/) and
[compiler commands](/chialisp-oxide/reference/cli/).

This is a Cargo-driven extractor and CLVM backend adapter. It does not implement
`cargo build --target clvm`; target configuration alone does not add a code
generator or establish supported library semantics.

## Execute the program you just built

The native tests called Rust functions. Now execute the emitted `pay` program
in CLVM. Python 3 is used only to follow the bundle manifest; it does not compile
the puzzle. Save this small runner from the repository root:

```sh
mkdir -p target/quickstart
cat > target/quickstart/run-pay.py <<'PYTHON'
import json
from pathlib import Path
import subprocess
import sys

root = Path('examples/cargo-puzzles/target/chialisp-oxide/artifacts')
pointer = json.loads((root / 'latest.json').read_text())
manifest_path = root / pointer['manifest']
manifest = json.loads(manifest_path.read_text())
entry = next(item for item in manifest['entries']
             if item['entry'] == 'chialisp_oxide_cargo_puzzles::pay')
program = manifest_path.parent / entry['clvm']
print(f'Running {program}', flush=True)
result = subprocess.run(['target/debug/chialisp-oxide', 'run', str(program),
                         '--solution', sys.argv[1], '--max-cost', '1000000'])
sys.exit(result.returncode)
PYTHON

python3 target/quickstart/run-pay.py '(101)'
python3 target/quickstart/run-pay.py '(1001)'
python3 target/quickstart/run-pay.py '(0x0001)'
```

For the default build, `(101)` succeeds with the condition tree
`((73 1000) (51 0x4242…4242 101))`; the destination atom contains 32 `42` bytes.
The CLI prints its serialized `output_hex` and VM cost. The amount assertion
requests an input of 1,000 mojos; the output requests 101. In an otherwise
balanced single-input spend, the remaining 899 mojos become a fee. This policy
does not automatically return change.

The other two commands deliberately exit with status 2. Inspect the JSON:
expect a program failure with code `raise`. `1001` exceeds the cap;
`0x0001` pads the integer 1 and fails the canonical input decoder before the
body runs. Resource or infrastructure failures are inconclusive, rather than
evidence that the policy rejected these inputs. A successful VM result still
does not check the input coin's actual amount or submit a spend.

## Change a policy and predict its effect

Use the example's existing Cargo feature as a first controlled edit. Predict
what happens to `(101)` when the generated cap becomes 100, then run:

```sh
cargo test --manifest-path examples/cargo-puzzles/Cargo.toml --features strict-cap --locked
cargo chialisp-oxide build --manifest-path examples/cargo-puzzles/Cargo.toml --features strict-cap --locked
python3 target/quickstart/run-pay.py '(100)'
python3 target/quickstart/run-pay.py '(101)'
```

`(100)` succeeds; `(101)` now rejects with `raise`. The feature selects generated
Rust through `build.rs`, so both native tests and CLVM compilation must receive
it. The earlier `check --features strict-cap` did not publish a bundle. This
`build` does: the runner resolves `latest.json` again for every invocation,
while the old immutable default bundle remains available. Executing an old
`.hex` file would test the old policy even after changing the source or features.

To restore the default lesson, build again without `--features strict-cap`.
Before designing another payment, decide whether callers should choose an
amount at all, who authorizes it, and whether any remainder must be returned.
Those choices need explicit policy and spend tests.

## What you have built

You now have a native-tested Cargo library and an immutable CLVM artifact bundle.
The `.hex` files are programs, not transactions; no address has been funded.
The consumer's `pay` accepts a caller-selected amount and pays the fixture
`0x42…42` destination. That destination is not a wallet you control. Do not fund it.

Continue with [your first fixed payment](/chialisp-oxide/tutorials/first-payment/)
to read a smaller fixed policy, then [build to coin spend](/chialisp-oxide/start/build-to-spend/)
for a complete wallet-free simulator exercise and the separate Testnet11 route.
The sections below are for contributors inspecting the full corpus and site;
they are not prerequisites for that learning path.

## Contributor task: validate the real Cargo corpus

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

## Contributor task: run the project checks

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
