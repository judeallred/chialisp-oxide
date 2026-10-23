# Public lifecycle journal foundation for #12

WIP prototype, not ready for use. This crate supplies a shared state machine,
trusted local simulator recipes and a separate read-only provider foundation;
it does not implement a live funded lifecycle. The isolated sibling workspace
passed 63 lifecycle unit tests, one recipe unit test and four real-process CLI
integration tests, plus Clippy with warnings denied. All 63 library tests were
rerun after the final test-style correction. Twelve core tests are constructed
host-model controls; runner tests separately execute native code and the SDK
simulator. Seven library tests exercise the GUI HTTP server. The provider has
13 mock controls and five actual loopback HTTP tests, without external HTTPS
execution credit. CLI tests invoke 11 child processes; process controls invoke
six more. Historical retained-fixture browser review exercised seven advances,
exact child, checkpoint reorg, reload and mobile layout. The final isolated CLI
reopened its revision-8 journal without changing bytes. Fresh-GUI browser review
and integrated-root production capture remain separately gated. No existing
compiler interfaces change here.

The fresh adapter is `fresh::check_fresh`, shared by the explicit
`simulator check-fresh` command. Its ordinary Cargo `recipe/` library takes a
typed 32-byte destination committed by canonical curry and produces complete
1,000-mojo conditions. Source implementation invokes bounded installed native
Cargo tests and the production dual-mode source-aware compiler, admits every
bundle through the package API, compares exact IR recompilation and executes
independent Chialisp/native/VM/SDK lanes. Fresh dual-mode Rust extraction and
VM/SDK execution passed the complete fresh command: five orchestrated tool
commands, independent Chialisp compilation, 33 primary-VM observations and six
SDK submissions in three groups. The arbitrary `[0x23;32]` destination is a
simulator fixture. Existing ABI behavior is witness/mode scoped: only the two
explicit extra pair tails use Rust/off `InvalidOpArg`, with Rust/on `Raise`;
all other cases retain exact expectations. Failed runs preserve bounded
completed observations and actual submission counts without a success report.
The public journal does not consume arbitrary fresh receipt assertions. The
source integration connects only a private executed `ValidatedRecipe` to the
shared runner/CLI/GUI; fresh checks run before each process opens a journal or
listener. That integration has native controls, while the earlier a03 capture
remains historical and does not cover this newer source.

The first supported plan is one testnet11 input with complete child destinations
and amounts, plus an implicit fee. A build event retains the actual serialized
CLVM reveal and solution. The input puzzle hash derives from those exact reveal
bytes; source text hashes and module names cannot substitute for a reveal.
Coin IDs use the pinned SDK's `Coin::coin_id`, including amount encoding.
Both reveal and solution first pass the existing package artifact inspector in
explicit plain mode: 1 MiB serialized/expanded/atom bytes, 65,536 nodes, depth
512, 2 MiB atom heap and 16 MiB charged work per inspection. Structural
backreferences are unsupported; an atom containing byte 0xfe remains valid.
Admission precedes program copies and tree hashes. These local host bounds
are neither compiler budgets nor on-chain policy guards.

`Journal::append(revision, Event)` admits an event atomically in memory or leaves
the journal unchanged. CLI and GUI callers use the same transitions and revision
preconditions. `to_json` exports versioned public data; `from_json` reconstructs
the state by replaying every event. A cached stage is never accepted. Event and
byte limits reject growth rather than trimming history. File persistence,
writer locking and replacement are separate `JournalStore` responsibilities;
see README for cooperating-writer and durability limits. Revisions alone do
not serialize concurrent processes or authenticate a file.

The order is build, external native/CLVM checks, observed funding, external
signing, explicit external signature validation, submission acknowledgement,
and chain reconciliation. The check event binds the exact artifact digest and
complete ordered outputs to retained native and VM public receipts. The journal
does not interpret those receipts, execute either check, or establish that the
declared outputs actually follow from the program. The trusted executor adapter
must perform those operations before emitting that event. Generic journal tests
use constructed adapter declarations and make no native/VM execution claim.
`SimulatorRunner` separately calls the actual fixed-payment native source and
executes its retained CLVM before emitting its own check record; resume
reexecutes and compares every event. This narrow recipe does not upgrade
arbitrary journal declarations or establish fresh source-to-CLVM compilation.

The signing request includes the network/challenge, exact funded coin, serialized
unsigned SpendBundle constructed from actual reveal/solution bytes, and all
expected child Coins. Its ID derives from the complete structured request.
The external signing receipt retains bytes without granting submission access.
The pinned streamable SpendBundle format is a vector of coin spends followed
by one 96-byte G2 signature. Core compares the complete unsigned coin-spend
prefix and exact total length before decoding any signer-provided data, avoiding
allocations based on untrusted declared vector lengths. Signature validation
requires a separate event matching both request ID and actual signed-byte digest;
it also decodes the fixed G2 point using the pinned BLS implementation. Point
validity is not message/signature verification. The external validator still
owns full network/domain/condition/signature validation and supplies a public
receipt. Recording an assertion is not authentication of that validation.
Only then does `submission_bytes` expose the approved bytes to a submitter.

Every observation names one exact input ID and network and includes a tip and
complete current child set. Matching input spend heights, child parents, child
creation heights, coin IDs and exact outputs can mark the observed spend
confirmed. This means the selected coin produced the planned outputs, not that
a particular wallet transport acknowledgement or authenticated chain was proved.
It works when an application stops after broadcasting but before recording an
acknowledgement. Missing or additional outputs mark a conflict. A later unspent
observation revokes confirmation and offers the same explicitly validated bytes
for resubmission; disappearance clears funding and signing state. Any new input
requires a new request and validation. An observer must serialize snapshots and
verify their current chain view: tip hashes alone do not establish ancestry,
freshness or chain authentication, and a reorg may legitimately lower height.

The loopback server is a thin adapter: GET public state and POST a closed,
revision-conditional simulator intent. It binds only to IPv4 loopback, validates
Host/Origin, uses an in-memory session capability and serves bundled assets
without CORS. It contains no wallet RPC or chain submission code. It never
exposes unchecked event insertion that would let clients forge check or
signature-validation assertions. Its single blocking-operation permit survives
an HTTP timeout: a started operation may still commit, and the browser must
reload durable state before retrying. README records the HTTP resource limits.
Public receipts must never contain private keys or wallet secrets; caller data
is not scanned or sanitized here.

The fresh Cargo/native/reference/VM/simulator adapter is implemented and executed;
the new source integration connects its private executed recipe to the journal and GUI.
New production captures of that integration remain separate from historical a03.
Main-workspace integration preserves the preceding registry lock changes;
combined gates and captures are recorded against the integrated sources.

The trusted runner now consumes a private validated fresh-recipe result binding
the exact curried reveal, nil solution, complete outputs and executed checks
before simulator funding. CLI and GUI invoke the same closed revision-checked
commands. Each fresh process reruns the adapter before replay; an arbitrary
saved report or caller-provided check event cannot grant validation credit.
Stable source/tool identities deliberately prevent silent provenance upgrades
of a checked journal. GUI scope distinguishes the retained fixture from checks
executed in the current process.

A bounded read-only Testnet11 provider observer is implemented separately with
mock and loopback tests. Next connect it and add a submission adapter, keeping
provider observations separate from authenticated chain validation. A public
recipient, external funding method and exact funded coin remain required, along
with a reviewed fee and polling policy. This permissionless recipe has no spend
secret and creates the full 1000-mojo output; funding is signed separately by an
external wallet. The historical simulator destinations are not evidence of a
user-owned live destination.

Finally execute the disposable Testnet build/fund/push/confirmed-children flow
and stop/resume/reorg demonstrations through both CLI and GUI. The current slice
does not close #12 or establish live-chain, signature or compiler-preservation
proofs. No new Rust frontend constructs are introduced.
