# Verification strategy

Current evidence compares Chialisp, Rust and Rue sources through five compiled
modes, native policy execution, generated inputs and complete-spend scenarios.
The corpus distinguishes original teaching fixtures, exact canonical Intro
expression ports, and the canonical raw-tree `p2_conditions` program factory.
The published `last_inner` list helper is a separate explicit module adapter,
with exact pinned source-span/license provenance and no whole-module identity
claim. Empty-slice matching tests distinguish nil from nonempty zero-byte atoms.
The factory has module-identity and second-stage application checks, rather than
direct ledger admission claims. See `corpus/validation.json`, `corpus/proofs.json`
and `corpus/tree-proofs.json` for the actual counts, inputs and scope. The early
backend-only experiment below remains a separate baseline.

## Runtime calls and divergence

Version-4 IR adds compiler-owned function tables and strict typed lexical
bindings. Real Cargo Rust tests cover self/mutual and non-tail calls, argument
order, unused results, caller/callee scopes, borrow checking and unsupported
MIR rejection. The `rightmost-tree` corpus example compares the actual Rust
function with independent Chialisp and Rue on bounded native inputs. Its
right-tail depth cap is a host-oracle error beyond 128, not policy rejection;
the compiled function adds no depth guard. Separate VM tests reach depth 2,048.
The `last-list-element` helper adds proper-list, improper-tail, empty/padded
atom and returned-subtree cases across all three languages. Its native traversal
bound is likewise an oracle error, never a policy-rejection observation.

Ordering controls require `raise` when rejection precedes a nonterminating
call, and `cost_exceeded` when nontermination comes first. Nontermination runs
only in the VM. A measured-cost control distinguishes one bound recursive
result read twice from two deliberate recursive evaluations. These are finite
execution witnesses, not universal termination, resource-equivalence or
compiler-preservation proofs.

Both current model translators reject every runtime program, including unused
function tables and runtime nodes in dead branches. They cannot map divergence
into their value/rejection domain. Execution-only replay is still possible but
aborts on resource or infrastructure errors. The typed-IR pair-guard pass also
preserves runtime programs unchanged: a later projection cannot justify moving
an earlier rejection past a potentially diverging call. Before recursive
operand lowering, active-binding and work budgets prevent shallow wide IR from
causing unbounded continuation stacks or cloned type storage. See
[runtime calls](runtime-calls.md) for implementation and durable test paths.

## VM failures are typed evidence

The shared `chialisp-oxide-vm` crate classifies each `clvmr 0.21.0` error using
its Rust variant. Its checked-in `failure-protocol.json` is the exhaustive wire
contract (`clvmr-0.21.0/v1`), also consumed by the Python and documentation gates.
Human-readable diagnostics are retained for inspection and never determine the
classification.

- `program` failures produce `rejected`: VM-semantic failures such as `raise`,
  `path_into_atom`, or `division_by_zero`. This category does not establish that
  an intended policy assertion rejected the input.
- `resource` failures, including `cost_exceeded` and VM stack/allocator limits,
  produce `inconclusive`.
- `infrastructure` failures, including serialization and internal errors, also
  produce `inconclusive`.

Every failed VM execution carries an exact `{kind, code}` record; successful
executions omit that field. Unknown codes, inconsistent categories, missing
metadata and inconclusive runs fail the current evidence gates. Native policy
rejection has its own outcome shape and makes no VM-resource claim.

Proof witness replay accepts only program failures as rejection; a resource or
infrastructure failure aborts replay instead of supplying evidence for the
model. The scalar guard-removal witness additionally requires exact code
`raise`; a diagnostic merely containing that word is insufficient. Structural
projection witnesses may reject with other program codes. Neither replay nor
the IR model proves equal costs, equal resource bounds or termination of all
compiled source programs.

Historical comparison baselines may predate typed metadata. That explicit
compatibility path applies only to the old report: every current execution is
validated before comparison projections can discard diagnostic fields.

## Consensus rejection is a separate boundary

The simulator uses `chia-consensus 0.36.1`, `clvmr 0.16.2` and TESTNET11 rules.
Its protocol is `chia-consensus-0.36.1/v1`, separate from the primary VM failure
protocol above. Every ledger scenario records an ordered `rejections` list with
exact `{stage: "consensus", code: "..."}` observations. Positive scenarios have
an empty list. The closed `rejection-protocol.json` binds each scenario name to
its expected codes, including both checks in the grouped recovery scenario.
Per-fixture ordered lists require every declared coin scenario, so agreement
between three incomplete language records cannot hide an omitted check.

Missing, partial and incorrect signatures must reach `new_transaction` as real
spend bundles and receive `BadAggregateSignature`. A signer-side SDK
`MissingKey`, generic runtime error or unrelated consensus error cannot count as
the expected ledger rejection. Gates check all language records and preserve
exact equality between them. Scenario-run counts describe grouped tests, not
the number of submitted transactions; rejection-check counts are separately
derived from the recorded observations. Neither protocol establishes mainnet
admission outside the simulator's selected rules.

Strong testability is a product goal. Puzzle authors should be able to run their
logic natively, exercise the exact generated bytecode, specify security
properties, and get reproducible counterexamples when those properties fail.

## Implemented experiment

The capped-payment backend experiment accepts exactly one integer atom in a
proper list, between 0 and 1000 inclusive, then returns one CREATE_COIN condition
with the submitted amount and a fixed 32-byte destination. Integer encodings may
be padded; output preserves the submitted bytes. This deliberately small policy
has no authorization and is not suitable for locking funds.

Five backends are compared: handwritten reference CLVM, Rue source with LIR
optimization off/on, and direct construction of Rue LIR with optimization off/on.
The native Rust numeric policy independently supplies expected acceptance, and
expected serialized conditions check the complete output. The other optimization
switches are held constant across the Rue pair. The reference is assembled CLVM,
not a Chialisp compilation; integrating the Chialisp compiler remains work to do.

Tests include exhaustive small integer coverage, random signed 64-bit inputs,
random valid amounts, bounded-depth arbitrary solution trees, malformed list shapes, huge signed integers, noncanonical
integer atoms, and explicit cost exhaustion. All runs use clvmr 0.21.0,
MEMPOOL_MODE, and a one-million-cost limit unless testing cost exhaustion.
VM failures carry the typed classification above. Cost exhaustion is an
inconclusive resource result and fails ordinary validation; explicit limit
controls must check `resource/cost_exceeded` rather than count it as rejection.

```sh
cargo test --workspace --locked
cargo run --locked -p chialisp-oxide-backend-experiment > /tmp/chialisp-oxide-backend-report.json
python3 scripts/check-proofs.py
```

The JSON report includes bytecode, disassembly, tree hashes, serialized sizes,
per-vector execution costs, outputs, and errors. Runtime costs exclude chain-level
byte costs, condition costs, and signature verification. Rejection equivalence is
not equality of error messages or the cost at which rejection occurs.

## First formal experiment: unbounded integer policy

`proofs/capped-payment.smt2` uses quantifier-free linear integer arithmetic in
[Z3](https://microsoft.github.io/z3guide/docs/logic/Quantifiers/) to search for a
counterexample to the range predicate's lowered form. It checks equivalence and
the accepted range; both should be unsatisfiable. A mutation removes the upper
bound and must be satisfiable, exposing amount 1001 or another invalid amount.
The runner fails on unknown, timeout, errors, and missing mutation detection.

This is a model-level result. The model is currently handwritten and does not
prove the Rust implementation, Rue optimizer, CLVM semantics, cryptography, or
transaction validity. Differential execution tests provide a separate connection
for tested inputs only. A future verifier should derive obligations from the
same typed IR as code generation and independently check their meaning.

## Where Lean fits

[Lean](https://lean-lang.org/theorem_proving_in_lean4/) is a theorem prover, rather
than a general automatic security scanner. It is a candidate for definitions of
the pure IR and its semantics, proofs of individual lowering/optimization rules,
and puzzle state-transition invariants. Do not introduce an unconnected model
and call the compiled puzzle verified. Required links are:

1. A precise puzzle specification and explicit attacker capabilities.
2. A semantics for the supported IR and puzzle ABI.
3. Evidence that extraction from checked Rust preserves those semantics.
4. Proofs or checked translation validation for lowering into CLVM, including
   failure behavior, integer encoding, and branch evaluation.
5. Explicit assumptions about hashing, signatures, consensus rules, and the VM.

No Lean toolchain or Lean proof is included yet. The chialisp-oxide-verifier crate now
generates obligations from actual typed IR and independently supplied specs,
checks nonvacuity, and replays attack witnesses through source/native and CLVM. Start with narrow SMT obligations
and tested counterexamples while designing this connection; evaluate Lean when
we have stable IR rules worth proving.

## Layers still to build

- Coverage-guided fuzzing for decoding and lowering, including very large atoms
  and deeply nested trees beyond the bounded property generator.
- Extend the implemented rustc frontend and compile-fail regressions to richer
  safe Rust and domain SDK types.
- Expand the implemented independent Chialisp compiler bridge to canonical
  tutorial and CHIP puzzle vectors.
- Expand the implemented chia-wallet-sdk simulator tests to more stateful and
  adversarial cross-puzzle compositions.
- Mutation testing of compiler-generated programs and IR passes. The current
  executable mutation control weakens the handwritten CLVM reference.
- Per-puzzle properties: allowed recipients, conservation, authorized transitions,
  lineage, replay resistance, and escape/recovery behavior as applicable.
- Extend the implemented IR-derived SMT checks beyond scalar/tuple values to
  bytes, cryptographic assumptions, and compiler-preservation proofs.

Each report must distinguish executed cases, bounded exploration, model proofs,
and end-to-end proofs. None supports an unqualified “no security breaches” claim.
