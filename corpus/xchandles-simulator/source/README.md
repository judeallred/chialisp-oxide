# XCHandles canonical host pilot

**WIP prototype, not ready for real funds.** This isolated unpublished Cargo
program uses the official SDK to compose existing canonical CLVM modules in a
local Testnet11 simulator. It is not an on-chain Rust frontend port, a mainnet
transaction replay, a general continuation type, or an authenticated commitment
to the remaining premine allocations. No network submission or wallet is used.

The current fixed recipe executes three separate registry spends with one, two
and three registrations respectively (six local handles total), using a
local fixed price, an issued payment CAT, actual precommit coins, canonical
neighbor slots and one real NFT as both owner and resolved singleton. The local
price singleton has the deliberately permissive inner program `1`; it is a
fixture, not the published price scheduler. A separately signed standard coin
satisfies registration security conditions; it is not a fabricated quoted-output
substitute for ledger submission. Simulator fixture funding is public test data.

The register module must be exactly 1,478 bytes with tree hash
`150ae295efe60fa049b87cff5c05c0031fa768959ba466b425ef5a58b4a95ee8`.
`fixtures/register.rue.hex` is the corresponding published artifact retained from
Yakuhito/slot-machine commit `6edc94856306d0f77e065c6c99aacf1055e51e2f`.
This later source snapshot does not authenticate the original deployment source.
The authoritative slot-machine license is Prosperity Public License 3.0.0,
retained as `fixtures/SLOT-MACHINE-LICENSE.md`; Cargo metadata is not an alternate
MIT license grant.

The first checkpoint used the official `launch_xchandles_registry` helper. That
helper obtains a random security key from `getrandom`, so Simulator seed 1337
alone does not make its launch IDs deterministic. The current small host adapter
in `src/launch.rs` instead composes public `create_security_coin_with_pk`,
`eve_singleton_inner_puzzle`, `Launcher::spend` and `spend_security_coin` APIs with
a named public fixture key. It neither calls private SDK helpers nor patches the
dependency or its RNG.

The host API recipe is adapted from the Apache-2.0 `chia-sdk-driver 0.36.0`
`primitives/action_layer/launch_drivers.rs` test and uses that exact crates.io
release, checksum
`33d22cc417877a290eff196fe851cfc1175a5432166f8d9bedeb9f55e010bc76`.
Its VCS metadata declares commit `e9a08f6e54ea5132f7da06327836a3b386a944b1`
with `dirty: true`; exact crate bytes are pinned, not a clean Git correspondence.
The only additional dependency absent from the reviewed root lock is its
nonoptional `bigdecimal 0.4.11`, checksum
`8fa3f3d8cbf4dffcfe4991de61d012bef509a409ecbe9dd41049bfe32b4d4653`.
All reused dependency tuples retain their reviewed versions and checksums.

Native builds use `nightly-2026-07-14`, an explicit matching RUSTC, two jobs and an
isolated target directory outside this worktree. Clear all six
`RUST_BIGDECIMAL_*` build settings to use the reviewed defaults. Monetary policy
uses integer mojos; decimal formatting supplies no consensus semantics.

Reports and bounded build/run logs belong under the main checkout's
`target/action15-*` paths. Component CLVM execution uses clvmr 0.16.2 and is
counted separately from actual SDK transaction submissions (SDK test 0.36.0,
consensus 0.36.1). The complete plain-generator cost lane uses consensus 0.36.1
`get_conditions_from_spendbundle` with actual simulator height and Testnet11
constants. Its flags are 720902, including `DONT_VALIDATE_SIGNATURE`; actual SDK
signature and ledger admission are separate submissions. The cost lane records
execution, conditions and charged bytes (complete plain generator minus the two
quote bytes), under a 5,500,000,000 ceiling. It is not a backreference-compressed
or full-block cost, nor a network fee quote. Components use a separate
500,000,000 execution ceiling and 64 MiB allocator. Resource or infrastructure
failures are inconclusive.

The mainnet observations show a constant registry state with changing slot
coins. This pilot must not invent a changing authenticated remaining-work cursor.
Current launch source batches 25 actions; the observed historical batches had 30.
Neither implies autonomous completion, liveness, fairness or a per-step fee quote.

## Executed isolated checkpoint

Before descriptor and duplicate-key hardening on 2026-10-26, format checks, two
native tests and all-target Clippy with warnings as errors passed. Those initial
checkpoint reports were byte-identical: 313,274 bytes,
SHA-256 `fb7f7541272ed8be74dab68d3062c7e8e95bf5307d1642fe7861354b0c9744ef`.
Reports and capped command receipts are retained outside source under
`target/action15-pilot-logs-a01` in the main checkout. Earlier failed attempts are
preserved, including the initial missing trait import, a fixture containing
invalid hyphenated handles, the unimplemented high-level solution codec and a
report-order mismatch. None supplies rejection or completion credit.

Each full run performs 20 actual SDK submissions: 11 accepted transactions, three
actual nonempty invalid aggregate signatures rejected as `BadAggregateSignature`,
three consumed-bundle replays rejected as `DoubleSpend`, and three wrong-neighbor
bundles rejected under the SDK's `GeneratorRuntimeError` umbrella. Each rejected
submission asserts complete simulator equality in memory. Wrong neighbors also
execute independently with exact component `Raise`; the umbrella is not an exact
semantic consensus code. There are 48 separate component executions per full
run, including those three failure witnesses, plus 11 complete-generator cost
passes. Repeating a run adds no distinct scenario coverage.

For the one-registration bundle, the measured plain generator is 14,677 bytes
(14,675 charged), with execution cost 5,598,763, condition cost 13,200,000 and
total cost 194,898,763. Two registrations measure 19,940 bytes (19,938 charged),
execution 7,801,709, conditions 20,400,000 and total 267,457,709. Three measure
25,199 bytes (25,197 charged), execution 10,014,131, conditions 27,600,000 and
total 339,978,131. All charge 12,000 per byte and consume one mojo of actual
fixture XCH fee. Names and neighbors differ between batches; this finite example
is not a universal batch-size bound or an attribution of historical premine cost.

Every step checks the exact complete registry child set, including ephemeral
slots, and the surviving sorted neighbor list. Slot records independently
reconstruct their creator singleton coin from lineage and retain that identity
separately from the current registry input. Registry state remains constant while
slot values, counters, NFT approvals and current coin identities advance.

## Pause and resume the public fixture

The executable accepts `--stop-after 1` or `--stop-after 2`. Save its stdout as a
public checkpoint, then pass that file to `--resume`. Resume reexecutes the fixed
recipe from the SDK seed and compares the entire retained prefix (source/lock
fingerprint, transactions, signatures, costs, outcomes, coins and slot proofs)
before proceeding. It imports no unchecked events, external signatures or keys.
It is a deterministic recipe replay, not a general wallet snapshot or state-store
implementation. The binary also refuses changed source/lock inputs until rebuilt.

A separate process paused after the first step (seven SDK submissions and 20
component runs). A resumed process produced the exact same final report as an
uninterrupted full run. Five actual process controls refused a modified current
coin, an unknown assertion field, a changed recipe fingerprint, oversized input
and malformed JSON. These are checkpoint/infrastructure refusals, not policy
rejection evidence. The coin/unknown-field controls reexecute the validated
seven-submission prefix before the mismatch; resource/header refusals occur
before replay.

Remaining work includes independent retained-report checking, broader adversarial
component substitutions, the full historical bundle costs and a published case
study. This standalone host pilot has no Rust/Chialisp/Rue paired corpus port,
Sage or live wallet evidence, authenticated remaining-work commitment, production
readiness or issue-completion claim.

## Hardened checkpoint reader

The subsequent `target/action15-pilot-logs-a02` checkpoint opens the actual Unix
file descriptor with `O_NONBLOCK | O_NOFOLLOW`, then checks the descriptor is a
regular file of at most 1 MiB before bounded reading. It does not stat a pathname
and then perform a potentially blocking open. The JSON reader rejects duplicate
decoded keys at every object (including escaped aliases), with depth 96 and
65,536 structural node/key charges. These are infrastructure bounds, not puzzle
policy guards. `libc 0.2.190` and `serde 1.0.229` are exact direct dependencies
already present in the reviewed root lock; no new external tuple was added.

Format, four native tests and all-target Clippy passed. The hardened full repeat
and separate resumed process each produced 313,274 bytes with SHA-256
`d2534c7c2431ebd9ea0d11d01eb0c9d421f2623f84b3f3db56738d4a8a92ce71`.
Ledger wires, coin identities, outcomes and costs match the earlier checkpoint;
only the recipe fingerprint changed. All original checkpoints and failed attempts
remain preserved.

Eleven actual refusal processes passed: the original five cases, duplicate root,
nested and escaped nested keys, a FIFO, a symlink to a valid checkpoint, and the
old-source checkpoint. FIFO and symlink refusal completed in about 4 ms under a
two-second deadline. The nine parser/descriptor/header refusals entered no SDK
work. The two whole-prefix mismatch cases reexecuted their seven-submission
prefix before refusal. Parser refusals are counted separately from SDK attempts.
