# Spuds: finite hot-potato source experiment

**WIP prototype. Not ready for use or real funds.** This normal unpublished
Cargo library is an original teaching policy for issue #10. It has no singleton,
CAT, wallet integration or live-network lane. The readable Rust source passes native tests but the public compiler check
and both attempted builds stop at the existing CLVM expansion-depth limit.
Only the independent Chialisp reference has executed template VM vectors;
there is no admitted Rust CLVM or complete ledger spend evidence.

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
The minimum age is intended to reject ephemeral same-bundle takes. Actual
consensus boundary and ephemeral rejection tests are required before crediting
that behavior.

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

No wallet builder or signer is supplied here. Native generation of signature,
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

Before promoting this example, execute actual source-derived Rust in both modes
and the independent Chialisp reference on malformed/padded/improper/oversized
input trees. Then use full signed SDK bundles to exercise repeated takes and
claims with actual mined birth/successor heights. Check age boundaries, expiry,
minimum age and ephemeral takes, missing/insufficient contributions, wrong
successor hashes/amounts, stale coins, changed payout with an old signature,
invalid BLS signatures and claim-plus-contribution theft. Rejected bundles must
match exact typed consensus codes and preserve full simulator state. Host and
resource failures remain inconclusive.

Actual source mutants should remove meaningful amount, age, next-state,
announcement or signature-message checks, each funded under its own reveal.
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


## Observed compiler boundary and independent reference

The retained `evidence/receipt.json` records seven native Rust tests, nine Python
recipe/vector tests, fmt and Clippy checks, then actual public `check` and both
Rust build attempts. All three compiler commands stop with `CLVM expansion
nesting limit`; their actual version-4 strict source IR is diagnostic material,
not a compiled or executable artifact. No Rust mode is credited with subset
admission or VM outputs. The original readable Rust policy remains intact;
there were no factoring attempts, policy deletions or compiler limit changes.

The independently compiled Chialisp module is 1,712 serialized bytes. Its 51
actual primary-VM template observations include 19 exact accepted condition
lists and 32 typed program raises for malformed shapes, widths, padded numbers,
invalid booleans, invalid rounds and taking at the cap. Claims cost 25,888 VM
units and takes 62,635 in these finite vectors. These are template costs, not
full generator, transaction or block costs. No batching, fee, Rust performance
or security improvement follows.

Accepted vectors deliberately include an incorrect selected-module field and
invalid BLS key bytes: the template cannot admit its own funding source or
verify returned conditions. A claim VM output is not evidence of passing its
age or signature conditions. No SDK transaction, top-up, timeout, BLS or
live-network test has executed for this experiment.

Build the host tools beforehand and leave the driver unchanged during capture:

```sh
python3 -B experiments/spuds/capture_boundary.py --tools target/debug --output target/spuds-boundary-new
```

The script expects the observed compiler boundary, retains its logs and fresh
actual-source IR, independently compiles the reference, and executes only that
reference lane. It freezes fourteen project inputs, four declared reader
inputs and the selected host tools. Retained bytes, command records and matching
hashes establish consistency, not source/execution authentication or compiler
preservation. The installed compiler, dependencies and mutable native build
inputs remain assumptions. Resolve this admission boundary with reviewed
semantics and regressions before planning Rust SDK game transactions.
