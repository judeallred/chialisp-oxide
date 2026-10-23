# chialisp-oxide-lifecycle

**WIP prototype, not ready for use. Simulator only; no live deployment.**

The library exposes a bounded public journal, cooperating-writer storage and a
trusted command runner. The CLI drives that same runner:

```sh
cargo run -p chialisp-oxide-lifecycle -- simulator run
cargo run -p chialisp-oxide-lifecycle -- simulator status
cargo run -p chialisp-oxide-lifecycle -- simulator resume
cargo run -p chialisp-oxide-lifecycle -- simulator serve
```

The default journal is `target/lifecycle-simulator.lifecycle.json`. An explicit
`--journal PATH.lifecycle.json` requires an existing parent directory. `run`
creates a new journal and refuses to overwrite an existing one. `status` leaves
the journal unchanged but reexecutes the local trusted recipe to validate it.
`resume` validates and replays retained commands, then continues. `--steps 1..7`
on `run` or `resume` stops after that many additional durable transitions; this
lets the same process-independent CLI exercise an interrupted run.

`serve` opens a new or existing journal through that same trusted runner and
prints a plain `http://127.0.0.1:PORT` URL. Its embedded GUI imports the retained
artifact, advances one trusted action at a time, displays exact input/child IDs,
refreshes observations and restores the actual unspent simulator checkpoint.
It visibly marks fresh compilation, extraction, reference checks and live
deployment unavailable. Isolated native checks passed 63 lifecycle unit tests,
one recipe unit test and four real-process CLI integration tests, plus Clippy
with warnings denied. Seven library tests exercise the GUI HTTP server; the
provider has 13 mock controls and five real loopback HTTP tests. CLI tests invoke
11 child processes; process controls invoke six more. No external HTTPS
provider request or live spend is credited by those tests.
Historical retained-fixture desktop/mobile browser review exercised all
seven advances, exact confirmed children, checkpoint restoration and reload.
A separate CLI status reopened the same GUI journal after restoration. The final
isolated CLI also reopened that original revision-8 journal after the fresh
integration, preserving all 8,785 file bytes and its ready-to-submit state.
The integrated workspace passed 66 lifecycle library tests, one recipe test
and four real-process CLI integration tests. A fresh CLI → GUI → CLI rehearsal
also executed: CLI stopped at revision 3, the GUI confirmed the exact child at
revision 7, checkpoint restoration and reload retained revision 8, and CLI
resume reconfirmed that same input and child at revision 10. Desktop 1280×800
and mobile 390×844 browser review found no horizontal overflow or page errors.
Each of the three processes independently executed the same 33 primary-VM
observations and six SDK check submissions; repetition adds no unique scenario
coverage. Journal initialization, replay and lifecycle submissions remain
separate from those six check submissions. No live network execution is credited.

The server accepts only the exact loopback Host and same-origin browser
requests. A random in-memory session capability is delivered only in the
no-store, frame-protected HTML and required for API calls; it never appears in
URLs, public state, journals or logs. No CORS or external assets are enabled.
This browser-origin protection does not authenticate against local processes.
The listener is fixed to `127.0.0.1` with an ephemeral port; neither address nor
journal path can be supplied through HTTP.

HTTP/1 parsing uses the pinned Hyper implementation: at most 24 headers, an
8-KiB header/read buffer, a two-second header deadline, a 2-KiB command body,
a two-second body deadline, eight concurrent connections and a five-second
connection lifetime. Keep-alive is disabled. Only one blocking journal operation
may run at a time. A connection timeout does not cancel a started filesystem
operation: it retains its operation permit until completion and may commit
after the browser disconnects. The GUI requires a durable-state reload before
retrying; revision checks prevent a stale duplicate advancement. HTTP resource,
transport and storage errors are not counted as policy rejections.

The historical intermediate fixed permissionless recipe consumes 1,000 simulator mojos and
creates exactly one 1,000-mojo child at `[0x42; 32]`. There is no authorization
key, external signer or fee in this recipe. The aggregate signature is empty
because no signature conditions are emitted; actual pinned SDK validation runs
before submission. No public destination here is a real wallet or funding
instruction.

## Fresh recipe checks

The ordinary Cargo library in `recipe/` exports `pay(destination: PuzzleHash)`:
assert exactly 1,000 input mojos and create exactly one 1,000-mojo destination
child. Commit the public 32-byte destination **atom** by curry before funding;
the solution is exactly nil. This remains a permissionless, zero-fee teaching
payment and does not use a private signing key. Its original independent
Chialisp counterpart validates the complete strict one-argument environment.

The source implementation is available through the shared `fresh::check_fresh`
adapter and the CLI:

```sh
chialisp-oxide-lifecycle simulator check-fresh \
  --cargo /absolute/pinned/nightly-2026-07-14-HOST/bin/cargo \
  --compiler /absolute/installed/cargo-chialisp-oxide \
  --destination-hex PUBLIC_64_LOWERCASE_HEX_DIGITS \
  --output /existing/parent/new-run
```

It runs actual ordinary Cargo library tests, then fresh production extraction
in both optimization modes. It admits the complete schema-2 bundles using the
production package reader, binds exact export/package/options/declared native
source, and compares production IR recompilation with the emitted bytes.
That comparison is consistency, not compiler preservation or authentication.
The compiled native library's source inputs, workspace lock/toolchain inputs,
installed executable bytes and bounded toolchain library inventory are checked
around stages; a changed source tree requires rebuilding the lifecycle binary.
These checks assume ordinary cooperating local processes and mutable build
inputs; they do not make Cargo/build scripts a sandbox or authenticate installed
binaries. Each child process group has a 180-second deadline, separate 1-MiB
stdout/stderr limits, and kill/reap handling. Bounded logs remain in the new run
directory on failure. Existing output directories are refused.

The installed-library snapshot covers immediate sysroot `lib` files and the
pinned host's `rustlib/HOST/lib` and `rustlib/HOST/codegen-backends` subtrees,
for `.dylib`, `.so`, `.rlib` and `.rmeta` files. Limits are 4,096 files, 512 MiB
per file, 2 GiB aggregate declared bytes and depth eight from `lib`; symlinks
are refused. Installed Rust source and documentation trees are omitted. This
is bounded consistency for those named roots, not complete build-input capture
or authentication. The exact root/type/bound scope is retained in the report.

The pinned `nightly-2026-07-14` Linux `x86_64-unknown-linux-gnu` profile also
includes the exact immediate regular file
`libLLVM.so.22.1-rust-1.99.0-nightly`. The official rustc distribution's
`libLLVM-22-rust-1.99.0-nightly.so` is a constant 43-byte linker script pointing
to that versioned ELF, also named by the rustc driver's `DT_NEEDED` entries.
Both files enter the bounded snapshot; other versioned names, nested paths and
hosts receive no exception. Symlinks remain unsupported. The
[dated official channel manifest](https://static.rust-lang.org/dist/2026-07-14/channel-rust-nightly.toml)
pins the inspected Linux rustc archive to SHA-256
`b1356f3c28481b189f1b5dbe08da9e66384dd644448c20b95c8a065e4ea82e98`.
This distribution-byte check does not authenticate installed files, prove
which libraries a process loaded, or cover operating-system libraries.

The adapter independently compiles Chialisp, executes exact complete outputs
and malformed whole-environment/curry vectors in the primary VM, retains typed
Rust/reference outcomes separately, and checks an exact finite oracle per mode,
case and complete environment. For the two extra-pair-tail witnesses only,
Rust/off expects `InvalidOpArg`: base `(DEST ())` and curried solution `(1)`.
Rust/on expects `Raise`. This is the existing decoder behavior documented by
[the named-condition tests](../chialisp-oxide-driver/tests/named_conditions.rs)
and boomerang's recorded code table. The other nine Rust cases retain exact
output/code expectations; the independent reference's nine negative `Raise`
expectations were separately executed for three destinations.
Each of three language/mode lanes submits one complete successful
SDK spend plus an actual 999-mojo invalid bundle, with exact
`AssertMyAmountFailed` and full in-memory simulator equality. The independent
reference/curry/SDK controls and recipe native tests have executed in isolation.
The complete fresh command also passed with an arbitrary simulator-only
destination `[0x23;32]`: actual native Cargo tests, both Rust extractions,
independent Chialisp compilation, 33 primary-VM observations (six accepted,
27 rejected with exact codes), and six SDK submissions in three groups
(three accepted and three rejected with full state unchanged).
That a03 capture predates the fresh journal/GUI integration below and remains historical.
A bounded read-only Coinset HTTPS observer is available separately; CLI/GUI network integration, push and live confirmation remain unavailable. The retained `[0x42;32]` fixture stays simulator-only and never
becomes a user's live recipient.

Execution/oracle/SDK/final-freeze failures retain `failed-observations.json`
with overall status `failed`, completed actual program/environment bytes,
expected and observed typed VM outcomes, completed SDK observations and the
actual submission count. Resource/infrastructure outcomes remain inconclusive
and fail the check. Success and failure JSON writes stream through a 16-MiB
cap and refuse existing files; child-command failures keep bounded logs.
Incomplete stages add no execution credit. Scratch failure captures remain
failed after a later successful retry.

The retained optimized Rust artifact is copied exactly from
`corpus/validation.json`, slug `fixed-payment`, backend `rust_mir`, optimization
on, commit `18d2f3bebc16c0ed460203d6751502719f1a5d91`:

- Serialized artifact SHA-256:
  `4571a31579378b03c1aeef5db4a394325474c0e1b185b187732d8d24bb87bac1`
- Module tree hash:
  `9a630f1e6d93705847614fc4b23b5f5cbf330edf802280684a050a572f47b6b9`
- Direct native source SHA-256:
  `e3d32e261b8403c9bf971f7d6c621552ff9ddf0964413c0e01f838f28540a84f`

The runner checks these identities, calls the actual linked
`chialisp_oxide_puzzles::fixed_payment::puzzle`, compares its returned conditions,
executes the retained program through `chia-consensus 0.36.1` with TESTNET11
constants and a 10-million cost cap, and requires the complete expected outputs.
It also submits a real underfunded bundle to `chia-sdk-test 0.36.0`, requires
exact `AssertMyAmountFailed` and compares the entire simulator with its clone.
The successful run submits the complete bundle to the SDK simulator and reads
its actual input and child states. Reports use ordered public records, never
simulator `Debug` text or internal map serialization.

Resume reconstructs the seeded SDK state by executing trusted commands again;
it compares every generated event to the retained event. A generic journal's
unchecked external validation assertions cannot unlock this runner. Local
simulator reorg tests restore a real SDK checkpoint and reconcile it. This is
deterministic simulator recovery, not persistence or authentication of a live
node state.

`JournalStore` supports reviewed Unix hosts. It checks regular files, a single
hard link, symlink-free path components, a dedicated basename and an empty
nonblocking locked sidecar. Commit compares prior bytes and append-only history,
writes a new 0600 temporary file, syncs it, renames it and syncs the parent.
`BeforeCommit` means this operation did not replace the journal;
`CommittedDurabilityUnknown` means replacement occurred and its durability is
unknown. Callers must inspect/reload after the latter rather than blindly retry
as a new create. Unrelated files and mismatched plans are refused. These checks
cover cooperating writers, not hostile directory mutation or power-loss safety.
The public journal must contain no keys or wallet secrets.

The historical `simulator` commands do not execute fresh source extraction.
The separate `fresh` workflow does; neither workflow establishes compiler
preservation, source authenticity or a complete native build graph. CLI/GUI Coinset integration, external wallet funding, live fee policy, singleton lineage and
actual testnet execution required by #12 remain unfinished.

## Fresh local journal and GUI

The source integration exposes the same trusted runner after executing the
complete fresh adapter, with no wallet required:

```sh
chialisp-oxide-lifecycle fresh run \
  --cargo /absolute/pinned-toolchain/bin/cargo \
  --compiler /absolute/target/debug/cargo-chialisp-oxide \
  --destination-hex 2323232323232323232323232323232323232323232323232323232323232323 \
  --mode on --output /absolute/new-capture-directory \
  --journal /absolute/payment.lifecycle.json --steps 3
```

Use `fresh status`, `fresh resume` or `fresh serve` with the same identity
options and a **new output directory for every invocation**. `--steps 1..7`
applies only to run/resume. Both optimization modes and the independent
reference are checked; `--mode off|on` selects the actual curried reveal locked
by the simulator coin. `serve` completes the checks before opening its loopback
listener, then prints only the plain URL. HTTP cannot select tools, paths,
receipts or raw events. No compiler process runs inside an HTTP request.

Only the fresh adapter can construct the private-field `ValidatedRecipe` used
by these commands. No report import or deserializer can turn saved declarations
into execution credit. Each process executes fresh checks again, then compares
every saved event against deterministic SDK replay. The stable journal check
binds selected mode, exact module/instance hashes, recipient, source fingerprint,
compiler/tool/library identities and complete output. Source/tool changes make
an existing checked journal fail matching; they do not silently upgrade its
provenance. Source inputs (including bundled HTML/JS/CSS assets) are checked again before trusted actions. A saved
journal alone is insufficient to reopen a fresh session.

The GUI labels fresh checks `Executed this process` only for that executed
recipe. Retained fixtures continue to show them unavailable. Its identity pane
separates the module hash, curried funded-reveal hash and public recipient.
Public state includes an unfunded request with Testnet11 challenge, selected
mode, source-check receipt digest, reveal tree/blob identities, nil-solution
digest, complete 1000-mojo output and fee zero. This is a public plan, not proof
of live funding or ownership. An external wallet signs funding separately;
the permissionless spend itself emits no owner signature condition.

Count the layers separately: a successful fresh report has 33 primary VM
observations and six SDK submissions. Constructing the runner performs one
additional actual underfunded SDK submission plus direct aggregate validation;
a seven-step new journal adds one accepted simulator submission. Resume reruns
fresh checks and initialization, then performs submissions represented by saved
commands during replay. These are additional executions, not new a03 credit.
The source integration's native tests and a new production capture are tracked
separately from the historical a03 observations.

Public view metrics expose actual reveal/solution byte counts and the executed
deterministic preflight SDK validation's conditions.cost, input ID, flags and
ceiling, scoped to consensus 0.36.1 / SDK VM 0.16.2 / Testnet11 height zero. This
cost includes SDK charged generator bytes, VM and conditions. It is not the
primary VM cost, a measurement of the later funded coin, a full-block cost or a
network fee quote. The displayed zero fee is the plan's value.
