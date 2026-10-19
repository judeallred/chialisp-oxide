# P2 delegated-conditions source controls — WIP prototype

This research harness is not ready for production use. It checks three deliberately
wrong Rust policies that still compile. It is not an example of errors Rust makes
impossible, a security proof, or a complete coin-spend test.

The generator starts with the exact annotated Cargo puzzle, applies the three
pinned substitutions, and builds a separate ordinary Cargo library. `baseline`
is explicitly a copied control. The native test calls that same four-export
package, not a separately copied oracle. Its complete input preflight and output
encoding are outside the policy-panic catch. The exact reviewed `native.rs` has
**one native test group and three policy observations**.

Preparation runs the pinned rustfmt on stdin before freezing eight generated
inputs and before the first Cargo command. The original recipes stay unchanged.
Only the strict-tail assertion has the reviewed multiline layout from the actual
formatter diagnostic; baseline, the other two mutants and `native.rs` remain
byte-identical. The reader checks this closed relation without running rustfmt or
claiming a general source-transformation proof. Cargo fmt --check is mandatory.

## Separate execution phases

Run these only with coordinated ownership of the chosen source and target. Never
run extraction while a host build can replace the driver or other tools.

```sh
python3 scripts/run-p2dc-source-controls.py prepare \
  --package /tmp/p2dc-controls-package \
  --receipt /tmp/p2dc-controls-prepare.json
python3 scripts/run-p2dc-source-controls.py native \
  --package /tmp/p2dc-controls-package \
  --prepared /tmp/p2dc-controls-prepare.json \
  --target /tmp/chialisp-controls-target \
  --output /tmp/p2dc-controls-native
```

The native phase generates and pins the ninth input, Cargo.lock, then runs normal
format/check/test/Clippy checks. Finish the full host workspace gates and its
**LAST all-target build afterward**. The external build coordinator must retain a
reviewed quiet-window receipt with protocol
`chialisp-oxide-p2dc-controls-quiet-window/v1`, status
`LAST_host_build_terminal_tools_quiet`, exact schema/version/root/package/compiler/
toolchain/source-window/tool maps, the native-receipt SHA-256, and an explicit
scope. This harness does not manufacture that receipt. It coordinates a quiet
input window; it is not authentication, a permission token or proof a build ran.

```sh
python3 scripts/run-p2dc-source-controls.py capture \
  --package /tmp/p2dc-controls-package \
  --native-receipt /tmp/p2dc-controls-native/native-receipt.json \
  --last-host-window /tmp/reviewed-last-host-window.json \
  --target /tmp/chialisp-controls-target \
  --output /tmp/p2dc-controls-capture
python3 scripts/check-p2dc-source-controls.py \
  /tmp/p2dc-controls-capture/report.json --self-test
```

Capture independently queries pinned rustc and driver versions and hashes the
current tools. Both actual four-export Cargo bundles use explicit complete
options and recomputed private/main source snapshots. Ordinary v5 Program
validation and recompilation are separate from twelve retained primary VM runs:
three baseline witnesses and their matching mutants in each mode. Exact outputs
come from the independent finite specification. Only typed `Raise` confirms the
extra assertion's rejection; resource and infrastructure failures are inconclusive.

Keep failed outputs/logs without success credit. Never retry into a used output
folder. Inspect owned child processes and late writes after a timeout.

## Recorded evidence boundary

The default checker executes no Cargo/compiler/VM/SDK/solver process. It checks
current23 authoring, compiler/native/reader source hashes, exact recipe/format
bytes, captured compiler-host/path/options relationships, retained IR/CLVM and
recompile receipts, one native test group/three observations, and twelve typed
VM records. Retained absolute paths describe the original run; the checker does
not read old external paths. Portable artifacts use confined relative paths.

The source window admits the exact reviewed workspace members and conventional
`src`, `examples`, `tests` and `benches` Rust source roots, plus the reviewed
`puzzles/lib.rs` and default `build.rs`. Declared custom target/build paths outside
those layouts are rejected before capture. This bounded admission rule is not a
general Cargo/Rust dependency-discovery or build-script sandbox claim.

Receipt checks do not authenticate execution or prove IR-to-CLVM preservation.
Installed compiler/dependency bytes and coordinated immutable inputs remain
assumptions. These controls add no SDK submissions, authorization/state/BLS proof,
generic ledger groups, or formal model results. They do not change the pending
23-puzzle append profile.

This public schema is a new capture contract. Earlier private Phase B layouts,
producer source windows and the shorthand `new_P2dc_native_control_tests=3` must
remain unchanged. Importing them requires a separately reviewed adapter that
preserves actual provenance and records one test group/three observations; it
must not relabel old execution as execution of this new producer.
