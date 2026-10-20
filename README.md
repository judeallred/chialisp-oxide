# chialisp-oxide

> **WIP PROTOTYPE — NOT READY FOR USE.** This is an experimental compiler and
> validation research project. **Do not use it for real funds, production
> puzzles, wallets, or deployed contracts.** Tests and model-level checks do not
> establish production safety.

Compile a supported subset of real Rust into CLVM using Rue's optimizer and
backend. Test puzzles against native Rust, independently compiled Chialisp, and
the chia-wallet-sdk transaction simulator.

## Explore the Cargo prototype

Use an ordinary Rust library with the real, unpublished `chialisp-oxide` dependency.
Mark exported functions with `#[chialisp_oxide::puzzle]`. Ordinary `cargo check` and
`cargo test` provide Rust checking and native tests; `cargo chialisp-oxide check` and
`cargo chialisp-oxide build` additionally validate the reachable CLVM subset and compile
the marked functions through Rue.

From this checkout, with rustup installed:

```sh
rustup show
cargo build --locked -p cargo-chialisp-oxide -p chialisp-oxide-driver
export PATH="$PWD/target/debug:$PATH"
cargo check --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo test --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo chialisp-oxide build --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo chialisp-oxide check --manifest-path examples/cargo-puzzles/Cargo.toml --features strict-cap --locked
```

The [consumer package](examples/cargo-puzzles) has two annotated entries, a real
SDK path dependency, feature-controlled limits, a host build script generating
code through `OUT_DIR`/`include!`, and unannotated native-only allocation code.
Neither `chialisp-oxide`, `chialisp-oxide-macros`, nor `cargo-chialisp-oxide` is published to crates.io.
Use the checkout's path dependencies or a reviewed pinned Git revision. Local
installation and output options are documented in [cargo-chialisp-oxide](crates/cargo-chialisp-oxide/README.md).
The package name is `chialisp-oxide`, its Rust import is `chialisp_oxide`, and
the Cargo command is `cargo chialisp-oxide`; see the
[naming rationale](docs/rust-authoring-research.md#package-import-and-command-names).

The compiler is pinned to **nightly-2026-07-14**, reporting Rust 1.99.0-nightly,
with `rustc-dev` and `llvm-tools-preview`. No `RUSTC_BOOTSTRAP` is required.
Cargo supplies features, dependencies, generated code, and actual rustc
arguments. Build scripts and procedural macros remain normal host programs.
The installed pinned compiler and matching unmodified standard library are
trusted. Target configuration and actual loaded core/alloc/std and primitive
owners must use the pinned installation. These are location checks, not binary
authentication; see [toolchain trust](docs/toolchain-trust.md).

Each successful export build creates an immutable bundle under the consumer
workspace's `target/chialisp-oxide/artifacts/<bundle>/manifest.json`, with typed IR,
serialized CLVM, disassembly, sizes, and tree hashes. `latest.json` points to the
most recent successful bundle. Failed builds preserve the previous successful
bundle instead of publishing partial artifacts.

This is a Cargo-driven MIR extractor and CLVM backend adapter, **not** a rustc
`--target clvm` implementation. A target configuration alone does not provide
code generation, runtime layouts, linking, or a CLVM SDK. See the
[authoring research and backend rationale](docs/rust-authoring-research.md).
General cross-crate Rust libraries remain outside the supported subset; the
external calls are restricted to vetted concrete SDK helpers and one reviewed
native SHA-256 implementation. The hash bridge checks the actual signature,
complete typed-MIR closure and pinned target/profile fingerprint; names alone
do not authorize it. See [hashing and its trust boundary](docs/hashing.md).

## Build and validate the Cargo corpus

The fixtures are modules of the workspace's `chialisp-oxide-puzzles` library.
Every entry uses the real SDK's puzzle annotation. Native tests,
the differential oracle, and CLVM extraction use that same library:

```sh
cargo test --manifest-path puzzles/Cargo.toml --locked
cargo chialisp-oxide build --manifest-path puzzles/Cargo.toml --locked
cargo build --workspace --locked
./scripts/chialisp-oxide validate --output corpus/validation.json
```

Follow `target/chialisp-oxide/artifacts/latest.json` to the bundle manifest, then select
a full entry name such as `chialisp_oxide_puzzles::capped_payment::puzzle` and its artifact
paths. [puzzles/authoring.json](puzzles/authoring.json) declares Cargo inputs and
entry/source mappings; both validation and proof reports include their hashes.
The site checks those records against current files. This declared input set is
not a complete transitive dependency fingerprint.

Standalone file extraction remains a compiler-regression and research tool;
it is no longer the corpus's authoring or native-oracle route.

Z3 is required for the formal experiments. CI uses checksum-pinned Z3 4.15.4;
see the [solver setup and limitations](docs/verification.md#solver-environment).
The backend directly integrates Rue compiler
source; generated Rue text is optional.

## What is validated

The locally validated [corpus report](corpus/validation.json) records thirty-one
Chialisp/Rust/Rue source comparisons: ten original teaching fixtures covering
payments, timelocks, signatures, recovery, a boolean gate, a bounded counter,
and recursive raw-tree traversal;
plus three canonical closed expressions from the pinned Chialisp Intro, the
three pinned Using Modules programs (add, multiply by a named constant, and
recursive factorial) through declared signed 64-bit typed adapters, an
original delegated-puzzle fixture that applies an untrusted inner program
through the reviewed `apply` bridge, a curried-puzzle-hash fixture checked
against the pinned upstream curry include, the canonical Currying page module
with its page-argument instances executed in five builds, a compiled curry
factory whose returned programs are applied in a second VM execution, an
inner-puzzle commitment fixture whose curried instances run only the committed
program, the
exact canonical `p2_conditions` program factory, pinned `last_inner`,
`prefix`, byte `!=`, insertion-sort-by-key and `sha256tree` helpers through visible
module adapters, the singleton launcher module, and the exact password tutorial
module with separate curried-instance observations, plus canonical pay-to-singleton
with dedicated counterpart composition evidence, and canonical delegated
conditions with owner-key and signature/reveal checks. The
Intro ports preserve the exact original expressions and ignored environments
through explicit parameterless annotations. The Using Modules ports compile the
original modules unchanged and declare, per case, every result where the
original's unbounded positional reading differs from the typed Rust adapter;
those declarations bind only Chialisp and Rue, never Rust CLVM. The corpus
checks native execution,
587 explicit vectors and 25,222 generated integer/tree/byte cases against five CLVM
builds: Chialisp, Rust off/on, and independently compiled Rue off/on. Forty
complete-spend scenario groups run separately in all three languages (120 ledger
scenario runs). The eight condition-producing teaching fixtures and the singleton
launcher have ledger scenarios; pure language results and program
factories are not direct spends. Factory validation separately executes the
returned program on four environments for every successful named compiler run.
The raw password module is also a pure-function comparison: its caller supplies
the expected hash. Separate standard-curried instances fix that hash and record
48 case groups across five builds, three varying-hash controls and 21 actual
transaction submissions. Six spends succeed, including unsigned destination
changes; twelve fail with dedicated consensus codes. Three wrong-password
`GeneratorRuntimeError` observations receive no semantic rejection credit.
This deliberately insecure tutorial is a lesson in authority boundaries, not a
password wallet. See the [walkthrough](https://judeallred.github.io/chialisp-oxide/tutorials/password-coin/).
The separate canonical pay-to-singleton port is a `protocol_component`. Its
raw environment preserves the original arbitrary atom widths, launcher trees
and ignored tail. Five builds use their own committed module and execute
110 canonical-counterpart transaction submissions: 15 accepted and 95 exact
typed consensus rejections with complete Simulator rollback. These do not add
to the ordinary 40 scenario definitions or 120 language scenario runs. The
canonical singleton top layer and signed StandardLayer are host counterparts,
not Rust ports; a real Eve spend and one later lineage transition do not
establish a complete singleton lifecycle. See the
[pay-to-singleton walkthrough](https://judeallred.github.io/chialisp-oxide/tutorials/p2-singleton/).

Canonical delegated conditions is a second `protocol_component`. Its explicit
`Cons<Head, Tail>` result owns a computed digest while preserving an arbitrary
borrowed conditions tail; ordinary Rust tuples keep their proper-list ABI.
The dedicated raw suite checks 200 native inputs and 1,000 VM evaluations across
five builds. A separate retained-artifact replay repeats those 1,000 evaluations
without adding unique inputs. The ownership harness submits 60 actual SDK
transactions: 20 accepted and 40 exact typed rejections with full simulator
rollback. Accepted controls also demonstrate why an uncurried caller-selected
key is unsafe. These counts are separate from singleton, password and generic
scenario counts. Optimized Rust is 629 bytes, versus 137 for canonical Chialisp
and 251 for optimized Rue; their hashes differ. One native test group with
three policy observations and 12 VM source-control runs exposes mistakes that
still compile. See the
[delegated-conditions lesson](https://judeallred.github.io/chialisp-oxide/tutorials/delegated-conditions/).

The retained unsigned source-only checkpoint reduces the optimized Rust module from 1,972
to 1,685 bytes. In the recorded accepted joint transaction, total conditions
cost falls from 48,752,720 to 45,309,974 (7.06%). Ordinary VM execution increases
from 63,896 to 65,150; the saving comes from 287 fewer charged generator bytes,
not faster execution. The other 21 puzzles and independent Chialisp/Rue builds
remain unchanged. See the [transaction-cost lesson](https://judeallred.github.io/chialisp-oxide/tutorials/transaction-costs/)
for all five builds, three accepted phases and exact retained captures.

The completed compiler composition combines guarded projection forwarding,
exact atom facts and unary runtime-frame lowering. It reduces that same source
from 1,685 to 1,334 bytes, ordinary VM cost from 65,150 to 57,925, and the measured
joint transaction cost from 45,309,974 to 41,090,749. Across its 22-puzzle capture,
seven optimized modules shrink and fifteen stay exact. Compared with the earlier
guarded-only experiment, 194 accepted named observations cost less and 93 stay
equal; none costs more. Successful-operation facts reuse immutable values only
where earlier execution establishes the required shape. The passes retain
strict failures, fresh callee scopes, bounded analysis and a fallback to the
preceding emitter. These are finite measurements, not a general cost or
compiler-correctness theorem. See the earlier
[guarded-projection lesson](https://judeallred.github.io/chialisp-oxide/tutorials/guarded-projections/).


Some examples are deliberately permissionless. The boolean gate is not
identity/authentication. See each puzzle's security scope before using it.

The `sha256tree` helper and pay-to-singleton component use an explicit ten-million
VM-cost comparison budget in all five builds; other examples retain one million.
The hash helper’s five deep-input
controls also produce 25 separate observations at one million, including typed
cost exhaustion. The password module adds three inputs across five builds at
a lower budget, giving 40 separate diagnostic observations in total. These
controls do not add passing cases or policy rejections to the totals above.
The preservation gates retain the historical reports and their migration
boundaries. The cost iteration separately permits reviewed optimized Rust
changes while keeping source IR, unoptimized artifacts, other languages and
semantic observations fixed. Resource controls remain separate from coverage.

The optimizer now folds checked constants, propagates facts after successful
execution, refines branch-local integer bounds, inlines bounded helpers, and
removes redundant runtime frames and constructor checks. A compact integer
decoder retains complete shape, canonicality and numeric-range validation. All
three closed Intro expressions reproduce the original Chialisp bytes and
20-unit VM cost. The [optimization guide](https://judeallred.github.io/chialisp-oxide/reference/optimization/)
shows retained before/after measurements for the 21-puzzle cost baseline and explains the
strict-evaluation boundary. Runtime puzzles still have substantial cost gaps.

The [complete generator benchmark](experiments/spend-costs) executes 35 accepted
bundles in five builds using both uncompressed and backreference encodings,
with real signature verification, 140 typed signature controls and 70 exact
cost-budget pairs. The ordinary delegated owner spend costs 9,025,107 units in
optimized Rust versus 6,530,606 in Chialisp with backreferences; almost all of
that gap is the 500-versus-293-byte generator size. The
[generator lesson](https://judeallred.github.io/chialisp-oxide/tutorials/generator-costs/)
distinguishes these measurements from the signature-skipping conditions helper,
raw VM costs, fee quotes and full ledger admission.

The preceding cost milestone passed 502 workspace tests and all-target Clippy
with warnings denied on macOS. The hash evidence gate passes 168 corruption controls and
22,285 executions of the retained hash-port artifacts. The source-derived proof,
error-study, Lean and documentation gates are checked separately; these finite
checks do not establish compiler preservation or production readiness.

A 578-test workspace run and all-target Clippy cover the carried compiler
sources. Final integration carries that run for unchanged Rust/Cargo inputs,
with a separately tested native-control template added outside the
workspace. The two Rue corpus tests were rerun on the current 23-puzzle data
before a fresh final host build. This is not a new full test run in the final
checkout. Source-derived proof, Lean and site gates require their own current
source checks; execution results do not supply those proofs.

The launcher's pinned source, include, license, commit/tree membership and
published artifact are recorded separately. Independent Chialisp reproduces
the canonical 175-byte module and its tree hash; optimized Rust emits 478 bytes
with a different hash. Matching outputs and ledger results do not establish
module identity or a completed singleton lifecycle.

```sh
cargo build --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 corpus/validate_inventory.py
python3 corpus/validate_tutorial_examples.py
python3 corpus/validate_published_puzzles.py --self-test
python3 scripts/check-error-study.py
python3 scripts/check-proofs.py
python3 scripts/check-ir-proofs.py --output corpus/proofs.json
python3 scripts/check-tree-proofs.py --output corpus/tree-proofs.json
python3 scripts/check-byte-proofs.py --output corpus/byte-proofs.json
python3 scripts/check-canonical-factories.py --self-test
python3 scripts/check-canonical-helpers.py --self-test
python3 scripts/check-runtime-frames.py --replay --self-test
python3 scripts/check-hashing-puzzles.py --self-test --replay
python3 scripts/check-password-coin.py --self-test --replay --extract-ir
python3 scripts/check-cost-optimization.py --self-test --replay
python3 scripts/check-helper-inlining.py --self-test --replay
python3 scripts/check-runtime-refinement.py --self-test --replay
python3 scripts/check-p2dc-append.py --self-test
python3 scripts/check-runtime-composition.py --self-test
python3 scripts/check-p2dc-source-controls.py --self-test
python3 scripts/check-integer-canonicality.py --check --self-test
```

The verifier generates SMT from actual source-derived IR, checks independently
specified bounds and acceptance, detects a removed guard, and replays witnesses
through native Rust and both CLVM optimization modes. These are **IR-model
proofs**, not proofs of compiler preservation or absence of all security flaws.

The separate canonical-integer report checks a manually specified Seq8 encoding
law with 23 solver queries and three model-source mutations. Its decoder-source
hashes provide metadata freshness only; they do not translate or prove the
decoder implementation, width checks, ABI, opcodes or compiler preservation.
Documentation builds inspect retained evidence offline; compiler CI reruns Z3.

The [byte-policy proof guide](https://judeallred.github.io/chialisp-oxide/verification/byte-proofs/)
adds actual Cargo IR for `atom-not-equal` and five source controls, plus two
explicitly synthetic ABI controls. Its independent whole-environment
specification compares exact 8-bit byte sequences and canonical Boolean output.
Eight models have nonvacuity checks and retained optimization-off/on programs;
selected witnesses also run through the original five language builds and
actual native exports. These finite replays do not prove the compiler bridge.

An explicit [acyclic scalar experiment](https://judeallred.github.io/chialisp-oxide/verification/acyclic-scalar/)
checks actual Cargo helper graphs against independent acceptance/output
specifications. Four baseline exports use nonrecursive function tables; three
source mutations demonstrate guard, output and unused-work policy errors that
ordinary Rust accepts. The additional model refutes four obligations and replays
their witnesses. Native Rust and both retained CLVM modes also run every `u8`
input for all sixteen export variants. This is a typed model after ABI decoding,
with separate finite malformed-input checks and no compiler-preservation theorem.
The default proof APIs still reject runtime tables. To execute the opt-in check
with the workspace tools built and Z3 installed:

```sh
acyclic_work=$(mktemp -d)
python3 scripts/check-acyclic-source.py --acyclic-scalar --execute --self-test \
  --tools target/debug --target-dir "$acyclic_work/target" \
  --output "$acyclic_work/evidence"
```

Optimized Rust now reproduces the published **nine-byte `p2_conditions` module
and its exact tree hash**, alongside independently compiled Chialisp and Rue.
A conservative typed-IR pass removes a pair guard only when a required checked
projection still rejects atoms. The structural verifier checks the actual
before/after IR for equal acceptance and output over arbitrary finite trees;
an unsafe guard-removal control must produce a replayable counterexample.
Independent interpreter/VM comparisons also cover strict evaluation, lazy
branches, overflow, malformed environments, and repeated optimization. This
does not prove the optimizer for every program or preserve execution costs and
diagnostic strings. See the [factory walkthrough](https://judeallred.github.io/chialisp-oxide/tutorials/p2-conditions/)
and [formal evidence](https://judeallred.github.io/chialisp-oxide/verification/formal/).

The [host-side VM harness](crates/chialisp-oxide-vm) distinguishes program
rejection from resource exhaustion and infrastructure errors using typed VM
codes. An inconclusive execution cannot count as a policy-rejection witness.
Ledger negatives require exact consensus error codes and unchanged state.
The pinned [Lean experiment](experiments/tree-theorems) separately checks 21
algorithm theorem statements, independent expected types, axiom dependencies,
and a finite bridge to both list-helper implementations. Its manual models
and finite VM observations do not prove compiler preservation.
Signature negatives submit actual unsigned, partial, or incorrectly signed
bundles; a signer preflight error cannot count as ledger rejection.

## Functionality and source coverage

[The functionality map](docs/chialisp-functionality.md) distinguishes Rust’s
ability to express an operation, support in this compiler, executable witnesses,
and missing preservation proofs. Full Chialisp parity is **not yet established**.
Borrowed raw-tree construction/matching, opt-in owned pair construction, recursive
direct calls, explicit whole-environment exports, and reviewed SHA-256
calls are implemented. Arbitrary-precision integers, other cryptographic
operators, currying, and runtime program evaluation remain compiler work; primitive Rust
integers must keep their checked finite semantics.
The reviewed `Cons<First, Rest>` SDK type adds an explicit typed wire pair in
version-5 IR. It preserves arbitrary tails without changing tuple encoding;
general Rust struct, heap and generic-call support remain separate work.
The [hashing guide](docs/hashing.md) explains exact byte concatenation, tree-hash
domain tags, strict once-only operands, profile restrictions and native identity
checks. Immutable `&[u8; N]` to `&[u8]` coercions are supported for `N <= 256`,
including local digest arrays; that fixed-array limit does not narrow atom input
payloads. Mutable references and general pointer casts remain unsupported.
This does not admit arbitrary RustCrypto libraries or ordinary source loops.
All current proof models reject SHA, including unused or dead occurrences;
native vectors and finite VM tests are not cryptographic or preservation proofs.
The [runtime-call implementation](docs/runtime-calls.md) lowers checked Rust
self/mutual and non-tail recursion through finite compiler-owned function tables
and strict lexical bindings in version-4 IR. Tests distinguish rejection from
divergence and check argument order, unused calls, scopes and resource limits.
The paired [rightmost-tree lesson](https://judeallred.github.io/chialisp-oxide/tutorials/recursive-trees/)
compares native Rust with independent Chialisp and Rue. These are finite execution
checks. The default model checkers reject runtime IR; the separately selected
acyclic scalar model admits bounded nonrecursive scalar tables. It does not
admit these recursive tree helpers. Large acyclic source graphs can retry a
typed legacy expansion limit with a fresh checked table-lowering pass;
successful legacy exports retain their existing route and artifacts.
The [published list-helper port](https://judeallred.github.io/chialisp-oxide/tutorials/last-list-element/)
adds ordinary Rust `Tree::Atom([])` matching and ports Chia gaming's pinned
`last_inner` through an explicit Chialisp module adapter. Nil and nonempty
zero-byte tails have different behavior; exact atom bytes and returned subtrees
are preserved. The helper has source/license provenance and finite comparisons,
without claiming a complete library port or published module identity.
The [owned prefix lesson](https://judeallred.github.io/chialisp-oxide/tutorials/prefix-list/)
adds a proper list onto an arbitrary tail with ordinary moves and restricted
standard Box construction. Its 69 named and 1,183 generated inputs run against
all five builds. Native whole-tree/output limits are testing errors, never
policy rejections; [owned-tree semantics](docs/owned-trees.md) explains the boundary.
The [exact-byte comparison lesson](https://judeallred.github.io/chialisp-oxide/tutorials/atom-not-equal/)
uses ordinary Rust `&[u8]` equality and inequality, with a third pinned published
helper adapter. Empty, padded and non-UTF8 atoms remain distinct byte data.
Custom `PartialEq` and mixed slice/array comparisons remain unsupported; this
pure Boolean example does not establish spend authorization.
The [published byte-key insertion sort](https://judeallred.github.io/chialisp-oxide/tutorials/insertion-sort-by-keys/)
uses ordinary Rust slice ordering and owned recursive outputs. It retains the
exact helper's descending order, equal-key behavior and permissive base cases;
this is a helper adapter, not a port of the complete credential proof checker.

Optimized runtime lowering removes redundant frames while keeping fallible
operands evaluated once in source order. The helper's optimized Rust program
fell from 743 to 369 bytes; its `(1 2 3)` cost fell from 25,933 to 11,568.
The independent Chialisp adapter costs 2,236 and optimized Rue costs 3,814 on
that input, so substantial overhead remains. A pinned baseline and 58 old-artifact
replays make these finite measurements reproducible; they are not a general
performance or compiler-correctness proof.

[The published-source catalog](corpus/published-puzzles.json) enumerates pinned
repository trees and records its exact discovery scope. It includes Chialisp,
Rue, Rust builders, bindings and host tooling with separate classifications;
source discovery does not mean a file has been ported, executed, or audited.

The dedicated [error study](https://judeallred.github.io/chialisp-oxide/verification/error-study/)
executes limited Chialisp test suites that miss concrete bugs, then records the
Rust diagnostics that reject the corresponding source mistakes, names the layer
that catches each one, and credits the compiled strict ABI and checked
arithmetic through executed corpus cases. It also keeps two policy bugs that
Rust still compiles as negative controls.

## Documentation

The searchable Astro/Starlight site is in [www](www). It includes tutorials,
three-language source comparisons, a puzzle laboratory, measured bytecode/cost differences,
verification results, supported Rust/ABI references, and a pinned coverage
inventory. Run `pnpm --dir www install && pnpm --dir www dev` for a local preview.
GitHub Pages deployment builds www with pnpm and deploys www/dist through GitHub
Actions at https://judeallred.github.io/chialisp-oxide/.

Puzzle bodies use named condition constructors, return aliases, policy
constants, and explicit amount/time units. Public test-key bytes live in a
separate fixture module; documentation keeps their complete recorded source in
expandable drawers. See the [readability rationale](docs/readability.md).

- [Project goals](goals.md) and [approach](docs/compiler-approach.md)
- [Verification strategy](docs/verification.md)
- [Rust frontend and limitations](crates/chialisp-oxide-driver/README.md)
- [Source-derived verifier](crates/chialisp-oxide-verifier/README.md)
- [Simulator and consensus scope](crates/chialisp-oxide-simulator/README.md)
- [Initial backend experiment](experiments/backend/README.md)
- [Chialisp tutorials/reference](https://chialisp.com/)

The compiler supports restricted single-field transparent newtypes, allowing
nominal domain checks through the real [SDK crate](crates/chialisp-oxide). The
[compatibility module](sdk/README.md) remains for standalone regression fixtures. General structs
and enums outside the restricted `Tree`/`OwnedTree` shapes, general loops,
allocation beyond restricted owned-tree construction, arbitrary external calls,
general references and casts, division/remainder, and other unsupported MIR
remain rejected.
The 98-entry tutorial/CHIP inventory is a roadmap; it is not a claim that all
standard puzzles have been reimplemented. A separate
[example inventory](corpus/tutorial-examples.json) snapshots 12 concrete tutorial
programs, their source spans/license, a naming variant, and supporting workflow
gaps. Three Intro expressions now have finite differential evidence; nine
other programs, the naming variant, and supporting workflows remain unrun.
The parent Intro page remains partial, and no CHIP migration is complete.
See the [canonical Intro walkthrough](https://judeallred.github.io/chialisp-oxide/tutorials/canonical-intro/)
for exact sources, environment contracts, and measured compiler gaps. No production security audit has been
completed. Seven vendored Rue fixes have upstream reports with reproductions,
regression tests and standalone patches: [identity products #108](https://github.com/xch-dev/rue/issues/108),
[strict truthiness #109](https://github.com/xch-dev/rue/issues/109),
[pair reconstruction #110](https://github.com/xch-dev/rue/issues/110),
[quoted nil environments #111](https://github.com/xch-dev/rue/issues/111),
[nested bitwise failure order #112](https://github.com/xch-dev/rue/issues/112),
[canonical Boolean values #113](https://github.com/xch-dev/rue/issues/113), and
[codegen stack exhaustion #114](https://github.com/xch-dev/rue/issues/114).
See [vendor provenance](vendor/rue-lir/UPSTREAM.md) before upgrading Rue.
