---
title: Compiler commands
description: Build annotated Cargo exports, validate the genuine Cargo corpus, select serialized artifacts, and inspect regression-only standalone extraction.
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

## Build the validated puzzle library

The corpus itself is the ordinary `oxide-puzzles` workspace package, declared
in `puzzles/Cargo.toml`. Its modules export annotated functions such as
`oxide_puzzles::capped_payment::puzzle`:

```sh
cargo test --manifest-path puzzles/Cargo.toml --locked
cargo oxide build --manifest-path puzzles/Cargo.toml --locked
cargo oxide check --manifest-path puzzles/Cargo.toml --locked --no-optimize
```

The workspace output root is `target/oxide/artifacts/`. Read `latest.json`,
then its bundle manifest, and select the full export name. The entry records
its IR, serialized CLVM, and artifact paths relative to that bundle.

### Execute a selected export

After building the workspace binaries and puzzle bundle, this example selects
the capped-payment export from the real manifest and runs its CLVM:

```sh
python3 - <<'PYTHON'
import json
from pathlib import Path
import subprocess

root = Path('target/oxide/artifacts')
pointer = json.loads((root / 'latest.json').read_text())
manifest_path = root / pointer['manifest']
manifest = json.loads(manifest_path.read_text())
entry = next(item for item in manifest['entries']
             if item['entry'] == 'oxide_puzzles::capped_payment::puzzle')
subprocess.run(['target/debug/oxide', 'run',
                str(manifest_path.parent / entry['clvm']),
                '--solution', '(500)', '--max-cost', '1000000'], check=True)
PYTHON
```

`(500)` satisfies the cap; `(1001)` rejects. `run` prints JSON with outcome,
successful cost when available, and diagnostics. Rejection exits with status 2.
The default cost ceiling is 1,000,000; set it explicitly for reproducible runs.

A CLVM result is not a ledger transaction. `run` cannot establish that
signatures, amount assertions, or time conditions hold in a spend bundle.
Use the ledger simulator for those obligations.

## Replay a linked native function

```sh
target/debug/oxide native bounded-counter --solution '(999)'
target/debug/oxide native bounded-counter --solution '(1000)'
```

The first command returns an accepted JSON outcome with `output_hex` equal to
`8203e8`, the serialized integer 1,000. The second returns a rejected policy
outcome. Both policy outcomes exit with status 0; invocation errors, unknown
slugs, and stale native-oracle inputs exit nonzero.

This helper applies the canonical codec and invokes the actual linked Cargo
function. It does not execute CLVM or validate a ledger spend. Its compiled
input snapshot is checked against `puzzles/authoring.json`; a deliberately
changed declared input has been tested to reject until the native oracle is
rebuilt. This prevents changed declared Cargo inputs from silently using an old
native policy, without claiming a complete transitive dependency fingerprint.

## Regenerate the registered corpus

```sh
cargo build --workspace --locked
./scripts/oxide validate --output corpus/validation.json
```

Validation builds the Cargo puzzle package once, selects the declared exports,
compiles each IR program in both backend modes, and compiles the authored
Chialisp references independently. The native oracle links that same Cargo
library. Named and generated inputs compare native output/rejection with all
three CLVM paths; optimized coin puzzles also run in the ledger simulator.
A divergence fails the run.

`puzzles/authoring.json` declares package, library, export/source mapping, and
local Cargo inputs. Validation and source-derived proof reports record the
same input hashes. The documentation gate checks those fingerprints, exact
fixture/dependency sources, manifest cases, backend agreement, ledger checks,
and formal inputs. These explicit local records are not a complete fingerprint
of all transitive Cargo dependencies.

## Standalone compiler regressions

Direct source-file extraction remains a research and regression interface.
It is no longer the corpus authoring path. Use a standalone fixture rather
than passing a Cargo module that requires external dependency metadata:

```sh
./scripts/oxide build crates/oxide-driver/tests/fixtures/guarded_payment.rs --output target/research
cargo build -p oxide-driver --locked
target/debug/oxide-driver crates/oxide-driver/tests/fixtures/guarded_payment.rs --output target/research.ir.json
```

The standalone driver selects the free function named `puzzle` by default and
accepts `--entry NAME`. That option is not an `oxide build` flag; Cargo mode
instead discovers annotated exports. Keep compiler-private APIs isolated in
the driver when investigating diagnostics or extending the subset.
