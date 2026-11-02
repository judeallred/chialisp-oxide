# Explicit two-payout continuation source experiment

**WIP prototype. Not ready for use or real funds.** This ordinary Cargo library
is a new original teaching policy for issue #16. It is separate from the
published one-payout experiment. Nothing here changes its source, artifacts,
measurements, journal format or live workflow.

Current [successful capture](success-evidence/receipt.json) admits this real
Cargo export in both Rust modes and executes 59 primary-VM vectors per mode
beside the independent Chialisp reference. The module sizes are 25,691 bytes
with optimization, 36,072 without, and 1,770 for Chialisp. Native Rust tests,
compiler admission, template VM observations and SDK consensus are separate lanes.

The shared [ledger report](../stateful-ledger/evidence/report.json) executes
complete two/four-step batch chains and immutable four/eight-step batch-one
baselines for identical pair-distinct queues. Every accepted bundle has actual
uncompressed/backreference generator, BLS and exact-budget controls. Optimized
Rust batch-two costs more in both encodings; unoptimized Rust improves raw
generator cost but regresses with backreferences. No universal gain is claimed.

Four [actual-source controls](source-controls/receipt.json) exercise second
opening, amount, terminal and successor rules, each funded under its own reveal.
The older [boundary receipt](evidence/receipt.json) remains an unchanged
historical unsupported-compiler checkpoint, checked against its exact frozen
source snapshot. It grants no current-source or current compiler-boundary credit.
There is no generic continuation API, standard wallet integration or new live test.

## Contract

Each queue item pays exactly 1,000 mojos. A step pays two items and has no fee,
reserve, remainder, controller, refund or abort branch. The same fixed byte
commitments as batch one authenticate queue order:

- End: SHA-256 of ASCII `chialisp-oxide/continuation/v1/end:1000`.
- Item: SHA-256 of ASCII `chialisp-oxide/continuation/v1/item:1000`, followed by
  the 32-byte recipient puzzle hash and 32-byte next digest.

`Terms = (PuzzleHash, Message32, u8)` is a proper three-element list containing
the selected module hash, current queue head and remaining item count. Only
counts 2, 4, 6 and 8 are admitted. Terms are curried as **one** argument.
`BatchOpening = (Opening, Opening)` contains two proper two-element openings,
each `(recipient puzzle hash, next digest)`. The exact uncurried environment is
`(terms ((first_recipient middle_digest) (second_recipient next_digest)))`.
The curried solution supplies exactly one batch argument:
`(((first_recipient middle_digest) (second_recipient next_digest)))`.

The first opening must match the current head; its next digest must not be the
terminal sentinel. The second opening must match that intermediate digest.
Its next digest must be the sentinel exactly when the remaining count is two.
A nonterminal step derives a successor using the same module, that next digest,
and count minus two. `ASSERT_MY_AMOUNT` requires exactly 1,000 times the current
count; each payout is exactly 1,000 and the successor receives the full remaining
budget. A terminal step emits no continuation coin.

Output has type
`Cons<AssertMyAmount, Cons<CreateCoin, Cons<CreateCoin, Option<CreateCoin>>>>`:
three conditions at termination and four otherwise. This uses the existing
output-only optional encoding in the emitted version-6 program. Executed strict-ABI
vectors reject malformed, padded, improper and oversized input trees in both Rust modes.

## Deliberate difference from batch one

The two recipients **within each batch must differ**. Two identical recipient
hashes receiving the same amount under one spent parent would create duplicate
coin IDs. The policy rejects that pair explicitly, and the host recipe checks
every pair before returning its complete queue plan. A recipient may appear
again in another batch because that payout has a different parent.

Batch one can pay the same recipient in consecutive separate spends. Batch two
therefore supports a narrower queue domain; it is not a behavior-preserving
replacement. Odd queue lengths reject. The four-identical-recipient
live trial is a batch-one workflow and is not a batch-two fixture.

As in batch one, the template cannot intrinsically verify its module-hash field
against its actual selected artifact. A native test retains an accepted wrong
module-field counterexample, separately from the host equality check.
`funding_hash` and Python `plan` require a caller-established selected module
hash; neither authenticates a build or admits a compiled packet. A future funding
adapter must admit the complete actual selected artifact and reconstruct the
whole pair-distinct queue before funding.

Permissionless callers can advance valid committed work. No liveness, block
inclusion, finality, cross-block atomicity, SHA security or compiler-preservation
proof follows from these tests.

## Source provenance and license status

The policy and reference are original project work, not a canonical protocol
port. The design derives from the original batch-one checkpoint
[`0f67e89df21b7878618a16aebfb0d6491c9acefd`](https://github.com/judeallred/chialisp-oxide/tree/0f67e89df21b7878618a16aebfb0d6491c9acefd/experiments/authenticated-continuations).
The reviewed batch-one source identities are:

- `src/lib.rs`: SHA-256 `630892dfd761cfd7d4e8986e08fac5e4424201c04f85d79f248d94b1e09092a3`.
- `reference.clsp`: SHA-256 `b94392de705c34404499f2fc2d0395195a545f1b8bf7b84f030c12d76289fbc1`.
- `oracle.py`: SHA-256 `021215a31e50789f45899a206e7c80f951d8b8d31bb41a709c31c52cef738307`.

The reference was adapted independently from this documented two-opening
contract; it does not execute the Rust implementation or import its SDK helpers.
The Python oracle independently constructs and hashes the standard curry tree.
Its all-`aa` module hash is explicitly a fixture, not a compiled module identity.
No upstream puzzle source was copied. The repository has no declared project
license; this checkpoint does not invent a redistribution license or claim a
licensed upstream port. The Cargo package remains unpublished.

The new lock was seeded from batch one's committed Cargo lock and resolved
using `cargo update --workspace --offline`; external package/version/source pins
are preserved. Its root package has a new name.

## Local native checks

From the repository root, using the repository's pinned dated nightly:

```sh
CARGO_TARGET_DIR=target/continuation-batches-native-a01 cargo test --manifest-path experiments/continuation-batches/Cargo.toml --locked
CARGO_TARGET_DIR=target/continuation-batches-native-a01 cargo clippy --manifest-path experiments/continuation-batches/Cargo.toml --all-targets --locked -- -D warnings
cargo fmt --manifest-path experiments/continuation-batches/Cargo.toml -- --check
python3 -B -m unittest discover -s experiments/continuation-batches -p 'test_*.py' -v
```

Eight native tests include full 2/4/6/8-item queue replays, exact payout and
successor fields, terminal transitions, count/budget tables, changed openings,
duplicate recipients and the selected-module boundary. Fourteen Python recipe/vector/reader
tests check the independent fixed fixtures, complete plans, adversarial openings
and raw-environment construction. These are host observations. Each of the three primary-VM modes executes
59 cases: 11 accepted exact outputs and 48 program rejections. Five extra-tail
cases deliberately retain Rust-off `InvalidOpArg` versus reference/on `Raise`;
no resource or infrastructure failure receives program-rejection credit.

## Reproduce and read current evidence

Build the pinned host tools first, then keep their compiler implementation and
binaries unchanged during capture. The shared gate executes fresh complete
Cargo bundles, native checks, all three primary-VM modes, source controls and
two complete SDK runs whose reports must be byte-identical:

```sh
python3 -B experiments/stateful-ledger/ci.py --tools target/debug --expected-host aarch64-apple-darwin --output target/stateful-capture-new
python3 -B experiments/stateful-ledger/ci.py --check experiments/stateful-ledger/evidence/receipt.json
python3 -B experiments/continuation-batches/check_boundary.py
```

Use the actual pinned compiler host on other platforms. `capture_success.py` and
`capture_mutations.py` provide separate capture/read-only `--check` entrypoints.
Readers check complete current source inventories, actual compiler-read source
attachments, exact native/VM/SDK witnesses and corruption controls. They do not
authenticate executions or prove compiler preservation.

## Measured complete chains and source controls

Both queues start with exactly 1,000 mojos per recipient, pay every recipient
exactly once and end without a successor. Normal accepted chains pay no fee.
The fixture has no reserve. Parent-linked coin names and all resulting children
are checked independently; they cannot match across different selected modules.

For four payouts, batch-two minus batch-one total generator cost is:

- Chialisp: -33,925,160 uncompressed; -32,029,160 with backreferences.
- Rust on: +227,292,802 uncompressed; +63,948,802 with backreferences.
- Rust off: -1,216,285,258 uncompressed; +257,194,742 with backreferences.

For eight payouts, the respective deltas are -67,850,312 / -64,058,312 for
Chialisp, +454,580,577 / +127,892,577 for Rust on, and -2,432,567,743 /
+514,392,257 for Rust off. The report retains every step and whole-chain charged
byte, execution, condition, byte and total cost. For example, four-payout Rust
on adds 19,250 uncompressed charged bytes or 5,638 backreference bytes despite
using fewer spends. These pinned simulator measurements are not fee estimates
or a claim about current live block admission.

The four source controls have eight native observations, sixteen primary-VM
observations and sixteen SDK submissions across original/mutant and both Rust
modes: ten accept, two reject with exact `AssertMyAmountFailed`, and four return
explicitly inconclusive runtime umbrellas without policy-rejection credit.
Original opening/terminal failures separately have executed typed primary-VM
`Raise` observations. Every rejected SDK submission preserves the full simulator.
Accepted mutants demonstrate finite policy errors ordinary Rust permits; they
are not compiler security proofs.

The unchanged historical `capture_boundary.py` reproduces only its old
unsupported-compiler contract with the corresponding old tools/source snapshot.
It is not the command for reproducing current successful emission. The current
reviewed compiler change preserves the existing budgets and earlier successful
artifacts; no policy guard was removed to emit the real source.
