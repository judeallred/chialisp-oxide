# Testnet11 CLI prototype

**WIP prototype. Not ready for use or real funds. Use a disposable Testnet11
wallet.** This describes the narrow live CLI implementation and its first
executed funding/payment cycle below. The loopback GUI remains a simulator interface.
Live CAT issuance, CAT transfer and Sage integration are not implemented here.

The workflow funds one checked Rust payment with exactly 1,000 mojos, then
spends it back to the same wallet. The payment recipient is committed by curry
before funding. The source recipe is the ordinary Cargo library in
[`../recipe`](../recipe), with an annotated `pay(destination: PuzzleHash)` export
and a strict nil spending solution. It is an original teaching payment, not a
general wallet or arbitrary puzzle signer.

## Build before starting

Use the checkout's pinned dated nightly and matching components. A release
build makes the host-side compiler checks, hashing and wallet preflight practical:

```sh
cargo build --release --locked \
  -p cargo-chialisp-oxide -p chialisp-oxide-driver \
  -p chialisp-oxide-lifecycle
mkdir -p target/lifecycle-testnet11
```

Complete builds before executing the driver. Keep source files and installed
tools unchanged during a live workflow. Every command runs native Cargo tests,
fresh extraction in both Rust optimization modes, complete bundle admission,
IR recompilation checks, independent Chialisp vectors and SDK preflight. A saved
report cannot substitute for these checks. Source, tool, wallet, mode or fee
changes can make an existing outbox fail matching; do not edit its identities to
force a resume.

Each invocation requires a new, nonexistent `--output` directory. The output
directory retains that process's public checks. The separate `.live.json`
outbox retains the transaction intent and history across invocations.

## Public wallet identity and local signing

The current adapter supports one exact standard-wallet role:

- `--master-public-key` is the compressed 48-byte public master key, expressed
  as 96 lowercase hexadecimal characters without `0x`.
- `--wallet-puzzle-hash` is the 32-byte puzzle hash decoded from a verified
  `txch1…` address, expressed as 64 lowercase hexadecimal characters without
  `0x`. The CLI takes the decoded hash, not the address string.
- The adapter derives the all-unhardened wallet path `m/12381/8444/2/0`, then
  the standard synthetic key using the default hidden puzzle. It checks that
  this public derivation matches the supplied wallet puzzle hash. A child or
  synthetic public key is not interchangeable with the master public key.

Other wallet paths, hardware signers and arbitrary message signing are outside
this prototype. A mismatched public identity rejects before compiler execution,
network calls or a signing prompt.

Only `prepare-funding` and a fee-sponsored `prepare-payment` request a secret.
The current signer requires a local macOS controlling terminal and reads the
disposable master secret through the native hidden `readpassphrase` prompt.
Enter its 64 hexadecimal characters only at that prompt. There is no mnemonic,
argv, environment-variable, stdin-pipe or key-file input path. Unsupported
platforms or a missing terminal fail instead of falling back to visible input.
The public key and selected inputs are checked against the narrow transaction
plan before signing. No secret belongs in an outbox, check report or command
example.

## Plan, fund and spend

Replace the two public placeholders below with the verified wallet values.
These shell variables contain public information only:

```sh
LIFECYCLE_MASTER_PUBLIC_KEY='<public master key: 96 lowercase hex characters>'
LIFECYCLE_WALLET_PH='<decoded wallet puzzle hash: 64 lowercase hex characters>'
LIFECYCLE_CARGO="$(rustup which --toolchain nightly-2026-07-14 cargo)"
LIFECYCLE_COMPILER="$PWD/target/release/cargo-chialisp-oxide"

live_step() {
  target/release/chialisp-oxide-lifecycle live "$1" \
    --network testnet11 \
    --cargo "$LIFECYCLE_CARGO" --compiler "$LIFECYCLE_COMPILER" \
    --wallet-puzzle-hash "$LIFECYCLE_WALLET_PH" \
    --master-public-key "$LIFECYCLE_MASTER_PUBLIC_KEY" \
    --mode on --output "$PWD/target/lifecycle-testnet11/$2" \
    --outbox "$PWD/target/lifecycle-testnet11/payment.live.json" \
    --fee-mojos 10000 --payment-fee-mojos 10000
}

live_step plan plan-1
live_step prepare-funding funding-prepare-1
live_step submit-funding funding-submit-1
live_step monitor-funding funding-monitor-1
```

`plan` creates the public outbox without signing or submitting. Funding selects
the smallest sufficient coin among a bounded set of observed candidates, then
queries that exact coin again before the hidden signing prompt. It checks the
complete coin identity, unspent state and that its confirmation height does not
exceed the fresh reported tip. Candidate discovery does not establish a complete
wallet inventory or balance.

The funding spend creates the exact 1,000-mojo payment coin and returns change
to the same wallet. The default funding fee is 10,000 mojos, with a cap of
1,000,000 mojos. These are explicit plan values, not a network fee estimate.

Continue only after `monitor-funding` reports `retained_funding_state` as
`confirmed`. If it is still unspent or inconclusive, use another monitor command
with a new output directory. The payment preparation also performs a new
funding-effects observation before it can sign:

```sh
live_step prepare-payment payment-prepare-1
live_step submit-payment payment-submit-1
live_step monitor-payment payment-monitor-1
```

The permissionless payment itself returns its full 1,000 mojos to the wallet.
With the default `--payment-fee-mojos 10000`, a separate wallet coin sponsors the
fee and returns its change to that same wallet. This is a two-input transaction
tracked in the live outbox; it does not alter the original simulator journal's
single-input payment contract. The sponsor requires the payment coin to be spent
concurrently. Only the sealed fresh fixed-payment recipe may enter this signing
path; executing an arbitrary program once with nil is insufficient authority.

`--payment-fee-mojos 0` explicitly selects the single-input, zero-fee
permissionless payment and needs no payment signing prompt. A node may decline a
zero-fee transaction. Choose this fee policy when creating the outbox and keep
the same options on resume. Both fee options default to 10,000 mojos and each is
capped at 1,000,000 mojos.

## Reconcile before retrying

Preparation persists the exact signed bundle, bundle ID, input and child IDs,
fee and local preflight costs before a submission command can push it. A durable
`PushStarted` event precedes HTTP submission. A timeout or lost response is
inconclusive: the node might have received the transaction.

Use `monitor-funding` or `monitor-payment` first after an interruption. An
explicit `submit-*` retry also reconciles before pushing and uses the identical
saved bytes. It never silently selects a replacement input or creates a second
funding transaction. Observed expected effects stop resubmission. Missing,
conflicting or inconclusive inputs block a push. Provider mempool acceptance or
`PENDING` is not confirmation; a provider-reported rejection is not an
independently executed typed consensus rejection.

`live status` reruns fresh checks and revalidates retained transaction intent
against current local SDK rules, provider labels and tip. It does not refresh
spend effects. Its `retained_funding_state` and `retained_payment_state` fields
describe the saved observations; use `monitor-*` for a new observation.

The outbox has finite byte/event limits and reserves room before pushing. Its
storage checks normalized paths, unaliased regular files, cooperating writer
locks, exact prior bytes and append-only history. It is not a hostile-directory
or power-loss guarantee. A post-rename durability error differs from a failure
before commit: reload the durable outbox before deciding what to do next. Never
hand-edit history to bypass a conflict, identity mismatch or exhausted budget.

## Observation and cost boundaries

The separate live client uses the fixed HTTPS Testnet11 origin, finite
deadlines and body limits, with no automatic retry, redirect or proxy. Coinset's
reported `testnet11`/`txch` labels are checked; the genesis challenge and signing
domain come from pinned local Testnet11 constants. The API does not supply an
authenticated genesis assertion. Equal bracketing tips are a provider
consistency heuristic, not chain inclusion or finality proofs.

For confirmation, the runner checks every known input's spent record, matches
its observed puzzle and solution bytes to the locally validated transaction,
and queries every planned child by its exact ID and creation height. The local
preflight established the complete expected output set for those exact spends.
This makes no general claim that a provider's child listing or wallet scan is
complete. Local bundle validation also does not establish coinbase maturity or
full chain eligibility.

Public preflight metrics separate VM execution, condition cost and charged plain
generator bytes. The wallet transaction has a separate 25-million total-cost
ceiling for its canonical wallet spend and optional fee sponsor. The existing
10-million policy VM/preflight bound is unchanged. These are bounded host
validation limits, not compiler budget increases, fee quotes or universal cost
guarantees. The selected actual fresh artifact still has to fit.

Executed local tests and recorded provider responses do not prove compiler
preservation, source or binary authenticity, BLS security, production readiness
or cross-block atomicity.

## First executed CLI cycle

The first retained local capture is
`target/testnet11-first-live-a01/receipt.json` in the execution checkout. It
records fresh native Rust, both Rust modes, independent Chialisp, VM and SDK
checks, hidden-terminal signing, two actual fixed-origin Testnet11 submissions,
and subsequent exact-spend/expected-child observations. This local capture path
is not a published evidence download.

The selected optimized Rust payment was funded at curried puzzle hash
`5c40872a3825aa95a3c84dbf109d960e9af0c1a76d9be0d9d0ba9aadc3a76b85`.
The two observed steps were:

- **Funding:** local signed-bundle ID
  `597ce72092579ef63f1c87be8ed7f06ae04e29e873bcc6e1de6e3412e6499851`;
  spent effects observed at height **4,883,104**. It created the 1,000-mojo
  payment coin and same-wallet change, with a **10,000-mojo fee**. Local
  preflight total cost was **10,179,584**, including **445 charged plain
  generator bytes**.
- **Payment with separate fee sponsor:** local signed-bundle ID
  `dfb744541e6f3e5d64de4b5f057d4d4da1438b47d1e8c337d8f12ebaa3ac16fb`;
  spent effects observed at height **4,883,137**. It returned the full
  **1,000 mojos to the same wallet**, with another **10,000-mojo fee** and
  same-wallet sponsor change. Local preflight total cost was **12,914,524**,
  including **673 charged plain generator bytes**.

The cycle's fees totaled **20,000 mojos**. A separate fresh process then ran
`submit-payment` against the retained outbox, revalidated its exact intent and
observed the already-spent inputs and expected effects. It added no third
`PushStarted` event and made no new push HTTP call.

These heights and effects are HTTPS provider assertions, checked against exact
spent-program bytes and every expected child with equal bracketing tips. They
are not authenticated chain inclusion or finality. The bundle IDs identify the
locally signed bundles; block aggregation need not preserve their boundaries,
so this observation is not an on-chain bundle-boundary proof. The cost figures
are local bundle preflight measurements, not whole-block costs. This cycle adds
no GUI, CAT or Sage execution credit.
