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

## Pay-to-singleton component boundary

The canonical `p2_singleton` module is a separate `protocol_component` with
empty ordinary simulator scenario arrays. Its `protocol_validation` field
embeds five actual build groups and 110 transaction submissions: 15 accepted
and 95 exact typed consensus rejections with complete in-memory Simulator
rollback. These counts remain separate from 40 ordinary scenario definitions
and 120 language scenario runs, and from the password instance submissions.

The harness independently reproduces canonical launcher, singleton top layer
and StandardLayer artifacts. Each language uses its own compiled P2 module
and curried coin commitment. The top layer and StandardLayer are host
counterparts, not Rust frontend ports. A real Eve proof, a 1 + 2 → 3 funding
composition and one subsequent lineage proof are executed; this is not a
complete singleton lifecycle or a completed primitive/tutorial/CHIP migration.

The closed reader binds every runtime include/source/published artifact,
reconstructs curry trees and coin IDs, and checks exact named receipts and
consensus codes under `chia-consensus-0.36.1/p2-singleton-v1`. The producer
submits missing and wrong nonempty signatures to consensus; signer preflight
cannot supply rejection credit. The producer checks complete Simulator equality
in memory against its saved clone before recording a Boolean result. The report
does not serialize SDK state or include Debug payloads; a recorded flag is not
an independent proof of equality or authenticated execution.

Current22 source/native/typed/protocol evidence and explicit fresh ON/OFF
bundles are checked before the append projection restores the entire signed
21-puzzle cost baseline. Existing cost profiles and historical measurements
remain byte-identical. Read-only evidence validation does not reexecute those
transactions or authenticate an earlier run. SHA/raw-tree protocol logic
remains outside the scalar models and compiler-preservation claims.

## Password module and instance boundary

The canonical password tutorial has a separate finite evidence contract. Its
original complete module accepts a raw environment and lets the caller supply
the expected hash. Standard-curried instances bind that hash before the coin is
created. The five module builds and their instances retain separate identities;
the host constructor quotes both the module and bound digest rather than
executing either while constructing the tree.

`scripts/check-password-coin.py` independently hashes exact atom bytes, selects
the original fields and checks unchanged condition-tree output. It reconstructs
the curry trees, serialization and hashes without calling the Rust builder.
Source, native closure and complete current Cargo provenance are checked before
historical projection. The cost transition first binds all 21 puzzles to the
signed before report and reviewed new optimized artifacts. Only reviewed Rust
optimized slots and independently derived instance/coin identities may change;
all other recorded fields remain exact. Fresh Cargo bundles check all
22 current raw IR programs and both modes against their reviewed artifacts
before historical projection. The append gate retains the signed 21-program
cost baseline.
This is freshness and finite agreement, not compiler authentication or a
translation theorem. Existing formal models reject SHA operations.

The next closed cost transition binds signed commit `c87957e2` and the unchanged
helper-inlining snapshot in `corpus/runtime-refinement-baseline.json`.
Branch-local integer intervals remove a proven-total repeated checked
initializer; lazy byte bounds compact the canonical integer decoder. All 21
raw IR programs, unoptimized artifacts, native outcomes and other-language
observations stay fixed through the reviewed projection. Current optimized
measurements are finite size/primary-VM observations, not universal cost
dominance or complete transaction-generator costs.

The ledger experiment submits seven transactions per language from separately
initialized simulator states. Intended payments and unsigned retargeting both
succeed; the wrong-instance reveal passes its own primary VM check before
failing the original coin's committed puzzle hash. Dedicated consensus failures
must have their expected codes and preserve the complete simulator state.
Wrong-password `GeneratorRuntimeError` observations remain broad runtime
umbrellas without semantic rejection credit. Low-cost resource observations
are inconclusive and separate from ordinary passing cases.

```sh
python3 scripts/check-password-coin.py --self-test --replay --extract-ir
python3 scripts/check-cost-optimization.py --self-test --replay
python3 scripts/check-runtime-refinement.py --self-test --replay
```

Run this after Rust builds and tests finish. It replays retained primary-VM
bytes and extracts fresh Cargo bundles in both modes, without rebuilding the
compiler tools or making new consensus submissions.
Regenerate the complete report with the actual CLI to repeat ledger validation.
The [walkthrough](https://judeallred.github.io/chialisp-oxide/tutorials/password-coin/)
explains why these successful checks still leave the password policy unsafe.

## Canonical integer encoding law

`proofs/integer-canonicality.smt2` models arbitrary finite unsigned byte
sequences. A minimal signed integer atom is nil or lies in unsigned
lexicographic `[0080, ff80)`. The retained report records 14 baseline queries
and nine queries for three actual model-source mutations, including positive
and negative-domain witnesses. The negated encoding equivalence is UNSAT;
changed bounds and a removed nil case have satisfying distinguishing inputs.
The 19 solver-protocol controls reject missing, extra, unknown or contradictory
statuses. No unknown or infrastructure failure establishes a proof.

The law's decoder-source binding is metadata freshness only: this model is not
translated from Rust, IR or LIR. It proves no actual opcode, environment ABI,
integer-width admission, native/VM bridge, execution cost or compiler-preservation
theorem. Finite decoder tests separately exercise every zero-, one- and two-byte
atom, all supported widths, longer padding and ordering against divergence.

```sh
# Run the actual pinned-version solver and compare retained source/results.
python3 scripts/check-integer-canonicality.py --check --self-test
# Offline documentation freshness only; no solver execution or new proof credit.
python3 scripts/check-integer-canonicality.py --check-recorded
```

Documentation builds use the second command. The compiler verification job
executes the first. Both check exact source/query/status metadata; source hashes
and the installed version-matching solver remain trusted inputs rather than
authenticated compiler or solver binaries.

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
operations. Default scalar APIs reject runtime IR; the separately selected
acyclic scalar model below admits bounded nonrecursive scalar tables. These checks
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

Default scalar, structural-tree and byte/Boolean SMT APIs reject every runtime
program, including unused function tables and runtime nodes in dead branches.
The explicit acyclic scalar model below is a separate, narrower admission path;
it also rejects recursive graphs. These models do not map divergence
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

## Explicit acyclic scalar model

`--acyclic-scalar` selects a separate model for checked integers, Booleans,
unit and tuples with nonrecursive function tables. The Rust APIs are
`emit_acyclic_scalar` and `verify_acyclic_scalar`; the existing default APIs
retain their runtime rejection boundary. Whole-IR validation precedes model
admission, including unused functions and dead branches. The model bounds
tables to 16 functions and also limits nodes, depth, types, work and emitted
text. A compiler-admitted program can exceed those narrower model limits.

Each function is summarized once in callee-first order. Arguments evaluate
strictly in source order, unused initializers still contribute their success
condition, and each callee starts a fresh argument/local scope. A copy reads
the captured value rather than evaluating its initializer again. Lazy branches
and checked integer width/overflow remain explicit. This models acceptance
and successful output, not the order or code of failures.

With the workspace tools built and Z3 installed, the retained counter provides
a small CLI example:

```sh
target/debug/chialisp-oxide-verifier proofs/bounded-counter/program.ir.json \
  --acyclic-scalar proofs/bounded-counter/acceptance.spec.json
target/debug/chialisp-oxide-verifier proofs/bounded-counter/program.ir.json \
  --acyclic-scalar proofs/bounded-counter/acceptance.spec.json --emit
```

The first command solves the nonvacuity and counterexample obligations. The
second prints their SMT queries for inspection. This particular retained
counter has no runtime table; it is a CLI compatibility example, not evidence
that a new source-to-table translation is proved. To inspect a newly extracted
Cargo export, supply its exact IR and an independent specification instead.

The domain starts **after typed ABI decoding**. It does not establish acceptance
of arbitrary malformed environments or prove the decoder. Recursive calls,
SHA, byte/atom/raw-tree values and dynamic application remain unsupported,
including in unused functions. Solver failure or model-budget exhaustion
provides no proof. Model regression tests and finite native/VM comparisons
remain distinct from Rust/MIR/IR/CLVM preservation, termination, resource-cost,
cryptographic or ledger proofs.

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
