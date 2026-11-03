# Authenticated continuations: exploration design

**WIP exploration and narrow prototypes. Not ready for use or real funds.**
This records the explicit teaching experiments for
[#16](https://github.com/judeallred/chialisp-oxide/issues/16). A generic continuation SDK type and automatic splitting
remain unimplemented. The shared Rust CLI/GUI now has a purpose-specific
batch-one session for fresh planning, funding, fee sponsorship, monitoring and
process recovery; see [the lifecycle commands](https://judeallred.github.io/chialisp-oxide/reference/lifecycle/#authenticated-continuation-cli-and-gui). An isolated [ordered-payout ABI/hash spike](https://github.com/judeallred/chialisp-oxide/tree/main/experiments/authenticated-continuations)
supplies a concrete annotated export, independent host oracle and native
adversarial tests. Its actual subset check, both-mode builds and 108 finite
primary-VM observations pass. A separate finite ledger milestone now mines
three four-step SDK simulator chains and records actual-source counterexamples
and generator measurements. The issue stays open; this is an explicit teaching
policy rather than a general continuation feature. A separate experimental
host workflow now admits the complete published artifact and queue, persists
public progress across processes, and supports fixed-provider Testnet11
observation and explicit submission. That historical Python workflow is separate from the new shared Rust
CLI/GUI session; see the [workflow commands](https://github.com/judeallred/chialisp-oxide/tree/main/experiments/authenticated-continuations#offline-funding-plan-and-process-recovery).

The intended feature lets an author divide committed work into bounded spends.
Each accepted spend performs one specified chunk and commits the next coin to
the remaining work. This requires an explicit policy for order, funds and
termination; recreating a coin alone does not establish those properties.

## What the XCHandles case motivates

The prepared [XCHandles case study](https://github.com/judeallred/chialisp-oxide/issues/15)
traces 129 authorized registration batches covering 3,836 pinned names. The
observed registry state and controller puzzle hash remain constant across those
batches. Those observations do not establish a commitment to a remaining CSV
queue. Authorization and sorted-slot checks serve different purposes from
authenticating the complete remaining workload.

The proposed continuation demo is an original teaching policy. It must introduce
and test its own work commitment. It must not relabel the XCHandles cascade as
an already implemented authenticated continuation or a completed Rust port.
Whole-block costs and fees also remain separate from the cost of any one
registration bundle.

## Authoring model

Use an explicit state machine in an ordinary Cargo library, depending on the
unpublished `chialisp-oxide` facade and exporting a concrete annotated step
function. Native `cargo check` and `cargo test` check Rust; the additional
`cargo chialisp-oxide check` and `build` check the reachable CLVM subset. Start
with the [authoring guide](https://judeallred.github.io/chialisp-oxide/reference/agent-guide/).

Three possible interfaces have different costs:

- **Explicit state machine:** the author defines the committed state, the
  opening supplied by the caller, and the exact next conditions. This exposes
  authentication, funding and failure behavior for review and is the recommended
  first model.
- **Annotated yields or generators:** the compiler chooses suspension points and
  captures live values. This needs new semantics for serialized locals, strict
  evaluation, failures, cost bounds and versioning. Defer it until an explicit
  model has working evidence.
- **Fixed-batch combinators:** a library can later package a reviewed ordered
  queue or payout recipe. First establish the concrete policy and measurements;
  a generic helper must not imply support for arbitrary iterators or workloads.

`step(state, opening) -> Continue(next_state) | Done` is conceptual notation.
It is not a supported `Step<State>` enum API. General enums and iterator
lowering are not admitted by this design. The prototype must first check a
concrete state/solution/output representation against the actual compiler.
Existing tuples, nominal wrappers, explicit `Cons` pairs and output-only
`Option<T>` have separate contracts; see [optional lists](https://judeallred.github.io/chialisp-oxide/reference/optional-lists/).

## First prototype: an ordered payout queue

Use a small finite queue with a fixed payout amount and one payout per spend.
This isolates the transition rules before testing a second explicit batch size.
The initial funding plan commits the complete ordered queue, its length and
budget, a terminal sentinel, and the immutable module and policy parameters.

For example, an item commitment can hash a domain tag, a fixed-width recipient
and the next queue digest. Any additional item fields need an unambiguous
encoding and domain separation. The final encoding, integer representation and
finite bounds must be specified and checked by an independent oracle before
implementation. SHA collision and preimage resistance are assumptions, not
properties proved by the compiler or finite vectors.

Each step must enforce all of these rules:

1. The supplied item and next digest open the current committed head. A caller
   cannot substitute, skip, reorder or inject an item.
2. The exact committed recipient receives the fixed payout. The current coin's
   amount matches the committed remaining budget, with checked arithmetic.
3. A nonterminal step creates exactly one continuation coin with the expected
   remaining funds and the same module and immutable policy parameters, curried
   with the derived next state. The caller cannot choose its puzzle hash.
4. The terminal state agrees with the committed count and sentinel. Early
   termination rejects, and a completed queue leaves no continuation coin.
5. The initial funding verifier checks that the immutable module-hash parameter
   really names the selected module artifact. Successors preserve that identity;
   an unchecked caller-supplied module hash would break the intended chain.

The concrete first experiment chooses no controller reserve, remainder or
abort branch: exactly 1,000 mojos per item is fully paid. A later reserve or
remainder policy must commit its destination and preserve fund conservation.
Continuation amounts, payouts and fee contributions need
separate names and units.

Permissionless advancement is a useful first policy because the queue fixes
every recipient and result. A third party can race or accelerate a valid step;
it cannot change the work. Consensus prevents spending the same coin twice.
These facts do not guarantee that anyone will advance the queue, that a spend
will be included, or that the chain is atomic across blocks.

Fee sponsorship belongs in the host wallet workflow. A spender must not turn
omitted continuation funds into a caller-selected fee. The first experiment can
omit abort and clearly disclose that funds remain committed until valid steps
are advanced. An abort/refund extension must commit its destination and deadline
before funding, define its interaction with outstanding work, and test both
sides of the exact height boundary. It must not add an informal owner drain.

## Existing pieces and unresolved features

The facade already has the reviewed SHA bridge, curry hash arithmetic, checked
fixed-width arithmetic, strict typed ABI, and named coin conditions. A plain
queue can derive a successor puzzle hash and emit `CREATE_COIN`; it need not
apply a dynamic program or wrap every step in a singleton. Those larger
protocols should be dependencies only when the chosen example needs them.

The [concrete ABI/hash spike](https://github.com/judeallred/chialisp-oxide/tree/main/experiments/authenticated-continuations)
fixes one payout at 1,000 mojos, a finite count of 1..=8, no reserve or abort,
fixed byte-domain queue hashes, and a one-parameter standard curry. It includes
original independent Chialisp source, retained both-mode/reference template VM
evidence, three actual SDK successor chains and separately counted source
controls. Only batch size one is measured.
Canonical state hashing, terminal shape, amount bounds and the initial module
binding require independent checks before claiming an SDK abstraction. No new compiler feature is assumed merely because its
native Rust spelling compiles.

## Required evidence before crediting the prototype

Keep these acceptance checks separate:

- Native Rust and independently compiled Chialisp, with both Rust CLVM modes,
  must agree on positive vectors and malformed, padded, oversized and incorrect
  openings. Include skipped, reordered and injected work, wrong next state or
  module, early termination, excess continuation, and arithmetic boundaries.
- A mined simulator chain must advance at least three actual successor coins,
  stop and resume, and reconcile a consumed coin. Submit negative bundles and
  check their intended typed consensus codes and full simulator equality.
  Resource and infrastructure failures remain inconclusive.
- Actual-source mutations must remove meaningful opening, next-state, terminal
  or amount checks. Each mutant funds its own reveal. Constructed evidence-file
  corruptions are a different control lane.
- Record each mode's serialized program bytes and primary VM cost. Measure
  complete generator charged bytes and total consensus cost for every accepted
  step and explicit batch size. Distinguish a selected batch fitting measured
  limits from a universal compiler cost guarantee. Retain total chain costs and
  fees separately from whole-block observations.
- CLI/GUI planning, submission, interruption recovery and monitoring must use
  the same checked recipe. This in-process continuation fixture is separate
  from the [wallet lifecycle](https://judeallred.github.io/chialisp-oxide/reference/lifecycle/); deployment through CLI/GUI on
  Testnet11 requires its own executed network integration.

## Host workflow and remaining boundaries

The host planner reconstructs every opening, budget and successor before
recording a funding parent. Recording that parent alone does not establish a
real coin. The separate HTTPS adapter checks the exact confirmed coin, repeats
SDK preflight at the provider's previous transaction block height, and persists
the exact unsigned zero-fee bundle before submission. Its normal submission
command never retries a started push automatically. A distinct explicit
recovery action can archive and retry the identical bundle once after fresh
reconciliation; mempool absence never proves an earlier submission failed.
Available response bytes, confirmation heights, actual reveals and complete
child sets are retained. Provider assertions are not chain authentication or
finality, and there is no automatic reorganization rollback or fee sponsorship.
The separate Testnet11 trial completed four provider-observed spends at heights
4,904,700, 4,904,715, 4,904,742 and 4,904,760, paying the committed 4,000 mojos
back to the test wallet with no final successor. Its four prepared bundles have
zero fees; funding and containing chain-bundle fees are not measured. Five HTTP
push attempts concern four bundle IDs because the first ambiguous attempt was
followed by one explicit identical-bundle retry. The missing first response
body remains an inconclusive evidence gap. The
[retained live packet](https://github.com/judeallred/chialisp-oxide/tree/main/experiments/authenticated-continuations/live-evidence)
is separate from every historical simulator count and does not authenticate
submission causality, chain inclusion or finality.

The separate [batch-two experiment](https://github.com/judeallred/chialisp-oxide/tree/main/experiments/continuation-batches)
retains its historical expansion-depth boundary receipt. The bounded
compiler-owned continuation-table retry now emits the unchanged real Cargo
export in both modes without raising compiler limits. Current successful VM,
SDK and cost evidence is captured separately from that historical receipt. Pairwise distinct recipients are an additional explicit policy
restriction, needed to avoid duplicate output coins within a batch. This is a
separate explicit policy, not a selectable batch size in the shared live workflow.

The concrete batch-one experiment records 45 base SDK submissions and a separate 16
actual-source-control submissions; runtime umbrellas remain inconclusive as
policy-rejection evidence. Its public cursor is persisted and reparsed within
one process while the simulator remains in memory. The new host journal and
adapter have separate process-restart and injected-provider controls; they do
not add SDK attempts to those historical counts. A reusable generic SDK API, reorganization recovery and actual Testnet11
execution through the new shared Rust adapters remain substantive work. The
new session never retries a started push automatically, including after a
missing response. Its simulator tests and local GUI checks do not add live
network credit to the historical Python trial.

The retained artifacts keep their original compiler and driver identities.
A separate `reader_revalidation` record captures fresh compatibility checks
against identical IR, program bytes and fixed native/VM/SDK observations.
An offline re-export of the completed trial updates exporter freshness while
preserving its raw observations, summary and artifact producer context; it adds
no live execution credit.

Source authentication and compiler-preservation credit do not follow. SHA remains
outside models that do not support it; termination/progress models, if added,
must remain separate from compiler preservation. Liveness, inclusion, finality,
cross-block atomicity and production safety are not guaranteed.
