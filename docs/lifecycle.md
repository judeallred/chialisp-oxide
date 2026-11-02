# Local coin puzzle lifecycle

**WIP prototype, not ready for use. This workflow uses a seeded SDK simulator.
It does not fund or spend live coins.** No wallet is needed for the local flow.

The unpublished `chialisp-oxide-lifecycle` companion provides one shared runner
for its CLI and loopback GUI. Its fresh payment recipe is an ordinary Cargo
library with an annotated `pay(destination: PuzzleHash)` export. The public
recipient is committed by curry before simulator funding; the nil solution
creates one 1,000-mojo child from an exactly 1,000-mojo input.

## Build the local tools

From this checkout, use the pinned dated nightly and matching components:

```sh
rustup show
cargo build --workspace --bins --examples --locked
mkdir -p target/lifecycle-local
```

Keep native Cargo tests and the additional CLVM subset check distinct. The fresh
command runs native tests, extracts both Rust optimization modes, admits their
complete source-aware bundles, checks IR recompilation consistency, compiles an
independent Chialisp reference, executes VM vectors and submits SDK simulator
spends. Consistency and finite execution do not establish compiler preservation,
source authenticity or production readiness.

## Run, interrupt and resume

This example's `23` recipient is an arbitrary simulator fixture:

```sh
target/debug/chialisp-oxide-lifecycle fresh run \
  --cargo "$(rustup which --toolchain nightly-2026-07-14 cargo)" \
  --compiler "$PWD/target/debug/cargo-chialisp-oxide" \
  --destination-hex 2323232323232323232323232323232323232323232323232323232323232323 \
  --mode on --output "$PWD/target/lifecycle-local/check-1" \
  --journal "$PWD/target/lifecycle-local/payment.lifecycle.json" --steps 3
```

Use the same options with `fresh resume` and a new output directory, such as
`check-2`, to continue. `fresh status` checks and replays without changing the
journal. Every process executes fresh checks before admitting saved events;
each invocation needs a new capture directory. `--mode off|on` selects the
actual reveal used for the simulator coin, while checks cover both modes.
Source, tool, recipient or mode changes make a checked journal fail matching.
Saved report JSON cannot manufacture a validated recipe.

The seven durable transitions record the artifact, executed checks, exact
simulator funding coin, permissionless bundle, actual SDK validation,
submission and complete child observation. A public funding request records
the network/challenge, selected mode, source-check digest, reveal tree/blob
identities, nil-solution digest, recipient and complete output. Its fee is zero.
This is an unfunded public plan; the fixture recipient establishes no ownership.

## Open the same flow in the GUI

Use `fresh serve` with the same identity options, a new check directory and
either the existing journal or a new `.lifecycle.json` path. Open the printed
loopback URL. Compilation and checks finish before the listener opens; HTTP
cannot select compiler paths or import validation assertions.

The GUI advances the same revision-checked commands, displays exact module,
curried reveal, input and child identities, and can restore an actual unspent
SDK checkpoint to exercise reconciliation. CLI status can reopen that journal.
Checkpoint restoration models a simulator reorg. Fresh checks are labeled
`Executed this process`; the separate historical `simulator` commands label
fresh compilation unavailable.

Displayed costs come from the executed deterministic preflight bundle under
`chia-consensus 0.36.1`, SDK VM `clvmr 0.16.2`, Testnet11 rules and its recorded
height, flags and ceiling. SDK validation cost includes charged generator bytes,
VM execution and conditions. Reveal/solution byte counts are shown separately.
These are preflight measurements; the zero fee is a plan value, not a network
fee quote.

The server binds IPv4 loopback with exact Host/Origin checks, an in-memory
capability, bounded requests and one journal-operation permit. A timed-out HTTP
request may finish committing; reload durable state before retrying. Storage
checks cover cooperating writers, not hostile directories or power-loss safety.
Keep all journals and receipts public: never put wallet secrets in them.

## Separate Testnet11 CLI prototype

The library also provides a read-only, fixed-origin HTTPS observer with finite
deadlines and streamed body limits. Its tests use local mocks and loopback HTTP.
For a spent coin it requires complete bounded parent-query pagination, including
already-spent children. Partial scans remain inconclusive. Equal bracketing tips
are a provider consistency heuristic, not authenticated chain inclusion or
finality. This observer is not connected to the CLI/GUI and exposes no push API.

A separate [live CLI prototype](https://github.com/judeallred/chialisp-oxide/blob/main/crates/chialisp-oxide-lifecycle/docs/live.md)
now connects a private freshly checked recipe to narrow Testnet11 funding,
hidden-terminal signing, explicit submission and exact-effect monitoring. It
uses its own public `.live.json` outbox; the simulator journal and GUI above
retain their contracts. The new client checks provider labels and pins local
signing constants, without inventing a remote genesis assertion or changing
this older read-only observer.

The payment returns all 1,000 mojos to the same wallet. Funding and an optional
separate payment fee sponsor use the standard wallet; the permissionless payment
itself emits no owner-signature condition. The current signer accepts a
disposable master secret only through a local macOS hidden terminal prompt,
never through CLI arguments, environment variables, files or journals.

The [first executed CLI cycle](https://github.com/judeallred/chialisp-oxide/blob/main/crates/chialisp-oxide-lifecycle/docs/live.md#first-executed-cli-cycle)
used two Testnet11 submissions, returned all 1,000 mojos to the same wallet and
paid 20,000 mojos in total fees. Its exact-spend and expected-child observations
are provider consistency evidence, not authenticated inclusion or finality.
A separate `live serve` loopback GUI now exposes the live workflow's seven
closed actions with exact outbox revisions, one background job at a time and
cached public observations. Signing stays in the server's hidden terminal;
the browser has no secret-entry field. See the [live GUI instructions](https://github.com/judeallred/chialisp-oxide/blob/main/crates/chialisp-oxide-lifecycle/docs/live.md#open-the-live-loopback-gui).
The separate [closed CAT issuance workflow](https://github.com/judeallred/chialisp-oxide/blob/main/docs/testnet11-cat-live.md) has issued a
Rust TAIL CAT on Testnet11 and observed it in Sage. CAT transfer and a second
wallet round trip remain open. Issue #12 stays open for its full acceptance
criteria, and #13 still requires actual wallet spending beyond discovery.

See the [crate implementation and detailed contracts](https://github.com/judeallred/chialisp-oxide/tree/main/crates/chialisp-oxide-lifecycle)
and [provider boundaries](https://github.com/judeallred/chialisp-oxide/blob/main/crates/chialisp-oxide-lifecycle/docs/provider.md).

## Authenticated continuation CLI and GUI

The separate `live continuation-*` commands use one Rust `ContinuationSession`
for the original batch-one policy. They are WIP Testnet11 tooling; the simulator
commands above remain local. This is separate from the later batch-two/spuds
experiments and does not credit a new live trial.

Prepare a public JSON file of 1..8 recipient puzzle-hash hex strings. Before any
wallet signing callback, each new process executes actual current-host native
Cargo tests, both complete source-aware compiler bundles, independent reference,
VM/SDK checks and source controls. Rust independently admits/recompiles all fresh
bundle exports, SDK-checks supported counts 1..8 and every selected queue step,
and commits the complete queue plus selected module/instances and public wallet.
Retained JSON alone cannot supply fresh execution authority.

Use `live continuation-plan` with the explicit `--network testnet11`, pinned
`--cargo`, `--compiler`, public `--wallet-puzzle-hash`, public
`--master-public-key`, `--mode on|off`, `--recipients FILE`, and a new `--output`
directory. Transaction commands additionally require a dedicated
`--outbox FILE.continuation.live.json`; keep the same immutable identity, queue,
mode, `--fee-mojos` and `--step-fee-mojos` on every reopening. The complete command
example is in [the companion README](https://github.com/judeallred/chialisp-oxide/blob/main/crates/chialisp-oxide-lifecycle/README.md#shared-continuation-trial).

1. `continuation-prepare-funding` selects and point-requeries one wallet input,
   invokes the existing hidden terminal signer, and durably saves exact funding.
2. `continuation-submit` pushes that saved transaction once.
3. `continuation-watch` checks provider-reported exact spend bytes and every
   child from the complete locally SDK-executed output set.
4. `continuation-prepare-step` reobserves the preceding transaction, checks the
   next exact coin and prepares its permissionless opening; repeat submit/watch.

`continuation-status` reopens history without provider calls.
`continuation-serve` exposes those same shared operations through the existing
capability-protected, same-origin loopback server. HTTP accepts closed actions
and an exact journal revision; programs, queues, fees, tools and keys cannot be
selected through the page. Opening/polling the GUI never submits or watches the
network automatically. Signing stays in the local terminal.

Funding is exactly `1000 × queue length` mojos; each step pays 1000 mojos to its
committed recipient. A positive step fee uses a separate purpose-specific wallet
sponsor with concurrent-spend binding and exact change. The separately measured
continuation SDK ceiling is 5.5 billion cost units; existing fixed-payment,
wallet/CAT and compiler limits remain unchanged.

A durable push-start permanently closes resubmission, even if the response is
lost or the process exits. Reload/status and watch reconcile the exact saved
transaction; no action blindly retries or replaces inputs. Changed source/tools,
queue or fees fail matching. The bounded journal rejects stale revisions and
history rewrites before provider/signing callbacks. A provider coin record newer
than the bracketed peak is inconclusive. Exact point records establish provider
consistency against locally complete effects, not a complete provider inventory,
authenticated inclusion, ownership or finality.

The focused Rust tests exercise all supported queue lengths, actual bad-BLS and
replay SDK rejections with complete simulator equality, four sponsored steps and
reload, response-loss recovery, pre-callback revision checks, provider snapshot
races, storage controls, and the actual loopback page and cached session view.
They remain fixture executions, separate from production source certification
and any future funded live run.

`continuation-watch` performs one observation and exits; it is not a watch loop.
GUI polling reads the cached local view; provider monitoring requires an explicit
button action. The journal is bounded to 96 events and 2 MiB. Repeated monitoring
can exhaust these bounds, after which further durable progress stops. There is
currently no pruning, migration or reorg-recovery workflow. The journal provides
no unbounded liveness guarantee; do not create replacement funding or blindly
resubmit to bypass a bound or conflicting observation.

The stable journal source identity includes the sorted root-relative compiled
native-input declarations and the exact three admitted package-source attachment
digests, alongside both IR/artifact identities and installed tool fingerprints.
A comment-only source or implementation change therefore cannot silently upgrade
an existing session even when the compiled module bytes remain identical. Fresh
output paths and run IDs are excluded from this identity.
