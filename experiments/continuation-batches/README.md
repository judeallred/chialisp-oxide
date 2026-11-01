# Explicit two-payout continuation source experiment

**WIP prototype. Not ready for use or real funds.** This ordinary Cargo library
is a new original teaching policy for issue #16. It is separate from the
published one-payout experiment. Nothing here changes its source, artifacts,
measurements, journal format or live workflow.

This checkpoint contains native Rust and independent Python recipe/vector tests,
an independently compiled Chialisp reference, and 59 executed primary-VM
observations of that reference alone. **The current Rust compiler cannot emit
this combined policy under its unchanged expansion-depth budget.** The public
`cargo chialisp-oxide check` and both optimized/unoptimized builds fail with
`CLVM expansion nesting limit`; no Rust CLVM artifact or complete source-aware
bundle is available. Native Rust success is not CLVM admission.

The retained [boundary receipt](evidence/receipt.json), actual diagnostic IR,
command logs and single-lane reference observations record that distinction.
Source-mutation controls, Rust VM execution, complete SDK spends and cost
comparisons remain open. There is no claim that batching improves bytes, cost
or fees, and no generic continuation API or compiler splitting feature.

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
output-only optional encoding in source; actual compiler emission is blocked by
the current expansion-depth limit. Unsupported inputs still need executed strict-ABI VM controls.

## Deliberate difference from batch one

The two recipients **within each batch must differ**. Two identical recipient
hashes receiving the same amount under one spent parent would create duplicate
coin IDs. The policy rejects that pair explicitly, and the host recipe checks
every pair before returning its complete queue plan. A recipient may appear
again in another batch because that payout has a different parent.

Batch one can pay the same recipient in consecutive separate spends. Batch two
therefore supports a narrower queue domain; it is not a behavior-preserving
replacement. Odd queue lengths reject. The planned four-identical-recipient
live trial remains a batch-one workflow and is not a batch-two fixture.

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
duplicate recipients and the selected-module boundary. Ten Python recipe/vector
tests check the independent fixed fixtures, complete plans, adversarial openings
and raw-environment construction. These are host observations. The separate
reference lane executes 59 actual primary-VM cases: 11 accepted outputs (including
the wrong-module template counterexample) and 48 typed program failures. It is
not Rust VM execution or complete SDK transaction evidence.

## Reproduce the current compiler boundary

Build the repository host tools first and keep them unchanged during capture.
Use a fresh output directory; the script never rebuilds the shared driver:

```sh
python3 -B experiments/continuation-batches/capture_boundary.py --tools target/debug --output target/continuation-batches-boundary-new
```

This reruns native checks, the public compiler check and both attempted builds.
The expected result is the same expansion-depth diagnostic, with actual Rust IR
retained solely for diagnosis. It then independently compiles the Chialisp
module and executes its 59 vectors. `boundary_observed` is an observed compiler
limitation, **not** a successful Rust port. Source/tool hash consistency and
retained command declarations do not authenticate executions or prove compiler
preservation. The old one-payout evidence and compiler budgets remain unchanged.

The original readable source remains intact after failed local attempts to add
ordinary policy frames and spell the standard curry hash in named local steps.
None admitted the combined program under the existing limit; no compiler
fallback eligibility, bounds or policy checks were changed. Local diagnostic
attempts remain under `target/continuation-batches-capture-a01` through `a04`.
The retained checkpoint re-executes the original source in both modes.

The replay data includes proposed Rust-off ABI-tail expectations for future
comparisons. They are expectations only; no Rust mode was executed here. The
report credits only the `chialisp` program. Its module is 1,770 serialized bytes;
accepted terminal vectors cost 32,420 and nonterminal vectors 66,468 primary-VM
units. These are template VM measurements, not generator, transaction, block or
fee measurements, and establish no improvement over batch one.

## Required comparison before making batching claims

First resolve the compiler admission boundary with reviewed semantics and
regressions, retaining the existing limits. Then extract this real Cargo export
in both modes, retain complete source-aware bundles, and execute the independent
reference alongside Rust on malformed/padded/improper-tree/oversized and policy
adversarial environments. Do not remove guards to fit the export. Add actual
source mutants for opening, amount, terminal and successor enforcement.

Then replay the immutable batch-one module bytes freshly beside batch two using
the same pinned SDK, consensus protocol and previous transaction height. Use the
same ordered pair-distinct four- and eight-recipient queues, exact initial
budget, zero fees and no reserve. Both Rust modes and independent Chialisp remain
separate lanes. Compare the payout sequence and amounts, remaining budget and
completion; coin IDs cannot match across different modules and parent chains.

Retain per-step and whole-chain generator charged bytes, execution/condition/
byte/total costs for both uncompressed and backreference encodings, BLS checks,
number of submissions and fees. Check full expected children after each accepted
SDK spend and exact typed outcomes/full simulator equality for appropriate
negative submissions. Report regressions as well as gains. Two hashes and two
payouts per spend may enlarge an individual module or increase step cost; fewer
spends alone is not a measured total-cost improvement.
