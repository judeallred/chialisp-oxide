# Verification strategy

Current evidence compares Chialisp, Rust and Rue sources through five compiled
modes, native policy execution, generated inputs and complete-spend scenarios.
The corpus distinguishes original teaching fixtures, exact canonical Intro
expression ports, and the canonical raw-tree `p2_conditions` program factory.
The published `last_inner`, `prefix`, and byte `!=` helpers use separate explicit module adapters,
with exact pinned source-span/license provenance and no whole-module identity
claim. Empty-slice matching tests distinguish nil from nonempty zero-byte atoms.
Byte-equality tests additionally preserve padding, invalid UTF-8 and lengths
through 8,192 bytes. Concrete standard trait resolution precedes logical type
erasure; custom equality implementations do not gain primitive semantics.
Hand-authored structural Seq8 regressions distinguish padded comparison literals
with a refuted mutation and VM witness replay. A separate source-derived
Boolean proof now checks the actual Cargo export; see the byte-policy section
below. The independent helper gate additionally refutes numeric-normalization
mutants. None proves rustc/MIR/IR/CLVM preservation or constant-time behavior.
The factory has module-identity and second-stage application checks, rather than
direct ledger admission claims. The validation, scalar, structural-tree and
byte-policy reports in `corpus/` record their actual inputs and scope. The early
backend-only experiment below remains a separate baseline.

## Solver environment

CI uses the official [Z3 4.15.4 release](https://github.com/Z3Prover/z3/releases/tag/z3-4.15.4).
The compiler job runs on Ubuntu 24.04 and verifies the SHA-256 of the selected
`x64-glibc-2.39` archive before installing its CLI. The exact URL, digest and
installation commands are in [the verification workflow](../.github/workflows/check.yml).
For local use, install that release for your platform and put `z3` on `PATH`;
`z3 --version` should report 4.15.4. macOS package-manager versions can change,
so check the actual version when reproducing a result.

Ubuntu 24.04's distribution package supplies Z3 4.8.12. On the checked-addition
and bitvector-mask regression it returned `unknown` within the unchanged
five-second solver budget; the selected 4.15.4 Linux release solves the same
query. The original nonvacuity obligations and two refuted arithmetic/mask
mutants were also replayed without changing their formulas or deadlines.

Other solver versions can still be selected through `SolverConfig`, but
completeness and performance are not promised across versions or machines.
An absent solver, `unknown`, a timeout or malformed output produces no proof.
The release pin does not turn these outcomes into success or increase the
solver budget. To reproduce the regression:

```sh
cargo test -p chialisp-oxide-verifier --test bitwise_inputs --locked
```

## Source-derived whole-environment byte proof

[`scripts/check-byte-proofs.py`](../scripts/check-byte-proofs.py) extracts
`atom-not-equal` from the actual Cargo library into fresh, identity-checked
bundles. An independent specification accepts exactly `(A B)` with two atom
operands and an empty final tail, and returns canonical Boolean inequality.
Atoms are symbolic sequences of 8-bit values: nil, padded zeros, invalid UTF-8
and arbitrary other bytes stay distinct. Both acceptance and output are
specified over the entire environment, rather than inferred from the source
parameter declaration. This is an IR-model proof of that modeled contract,
including its modeled strict decoder, not a proof of the emitted ABI decoder.

The eight models comprise the original Cargo export, five actual source
controls, and two explicitly synthetic ABI edits. Four source changes
(operator inversion, self-comparison and the two constants) are refuted;
operand reversal is a positive equivalence control. The synthetic controls
permit extra/improper outer tails or narrow both operands to two bytes. They
are not represented as source-derived Rust policies. The exact source patches,
IR, independent specification and SMT queries are retained. Earlier manually
authored byte-IR regressions remain separate.

ABI and independent-acceptance nonvacuity, plus specified true/false witnesses,
prevent empty-domain proofs. Reproducible candidate witnesses are solver-checked
after the unbounded obligation is solved; they do not bound the proof domain.
Each model has optimization-off/on artifacts and selected witness replays,
including execution of the exact retained serialized programs. The original
policy additionally replays witnesses in all five Chialisp/Rust/Rue builds.
Actual exported native functions supply source-control results, without copied
policy implementations. Resource/infrastructure failures abort the evidence.

```sh
python3 scripts/check-byte-proofs.py --output corpus/byte-proofs.json
python3 scripts/check-byte-proofs.py --check
python3 scripts/check-byte-proofs.py --self-test
```

The first command builds, extracts, solves and replays. `--check` validates
recorded source/artifact freshness and consistency without rerunning those
tools. See the [step-by-step guide](https://judeallred.github.io/chialisp-oxide/verification/byte-proofs/).
The installed pinned compiler, standard library and primitive bridge remain
trusted; [target configuration and loaded-library checks](toolchain-trust.md)
enforce the supported installation without authenticating its bytes. This
layer establishes no compiler preservation, constant-time behavior, ledger
property, whole-library verification or full Chialisp parity.

## Fixed-width integer masks

Actual built-in Rust `&`, `|` and `^` preserve the supported signed/unsigned
integer width; `!` becomes XOR with the unsigned maximum or signed -1. Boolean
`&`/`|`/`^` are strict, with both operands evaluated. These use existing typed
binary IR, not an SDK symbol convention. Driver and IR regression tests cover
native results, width/sign boundaries, malformed ABI values, optimization modes,
strict failure/divergence, recursive calls and source snapshots.

For programs containing integer bitwise operations, the scalar verifier declares
finite-width bitvector inputs and derives their exact signed/unsigned integer
interpretations. Every integer in the declared type has one such input word.
Bitwise results retain their typed word representation; checked arithmetic stays
mathematical and converts exactly at subsequent bitwise uses.
Boolean operations retain the success conditions of both operands. Independent
identity specifications, deliberately incorrect masks and replayed
counterexamples check the model. Current byte/tree models still reject integer
operations, and runtime IR stays outside all SMT proof domains. These checks
are not a proof that Rust extraction or CLVM code generation preserves the
model, and they establish no cryptographic property.

```sh
cargo test --locked -p chialisp-oxide-driver --test bitwise
cargo test --locked -p chialisp-oxide-ir --test bitwise
cargo test --locked -p chialisp-oxide-verifier --test bitwise
```

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

All current SMT model APIs reject every runtime program, including unused
function tables and runtime nodes in dead branches. They cannot map divergence
into their value/rejection domain. Execution-only replay is still possible but
aborts on resource or infrastructure errors. The typed-IR pair-guard pass also
preserves runtime programs unchanged: a later projection cannot justify moving
an earlier rejection past a potentially diverging call. Optimized runtime
lowering separately forwards total leaves, lowers single-consumer operations
directly, and removes tail identity bindings without dropping their initializer.
Non-leaf multi-operand expressions retain ordered, once-only continuations.
Before recursive operand lowering, frame, pending-operand, direct-depth and work
budgets prevent shallow wide IR from causing unbounded continuation stacks or
cloned type storage. See
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
switches are held constant across the Rue pair. This early experiment retains an
assembled CLVM reference. The paired puzzle corpus separately compiles actual
Chialisp source; its evidence should be used for Chialisp-to-Rust comparisons.

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

The pinned Lean 4.35.0 [finite-tree experiment](../experiments/tree-theorems) now
proves 21 named statements for manually authored prefix/last-element algorithms
over all finite trees. Independent expected types prevent weakening those
claims, and exact axiom audits reject unreviewed assumptions. A finite bridge
connects named model equalities to all five recorded compiler builds; optional
VM replay checks the actual retained bytes, costs and failure codes. CI uses a
checksum-pinned official Lean distribution and runs the kernel, corruption
controls and VM replay.

This is an algorithm-model proof plus finite execution evidence. It does not
extract Rust or prove MIR/IR/CLVM preservation, native allocation behavior or
resource equivalence. The existing source-derived SMT verifier remains a
separate layer and deliberately rejects recursive runtime IR.

## Layers still to build

- Coverage-guided fuzzing for decoding and lowering, including very large atoms
  and deeply nested trees beyond the bounded property generator.
- Extend the implemented rustc frontend and compile-fail regressions to richer
  safe Rust and domain SDK types.
- Expand the implemented independent Chialisp compiler bridge to canonical
  tutorial and CHIP puzzle vectors.
- Expand the implemented chia-wallet-sdk simulator tests to more stateful and
  adversarial cross-puzzle compositions.
- Broader mutation testing of compiler-generated programs and IR passes. Current
  controls include actual Cargo byte-policy source changes, explicit synthetic
  ABI edits, factory IR rewrites and the earlier handwritten CLVM baseline.
- Per-puzzle properties: allowed recipients, conservation, authorized transitions,
  lineage, replay resistance, and escape/recovery behavior as applicable.
- Extend the implemented scalar, structural-tree and strict-byte/Boolean SMT
  checks to broader byte operations, cryptographic assumptions, state
  transitions and compiler-preservation proofs.

Each report must distinguish executed cases, bounded exploration, model proofs,
and end-to-end proofs. None supports an unqualified “no security breaches” claim.
