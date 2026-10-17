---
title: Compiler commands
description: Build annotated Cargo exports, validate the genuine Cargo corpus, select serialized artifacts, and inspect regression-only standalone extraction.
---

## Cargo authoring prototype

**WIP prototype. Not ready for use. Do not use for real funds or production puzzles.**

Build the two local binaries with the repository's pinned nightly, then put
them on `PATH`. Cargo discovers `cargo-chialisp-oxide` as the `cargo chialisp-oxide` subcommand:

```sh
cargo build --locked -p cargo-chialisp-oxide -p chialisp-oxide-driver
export PATH="$PWD/target/debug:$PATH"
cargo chialisp-oxide check --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo chialisp-oxide build --manifest-path examples/cargo-puzzles/Cargo.toml --locked
```

`./scripts/cargo-chialisp-oxide` builds and invokes the same local command without the
`PATH` setup. The SDK and binaries are unpublished. Local installation is
possible with `cargo install --path crates/cargo-chialisp-oxide --locked` and
`cargo install --path crates/chialisp-oxide-driver --locked`; keep the pinned compiler
components installed. See the [quickstart](/chialisp-oxide/start/quickstart/).

Ordinary `cargo check` and `cargo test` still check and test the native Rust
library. `cargo chialisp-oxide check` additionally checks every marked export's reachable
CLVM subset and backend lowering. It removes its temporary artifacts rather
than publishing a bundle. `cargo chialisp-oxide build` publishes a successful bundle.

The compiler analyzes a normal Cargo library with `#[chialisp_oxide::puzzle]`
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
cargo chialisp-oxide check --manifest-path examples/cargo-puzzles/Cargo.toml --locked --features strict-cap
cargo chialisp-oxide build --manifest-path examples/cargo-puzzles/Cargo.toml --locked --no-optimize
```

`--no-optimize` skips the optional Rust pipeline: checked constants, forward
success facts, constants again, legacy tree guards, runtime frame elision, Rue
LIR and static pair codegen. Frame elision applies only to runtime lowering;
the legacy backward guard pass stays disabled there. Rust checks,
ABI validation, and bounded arithmetic remain active in both modes. Existing compiler wrappers are rejected
rather than silently composed with the extraction wrapper.

### Output bundles

The default output root is the selected package workspace's
`target/chialisp-oxide/artifacts/`. `--output-dir PATH` chooses another root. Each
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

The corpus itself is the ordinary `chialisp-oxide-puzzles` workspace package, declared
in `puzzles/Cargo.toml`. Its modules export annotated functions such as
`chialisp_oxide_puzzles::capped_payment::puzzle`:

```sh
cargo test --manifest-path puzzles/Cargo.toml --locked
cargo chialisp-oxide build --manifest-path puzzles/Cargo.toml --locked
cargo chialisp-oxide check --manifest-path puzzles/Cargo.toml --locked --no-optimize
```

The workspace output root is `target/chialisp-oxide/artifacts/`. Read `latest.json`,
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

root = Path('target/chialisp-oxide/artifacts')
pointer = json.loads((root / 'latest.json').read_text())
manifest_path = root / pointer['manifest']
manifest = json.loads(manifest_path.read_text())
entry = next(item for item in manifest['entries']
             if item['entry'] == 'chialisp_oxide_puzzles::capped_payment::puzzle')
subprocess.run(['target/debug/chialisp-oxide', 'run',
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

### Replay a retained program against many inputs

`run-batch` takes a JSON array of CLVM assembly strings in a file:

```json
["(500)", "(1001)", "()"]
```

```sh
target/debug/chialisp-oxide run-batch retained.clvm.hex \
  --solutions inputs.json --max-cost 1000000
```

The output is an array of the same typed execution records as `run`, in input
order. Each input gets a fresh allocator and its own cost budget. Exit status 0
means the batch completed; inspect **every** outcome, since policy rejections
and inconclusive resource failures remain separate records. Malformed input or
an invocation error exits nonzero and publishes no partial JSON.

Transport limits are 4,096 solutions, 8 MiB of input JSON, 4 MiB of program hex
and 16 MiB of output JSON. Count bounds apply during parsing. The per-input
cost budget defaults to 1,000,000 and may not exceed 100,000,000 in this bounded
batch interface. These are host transport limits, not guards in a puzzle.
The hashing evidence gate uses batches to replay the exact retained program
bytes without recompiling any source.

## Replay a linked native function

```sh
target/debug/chialisp-oxide native bounded-counter --solution '(999)'
target/debug/chialisp-oxide native bounded-counter --solution '(1000)'
```

The first command returns an accepted JSON outcome with `output_hex` equal to
`8203e8`, the serialized integer 1,000. The second returns a rejected policy
outcome. Both policy outcomes exit with status 0; invocation errors, unknown
slugs, and stale native-oracle inputs exit nonzero.

This helper applies the canonical codec and invokes the actual linked Cargo
policy. Explicit ignored-environment nullary exports parse the supplied CLVM
tree but do not impose an argument-list or typed-scalar check. Raw-tree exports
pass the whole environment as borrowed
tree data; their source determines shape checks. Default exports retain their
strict fixture ABI. The native helper does not execute the VM or
ledger.
Its compiled input snapshot is checked against `puzzles/authoring.json`; a deliberately
changed declared input has been tested to reject until the native oracle is
rebuilt. This prevents changed declared Cargo inputs from silently using an old
native policy, without claiming a complete transitive dependency fingerprint.

## Regenerate the registered corpus

```sh
cargo build --workspace --locked
./scripts/chialisp-oxide validate --output corpus/validation.json
```

Validation builds the Cargo puzzle package once, selects the declared exports,
compiles each IR program in both backend modes, and compiles the authored
Chialisp references independently. The native oracle links that same Cargo
library. Named and generated inputs compare native output/rejection with all
five CLVM builds (Chialisp, Rust off/on, and Rue source off/on); optimized coin puzzles from each language also run separately in the ledger simulator.
A divergence fails the run.

The report's `optimization_pipeline` distinguishes Rust's three optimization
stages from the Rue source compiler's LIR pass. The existing `optimize_lir`
boolean names each off/on build for compatibility; read it together with that
metadata. Rejection and exact returned bytes are compared; optimization may
change execution costs and rejection diagnostics.

`puzzles/authoring.json` declares package, library, export/source mapping, and
local Cargo inputs. Validation and source-derived proof reports record the
same input hashes. The documentation gate checks those fingerprints, exact
fixture/dependency sources, manifest cases, backend agreement, ledger checks,
and formal inputs. These explicit local records are not a complete fingerprint
of all transitive Cargo dependencies.

## Source-derived byte-policy proofs

After building the tools and refreshing the corpus, run the complete
Cargo-extraction, solver and native/VM evidence gate:

```sh
python3 scripts/check-byte-proofs.py
python3 scripts/check-byte-proofs.py --check
python3 scripts/check-byte-proofs.py --self-test
```

The first command executes the checks. `--check` validates recorded freshness
without running Rust, Z3 or the VM. `--self-test` exercises corruption controls.
The [byte-proof guide](/chialisp-oxide/verification/byte-proofs/) explains the
independent whole-environment specification, actual source controls, synthetic
ABI controls and narrower IR-model claim.

The generic verifier accepts an extracted program and independent specification:

```sh
target/debug/chialisp-oxide-verifier proofs/atom-not-equal/original.ir.json \
  --structural-boolean proofs/atom-not-equal/specification.json
target/debug/chialisp-oxide-verifier proofs/atom-not-equal/original.ir.json \
  --structural-boolean proofs/atom-not-equal/specification.json --emit-json
target/debug/chialisp-oxide-verifier proofs/atom-not-equal/original.ir.json \
  --structural-boolean-replay witness.json
```

`--emit-json` emits the five SMT queries without invoking the solver. Replay
requires a complete witness object from the report, runs both compilation
modes, and aborts on resource/infrastructure failures. A refuted model exits
with status 1; a solver or unsupported-model error also exits nonzero, so inspect
the structured result rather than treating any nonzero exit as a counterexample.

## Standalone compiler regressions

Direct source-file extraction remains a research and regression interface.
It is no longer the corpus authoring path. Use a standalone fixture rather
than passing a Cargo module that requires external dependency metadata:

```sh
./scripts/chialisp-oxide build crates/chialisp-oxide-driver/tests/fixtures/guarded_payment.rs --output target/research
cargo build -p chialisp-oxide-driver --locked
target/debug/chialisp-oxide-driver crates/chialisp-oxide-driver/tests/fixtures/guarded_payment.rs --output target/research.ir.json
```

The standalone driver selects the free function named `puzzle` by default and
accepts `--entry NAME`. That option is not a `chialisp-oxide build` flag; Cargo mode
instead discovers annotated exports. Keep compiler-private APIs isolated in
the driver when investigating diagnostics or extending the subset.
