# Actual boomerang source controls — WIP prototype

This harness is not ready for real funds. It demonstrates policy errors that
ordinary Rust and the CLVM subset can accept. It does not add an error to the
“Rust makes impossible” category or establish a security proof.

The prepare command derives a separate ordinary Cargo library from the exact
current `puzzles/boomerang/puzzle.rs`. The `original` control remains byte-identical.
`mutations.json` gives one checked, unique substitution for each other export:

- `equivalent` reverses the payout predicate and swaps its arms, preserving policy.
- `drop_claim_signature` constructs but discards the claim signature condition.
- `late_claim` constructs but discards the before-deadline condition, returning an
  after-epoch condition instead.
- `early_refund` replaces the refund deadline with the epoch.

Constructing a condition is not enforcement: consensus checks the returned list.
The deliberately discarded values retain ordinary strict Rust evaluation. No
signature is added to the permissionless refund to simplify this experiment.

`native.rs` calls the actual five-export generated library. Nine finite output
witnesses use an independent bounded serializer, not a copied policy or compiler
output as oracle. The normal Cargo test, check and Clippy commands must pass.
The generated sources must already pass `cargo fmt --check`; preparation never
silently formats or rewrites the original source or a mutation recipe.

`ledger.rs` is built as an example during the native Cargo phase. It creates
fresh SDK fixtures and funds **each mutant's own compiled curried reveal**.
Substituting a mutant reveal on the original coin would only test puzzle-hash
mismatch. The Rust/off and Rust/on captures each execute nine primary-VM and
nine separate SDK attempts:

- Original and equivalent: unsigned claim at 99 rejects with
  `BadAggregateSignature`; a genuinely signed claim at 101 rejects with
  `AssertBeforeSecondsAbsoluteFailed`; unsigned refund at 99 rejects with
  `AssertSecondsAbsoluteFailed`.
- Each corresponding mutant admits that forbidden spend. The accepted input
  must be spent and have exactly one child, with its exact parent, full 1,000-mojo
  amount and committed recipient/refund destination.

After every exact typed rejection the runner compares the complete in-memory SDK
simulator with its clone. Retained receipts omit unstable simulator Debug text.
All submitted signatures are retained; the signed late-claim negative cannot
silently become an unsigned BLS failure. The helper independently reconstructs
the standard curry bytes and checks the actual reveal before funding.

## Coordinated phases

Run only after all implementation sources, readers and formatting are frozen.
Use fresh private directories; never reuse a failed output directory. Preparation
itself executes no compiler or formatter.

```sh
python3 scripts/run-boomerang-source-controls.py prepare \
  --package /tmp/boomerang-controls-package \
  --receipt /tmp/boomerang-controls-prepare.json
python3 scripts/run-boomerang-source-controls.py native \
  --package /tmp/boomerang-controls-package \
  --prepared /tmp/boomerang-controls-prepare.json \
  --target /tmp/chialisp-controls-target \
  --output /tmp/boomerang-controls-native
```

The native phase generates the package's locked dependency graph, runs the
ordinary gates and builds the SDK example. Finish all host workspace gates and
the LAST host build afterward, with coordinated ownership of the target. The
external coordinator retains a quiet-window receipt with protocol
`chialisp-oxide-boomerang-controls-quiet-window/v1`, status
`LAST_host_build_terminal_tools_quiet`, schema version 1, exact `source_root`,
`package_root`, full `compiler` identity, `toolchain`, current `source_window`,
tool path/hash map, native receipt SHA-256 and an explicit `scope` string.
The capture script checks this receipt; it never creates one.

```sh
python3 scripts/run-boomerang-source-controls.py capture \
  --package /tmp/boomerang-controls-package \
  --native-receipt /tmp/boomerang-controls-native/native-receipt.json \
  --last-host-window /tmp/boomerang-controls-quiet.json \
  --target /tmp/chialisp-controls-target \
  --output /tmp/boomerang-controls-capture
```

Capture independently queries rustc and driver versions, rehashes all tools and
loads complete fresh five-export Cargo bundles in both modes. It retains the
source recipes/package, IR, compiled CLVM, separately recompiled artifacts,
curried instances, exact VM inputs/results, SDK request/result, native logs and
command logs. Current source and binary identities remain fixed during capture.
The independent boomerang reader checks the retained source-control packet
without running Cargo, a compiler or the SDK.

Keep the counts distinct: **one native test group, nine native output observations,
ten export/mode artifacts, eighteen primary-VM observations and eighteen SDK
transaction attempts**. These are Rust source mutants only; no Rue or Chialisp
mutant runs, generic ledger groups, formal proofs or whole-policy exhaustiveness
are implied. The primary VM is clvmr 0.21.0; SDK records carry their separately
pinned clvmr 0.16.2, consensus and TESTNET11 identities.

Source/receipt freshness does not authenticate execution, the installed compiler
or dependencies, verify BLS independently, prove compiler preservation or prove
the absence of security breaches. Host resource failures are inconclusive and
abort capture without rejection credit. The public demonstration key is known;
never use it for custody.
