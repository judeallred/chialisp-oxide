# Authenticated continuations: exploration design

**WIP design, not an implemented feature. Not ready for use or real funds.**
This records a proposed first experiment for
[#16](https://github.com/judeallred/chialisp-oxide/issues/16). The continuation
SDK type, demo export, measured costs and multi-step simulator evidence remain
unimplemented. The issue stays open.

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
with the [authoring guide](llms.md).

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
`Option<T>` have separate contracts; see [optional lists](optional-lists.md).

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

The exact treatment of a controller reserve and final remainder is a policy
decision still to implement. It must have a committed destination and preserve
fund conservation. Continuation amounts, payouts and fee contributions need
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

The next implementation task is a small concrete ABI and hashing spike, then a
demo with independent reference source. It must resolve canonical state hashing,
terminal output shape, amount bounds and exact module binding before claiming
an SDK abstraction. No new compiler feature is assumed merely because its
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
  the same checked recipe. The current [local lifecycle](lifecycle.md) is a
  simulator workflow; funded Testnet11 evidence requires its own executed
  network integration.

This design is a stopping point before implementing the prototype. It adds no
SDK-spend, live-network, source-authentication or proof credit. SHA remains
outside models that do not support it; termination/progress models, if added,
must remain separate from compiler preservation. Liveness, inclusion, finality,
cross-block atomicity and production safety are not guaranteed.
