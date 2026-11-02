# Stateful SDK ledger experiment (WIP)

This standalone, offline host runner executes the original batch-two and Spuds
teaching policies through the pinned Testnet11 SDK simulator. It has no wallet,
network, publication or compiler-driver authority. These finite executions do
not establish production readiness, compiler preservation, or standard wallet
integration. Native/VM condition observations and consensus admission are
separate checks.

Run from the repository root with a private Cargo target:

```sh
CARGO_TARGET_DIR=target/stateful-ledger-private cargo test --manifest-path experiments/stateful-ledger/Cargo.toml --locked --offline
CARGO_TARGET_DIR=target/stateful-ledger-private cargo run --manifest-path experiments/stateful-ledger/Cargo.toml --locked --offline -- input.json output.json
```

The bounded, closed input schema is:

```json
{
  "reference_only": false,
  "programs": [
    {"name":"chialisp", "batch_two":"batch-reference.clvm.hex", "spuds":"spuds-reference.clvm.hex", "batch_one":"immutable-reference.clvm.hex"},
    {"name":"rust_on", "batch_two":"batch-on.clvm.hex", "spuds":"spuds-on.clvm.hex", "batch_one":"immutable-on.clvm.hex"},
    {"name":"rust_off", "batch_two":"batch-off.clvm.hex", "spuds":"spuds-off.clvm.hex", "batch_one":"immutable-off.clvm.hex"}
  ]
}
```

The mode set and order must match exactly. `reference_only: true` requires only
`chialisp` and labels the result `exploratory_reference_only`; it cannot produce
a complete three-mode report. The two Chialisp artifacts must equal freshly
compiled independent reference sources. Rust artifacts require the external
fresh Cargo/source checks; this runner does not claim their provenance from
module hashes alone. Batch-one inputs are retained immutable baseline artifacts.

Batch comparisons use identical four/eight-recipient queues with adjacent
recipients distinct. They mine complete four/eight-step baseline chains and
two/four-step batch-two chains, compare every exact output/successor/terminal,
and check typed amount, inflation and stale-coin failures. Malformed openings
are retained as inconclusive `GeneratorRuntimeError` umbrellas without policy
rejection credit. Inflation uses an explicitly quoted composition fixture.

Spuds uses only the named, disposable deterministic seeds in `fixture_keys.rs`.
It takes at actual coin ages one and four, checks successor timer resets, and
claims at age five. Every construction checks the complete six-condition VM
output independently of consensus submission. Submitted failures cover a
nonempty wrong-key BLS signature, changed payout with an old signature, an
underfunded top-up, age-zero take, early claim at four, reset early claim,
expired take at five, stale coin and claim with a take-protected top-up.
A same-bundle ephemeral take separately returns `EphemeralRelativeCondition`,
because consensus forbids its relative conditions; that is not mislabeled as
an ordinary coin-age failure. A maximum-round take is separately an inconclusive runtime umbrella without
policy rejection credit. Every rejected SDK submission checks exact typed
consensus code and full in-memory simulator equality.

The top-up is a quoted fixture with an exact take-announcement assertion and
fixed own change. It is **not standard wallet integration**. The host explicitly
funds this fixture, adds exactly 1,000 mojos net to a valid take, and verifies all
resulting coins, actual birth/spent heights and simulator height transitions.

Every accepted bundle is measured by the existing reviewed spend-cost module:
actual uncompressed and backreference generators, expanded-tree equality, BLS
verification, unrelated/identity signature controls, exact cost budget success,
and one-less budget failure. Whole-chain sums and signed batch-two-minus-baseline
deltas retain charged bytes, execution, condition, byte and total costs for both
encodings, including regressions. No improvement is assumed. These simulator
runs do not create live Testnet11 transactions or count as live evidence.

The complete reproducible gate is:

```sh
python3 -B experiments/stateful-ledger/ci.py --tools /absolute/path/to/target/debug --expected-host aarch64-apple-darwin --output target/stateful-capture
```

Use the actual pinned compiler host on other platforms. This runs the standalone
SDK tests, both fresh source-aware Cargo bundles for each policy, independent
Chialisp, all three primary-VM modes, actual-source controls, and two complete
SDK runs with byte-identical reports. It requires the compiler implementation
source at the supplied tools' build root to match the current declared compiler
closure, and records the executable fingerprints separately. Installed binaries,
registry sources, compiler/sysroot bytes and mutable build inputs remain assumptions.

`ci.py --promote target/stateful-capture` copies only inventoried reports, logs,
source snapshots and Cargo bundles to the canonical evidence directories; it
preserves captured origins as declarations and uses reviewed relative packet
references. It refuses existing destinations. `ci.py --check
experiments/stateful-ledger/evidence/receipt.json` validates the retained packet,
current declared source hashes, complete IR/blob/source inventories, exact typed
outcomes, selected modules and measured sums without granting execution credit.

Each policy also has `capture_success.py`, `capture_mutations.py` and
`check_boundary.py`. The unchanged earlier expansion-depth receipts are checked
against exact frozen historical source snapshots. They carry no current-source
freshness, current compiler-boundary or current success credit.

The four batch controls alter the second opening, amount, terminal and successor
rules. The five Spuds controls alter amount, age, next state, announcement and
signature binding. Each original/mutant pair is native-executed, compiled and
VM-executed in both Rust modes, then funded under its own selected curried
reveal for actual SDK submissions. The capture reader rejects altered codes,
accepted/rejected kinds, counts, modes, own reveals, selected artifacts and
source recipes. These are finite policy counterexamples that ordinary Rust
permits, not compiler security proofs.

Measured whole-chain results depend on encoding and mode. Optimized Rust
batch-two costs more than batch-one in both encodings; unoptimized Rust improves
uncompressed cost but regresses with backreferences. The Chialisp reference
improves both. Retained signed deltas show those regressions explicitly.
