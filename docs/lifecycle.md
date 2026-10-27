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

The [first executed CLI cycle](../crates/chialisp-oxide-lifecycle/docs/live.md#first-executed-cli-cycle)
used two Testnet11 submissions, returned all 1,000 mojos to the same wallet and
paid 20,000 mojos in total fees. Its exact-spend and expected-child observations
are provider consistency evidence, not authenticated inclusion or finality.
A separate `live serve` loopback GUI now exposes the live workflow's seven
closed actions with exact outbox revisions, one background job at a time and
cached public observations. Signing stays in the server's hidden terminal;
the browser has no secret-entry field. See the [live GUI instructions](../crates/chialisp-oxide-lifecycle/docs/live.md#open-the-live-loopback-gui).
Live CAT issuance/transfer and Sage integration remain unimplemented here.
Issue #12 stays open, and CAT/Sage work in #13 has its own acceptance criteria.

See the [crate implementation and detailed contracts](https://github.com/judeallred/chialisp-oxide/tree/main/crates/chialisp-oxide-lifecycle)
and [provider boundaries](https://github.com/judeallred/chialisp-oxide/blob/main/crates/chialisp-oxide-lifecycle/docs/provider.md).
