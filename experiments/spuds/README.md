# Spuds: finite hot-potato source experiment

**WIP prototype. Not ready for use or real funds.** This normal unpublished
Cargo library is an original teaching policy for issue #10. It has no singleton,
CAT, standard wallet integration or live-network lane. The current
[successful capture](success-evidence/receipt.json) contains fresh complete Cargo
bundles in both Rust modes and 51 executed primary-VM vectors per mode beside
independent Chialisp. Module sizes are 22,909 bytes for Rust on, 19,546 for Rust
off and 1,712 for Chialisp. The shared [SDK report](../stateful-ledger/evidence/report.json)
executes actual signed takes and a claim with exact ledger effects and ages.
Native outputs, VM conditions and consensus admission remain distinct evidence.

## Policy and exact ABI

One self-recreating coin is the pot. Its single curried parameter is the proper
four-element `Terms = (PuzzleHash, PublicKey, PuzzleHash, u8)` list: selected
module hash, current holder's BLS key, current holder's payout hash and finite
round. Round 1 holds 1,000 mojos. A take adds **exactly** 1,000, up to round 8 and
8,000 mojos. This intentionally chooses an exact increment rather than arbitrary
bids. Round 8 has only the eventual claim path.

The annotated export is ordinary Rust:
`step(terms, take: bool, next_key: PublicKey, next_payout: PuzzleHash)`.
The exact uncurried environment is `(terms take next_key next_payout)` and the
curried solution is `(take next_key next_payout)`. The strict typed ABI must
check canonical scalars and exact shapes. Claims still decode the two next-holder
fields but ignore their values; the claim always uses the committed current
holder key and payout. Keys are 48-byte atoms and hashes are 32-byte atoms.
Those widths establish neither valid BLS keys nor ownership or recoverability
of a destination.

Each path returns the same six-condition tuple:

1. `AGG_SIG_ME` for that path's key and intent message.
2. `ASSERT_MY_AMOUNT` for exactly the round's pot amount.
3. `ASSERT_HEIGHT_RELATIVE(1)` as a minimum coin age.
4. Take: `ASSERT_BEFORE_HEIGHT_RELATIVE(5)`; claim: `ASSERT_HEIGHT_RELATIVE(5)`.
5. One exact `CREATE_COIN`: the larger successor on take, the full current pot
   to the current holder's payout hash on claim.
6. `CREATE_COIN_ANNOUNCEMENT` using that path's intent message.

The before/after height constructors have the same supported Rust layout, so
this policy needs no general enum, optional input or variable output list.
That spelling does not establish compiler admission; the actual combined
program must fit the existing frontend/backend bounds.

## Timer reset and state commitment

The timer is tied to the actual pot coin's consensus creation height, never a
caller-supplied clock or deadline. Each successor has its own creation height,
which resets the relative window. Under the pinned height protocol, take is
eligible once the previous transaction block height reaches creation plus one
and strictly before creation plus five; claim is eligible at or after creation
plus five. These are block-age conditions, not seconds or a wall-clock promise.
The executed SDK lane rejects ordinary age-zero takes with
`AssertHeightRelativeFailed`. Ephemeral same-bundle spends instead fail with
`EphemeralRelativeCondition`: consensus forbids their relative conditions. That
separate rule is not mislabeled as an ordinary age failure.

The successor commits the same module, new holder key, new payout hash and round
plus one. `instance_hash` hashes the exact proper Terms list using `atom_hash`
and `pair_hash`, then the standard one-parameter curry. The finite round has a
literal canonical count-atom table; pot amounts also use an explicit finite
table. No dynamic integer byte encoding or unchecked arithmetic is assumed.

The template cannot intrinsically identify its own module. A creator choosing
a wrong module field can produce a wrong successor. `funding_hash` only compares
a caller-established selected hash; it does not authenticate source or artifacts.
A real funding adapter must admit the complete selected compiled packet, derive
its actual module hash and exact initial instance, and validate its initial
holder and budget before funding.

## Signatures and the external top-up

For a take, let `next_puzzle_hash` be the fully derived successor hash. The
message is SHA-256 of ASCII `chialisp-oxide/spuds/take:v1` followed by those exact
32 hash bytes. The new holder signs that message with `AGG_SIG_ME`, which also
binds the actual current coin and network. Binding the successor prevents a
copied signature from changing the new holder key, payout or round. A fixed
take message alone would leave those untrusted fields unbound.

For claim, the message is SHA-256 of ASCII `chialisp-oxide/spuds/claim:v1`.
The curried holder signs it and the curried payout is the only destination.
The claim announcement differs from the take commitment.

A take creates more value than the pot input, so an additional wallet input
must supply the exact 1,000-mojo deficit plus any independently chosen fee.
The wallet's signed delegated conditions must assert the coin announcement
SHA-256 of `old_pot_coin_id || take_message`, in addition to its exact change
and explicit fee policy. This binds the contribution to the genuine pot's
specific take. Merely asserting a concurrent spend of the pot is insufficient:
an old holder's claim could otherwise be aggregated with the contribution,
burning it as a fee instead of creating the intended successor.

The SDK experiment supplies a deliberately quoted top-up fixture with that exact
announcement assertion and fixed own change. It is not standard wallet integration.
Native generation of signature,
age or announcement conditions does not verify them. A wallet signature alone
is not a complete transaction or a consensus test.

## Limits and adversarial evidence

The native tests compare independent Python hash fixtures, walk all seven
possible takes, inspect every round's claim, check amount conservation and the
external deficit, bind messages to all successor fields, reject invalid rounds
and taking at the cap, and retain the accepted wrong-module-field counterexample.
The Python oracle separately constructs the curry hash and condition wires.
Fixture module `aa...aa` is not a compiled module. Public teaching keys are copied
from `puzzles/fixture_keys.rs`; no user's wallet key or secret appears here.

The current capture executes Rust on/off and independent Chialisp on the full
51-vector malformed/padded/improper/oversized and policy suite. Each mode has
19 accepted condition lists and 32 program failures. Two extra-tail Rust-off
cases have typed `InvalidOpArg`; the corresponding reference/on cases raise.

The SDK chain takes at actual ages one and four, resets each successor's timer,
and claims at age five. Every complete six-condition output and resulting child
coin is checked. Per mode, three accepted bundles accompany ten exact typed
rejections for timing, signatures, transplant, top-up, stale coin and protected
announcement cases. A separate cap take is an inconclusive runtime umbrella
without rejection credit. Full simulator equality follows every rejected
submission. Wrong-key and old-message signature tests submit actual bundles;
signer-preflight refusal is not counted as consensus rejection.

Five [actual-source controls](source-controls/receipt.json) alter amount, age,
next state, announcement and signature binding. Their original/mutant programs
have ten native, twenty primary-VM and twenty SDK observations across both Rust
modes: twelve SDK accepts and eight exact typed rejections with full state equality.
Each instance is funded under its own selected compiled reveal. These are finite
policy counterexamples, separate from the base chain, with named disposable
simulator keys; no user's secret key is used.

No finite tests establish compiler preservation, cryptographic security,
production readiness or universal absence of attacks.

Anyone with a new valid key and contribution may race a take. There is no
current-holder veto, first-arrival fairness or protection from independent
higher-fee competing takes. Copying the exact signed take cannot redirect its
committed successor. This is a coin chain, not a globally unique singleton game;
its identity and history require following the original funded coin. Lost keys,
bad payout hashes and lack of a willing spender can strand funds. No liveness,
finality, refunds or cross-block atomicity are guaranteed.

## Source and verification status

The policy, independent reference and oracle are original project work. No
canonical protocol is ported or claimed. Named constructors, curry hashing and
finite-table authoring follow the existing SDK and original continuation
examples; boomerang and precommit demonstrate the separate condition layouts.
The repository has no declared project license; this experiment does not invent
one. It remains unpublished and uses path dependencies and a reviewed lock.

Native Rust, compiler subset admission, emitted CLVM, VM observations and SDK
spends are separate milestones. Start with:

```sh
cargo fmt --manifest-path experiments/spuds/Cargo.toml -- --check
CARGO_TARGET_DIR=target/spuds-native-a01 cargo test --manifest-path experiments/spuds/Cargo.toml --locked
CARGO_TARGET_DIR=target/spuds-native-a01 cargo clippy --manifest-path experiments/spuds/Cargo.toml --all-targets --locked -- -D warnings
python3 -B -m unittest discover -s experiments/spuds -p test_oracle.py
```

No compiler limit will be raised or policy guard removed to fit this experiment.
An observed admission boundary is a substantive stopping point, not a successful
Rust puzzle port.


## Reproduce current evidence and retain the historical boundary

Build the pinned tools first and keep the compiler implementation and executables
unchanged during capture:

```sh
python3 -B experiments/stateful-ledger/ci.py --tools target/debug --expected-host aarch64-apple-darwin --output target/stateful-capture-new
python3 -B experiments/stateful-ledger/ci.py --check experiments/stateful-ledger/evidence/receipt.json
python3 -B experiments/spuds/check_boundary.py
```

Use the actual pinned compiler host on other platforms. The shared gate includes
seven native policy tests, thirteen Python recipe/vector/reader tests, both
source-aware Cargo builds, all three VM modes, source controls and SDK replay.
The two complete retained SDK reports are byte-identical. Individual
`capture_success.py` and `capture_mutations.py` commands also offer read-only
`--check` gates. They bind exact current source inventories, compiler-read
attachments, typed outcomes, coin names, witness inputs, complete outputs and
reader corruption controls. They grant no source/execution authentication or
compiler-preservation proof.

The three-bundle Spuds chain's whole generator costs are 87,127,936 uncompressed
and 81,451,936 with backreferences for Chialisp; 850,350,713 / 206,106,713 for
Rust on; and 729,758,911 / 422,762,911 for Rust off. These include actual charged
generator bytes and BLS verification, plus independent exact-cost and one-less
budget controls. Per-step costs and byte counts remain in the report. They use
the pinned simulator's pre-hard-fork-2 configuration, not current live height or
fee estimates.

The unchanged `evidence/receipt.json` records the earlier expansion-depth
boundary, diagnostic IR and Chialisp-only VM lane. Exact original source and
dependency snapshots make that historical packet independently readable; it
has no current-source freshness, present compiler-boundary or current-success
credit. Its unchanged `capture_boundary.py` is a historical reproducer, not the
current successful capture command. No policy guard or compiler budget was
removed to emit the current real source.
