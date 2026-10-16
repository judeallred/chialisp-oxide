# Rust → CLVM: research and proposed approach

Research date: 2026-10-16. Status: pinned rustc-driver frontend, typed IR, direct Rue backend,
paired puzzle corpus, simulator checks, and source-derived SMT experiments implemented. See [measured results](../experiments/backend/README.md) and
[verification strategy](verification.md).

## Recommendation

Build a compiler for a documented subset of real Rust. Use rustc for name
resolution, type checking, trait resolution, and borrow checking; lower supported
MIR into a small, typed oxide IR; reuse Rue compiler infrastructure to lower,
optimize, and generate CLVM. Generated Rue source is a bootstrap option, not a
required intermediate. Integrating or adapting Rue compiler source directly into
this repository is explicitly acceptable, including for the first working slice.
Choose the integration boundary through a small comparative experiment.

Proposed pipeline:

```text
Rust puzzle crate + oxide SDK
    → pinned rustc, with all required Rust checks completed
    → reachable, monomorphized MIR + supported-feature validation
    → oxide typed, pure IR + explicit puzzle ABI
    → Rue backend integration (source emission or adapted compiler IR/lowering)
    → Rue LIR optimizer → CLVM serialization
    → clvmr execution + native Rust / Chialisp comparison
```

The typed IR keeps Rust-specific semantics separate from Rue. A direct backend
can reuse adapted Rue lowering rather than rebuilding everything ourselves.
Generated Rue can remain an optional debugging artifact and a
way to compare this project with the existing Rust → Rue prototype referenced
in goals.md. That prototype has not been inspected: obtain its source before
duplicating it or treating any of its capabilities as established.

## What the Rue source establishes

Inspected upstream revision:
[`245c107a8e795229ea08097142f005a4c0d91075`](https://github.com/xch-dev/rue/tree/245c107a8e795229ea08097142f005a4c0d91075).
Links below are pinned to that revision so later upstream changes do not silently
change the proposal's evidence.

- [`rue-lir`](https://github.com/xch-dev/rue/blob/245c107a8e795229ea08097142f005a4c0d91075/crates/rue-lir/src/lir.rs)
  exposes an arena-based CLVM-oriented IR with atoms, paths, pairs, application,
  branches, arithmetic, hashes, and cryptographic operators.
- [`optimize`](https://github.com/xch-dev/rue/blob/245c107a8e795229ea08097142f005a4c0d91075/crates/rue-lir/src/optimize.rs)
  is a public Rust function accepting an LIR arena and root ID. It is an IR
  optimizer, not an arbitrary serialized-CLVM postprocessor.
- [`codegen`](https://github.com/xch-dev/rue/blob/245c107a8e795229ea08097142f005a4c0d91075/crates/rue-lir/src/codegen.rs)
  emits a clvmr node and has a separate static-pair optimization option.
- [`rue-compiler/src/file.rs`](https://github.com/xch-dev/rue/blob/245c107a8e795229ea08097142f005a4c0d91075/crates/rue-compiler/src/file.rs)
  demonstrates the actual sequence: dependency graph, HIR lowering, optional LIR
  optimization, code generation. Direct LIR emission bypasses earlier compiler
  work, including dependency handling and lowering; the LIR optimizer alone does
  not replace that work.
- [`CompilerOptions`](https://github.com/xch-dev/rue/blob/245c107a8e795229ea08097142f005a4c0d91075/crates/rue-options/src/compiler_options.rs)
  controls automatic inlining, LIR optimization, static pairs, debug symbols,
  and constant-evaluation cost. Benchmark these independently rather than
  equating every debug/release difference with the LIR optimizer.
- Upstream includes CAT, singleton, delegated-condition, partial-offer, and
  royalty-split examples. Some explicitly say they are unaudited and untested;
  they are learning material, not correctness or security oracles.
- The inspected workspace declares Rue 0.11.0, clvmr 0.21.0, Apache-2.0 licensing,
  and Rust toolchain 1.97.0. The local compiler reports 1.93.0. Building this
  upstream revision requires provisioning a compatible toolchain first; no
  upstream build or execution was performed during this research.

## Frontend decision

Use a pinned nightly rustc driver with rustc-dev components. Rust's documented
[MIR](https://rustc-dev-guide.rust-lang.org/mir/index.html) has explicit types,
locals, and control-flow blocks. The
[driver interface](https://rustc-dev-guide.rust-lang.org/rustc-driver/intro.html)
provides integration with the compiler. The exact callback/query sequence and
cross-crate monomorphization need a feasibility spike; do not assume merely
accessing a MIR body proves borrow checking has run.

Do not parse rustc's human-readable MIR output as an interchange format. Do not
reimplement Rust type or borrow checking in a syn-based source translator.
A macro DSL could be a faster separate product, but would require narrower
claims about Rust compatibility and library support.

Alternatives considered:

- Integrated Rue compiler source: a first-class option. Adapt relevant IR,
  dependency handling, lowering, optimization, and code generation to accept our
  typed IR without a Rue text round trip. Inspect dependencies on Rue-specific
  types and symbol tables before selecting the entry point. Preserve upstream
  license and NOTICE requirements, record the imported revision and local changes,
  and retain relevant upstream tests. Prefer a focused import or fork over copying
  unrelated CLI, LSP, or frontend code. Accept responsibility for maintenance and
  upstream reconciliation.
- Direct typed IR → Rue LIR: avoids generated text and
  repeated type checking, but requires managing environments, paths, function
  application, and representation ourselves unless adapted Rue lowering supplies
  them. Compare this with source emission and integrated lowering during the
  first spike; do not require a text-emitting compiler first.
- LLVM or WebAssembly → CLVM: adds a memory/stack machine representation and
  runtime obligations that do not naturally match immutable CLVM trees. Defer.
- A full rustc codegen backend: potentially useful later, but more integration
  work than a focused driver and lowering pass for a deliberately small subset.

## Define semantics before accepting language features

Start with pure functions, owned immutable values, tuples/structs, booleans,
pattern matching, calls to known functions, and a small set of SDK intrinsics.
Translate branches and locals into explicit pure values. Begin with acyclic
control flow; loops, recursion, closures, generics, and references each need
separate lowering and cost tests before being advertised as supported.

Reject unsupported constructs with source locations. Initial exclusions include
unsafe code, raw pointers, mutable aliasing, allocation, trait objects, custom
destructors, async, threads, I/O, floating point, FFI, and unwinding. Inspect the
reachable dependency graph too; checking only the entrypoint is insufficient.
Ordinary Rust syntax does not imply that every Rust crate can execute on CLVM.

Critical representation contracts:

1. Integers: CLVM integer operations must not silently stand in for fixed-width
   Rust arithmetic. Preserve widths, signedness, overflow behavior, casts,
   division/remainder, and shifts explicitly. Prefer checked SDK operations for
   the first puzzles; reject operations without implemented semantics. A custom
   arbitrary-precision SDK integer can be a later, explicit type.
2. Bytes and booleans: define canonical encodings; never confuse numeric zero,
   byte strings, false, empty lists, and unit merely because some share a CLVM
   representation. Validate untrusted inputs at the puzzle boundary.
3. Products and sums: specify struct/tuple layout, list termination, enum tags,
   field order, and solution decoding. Rust's native memory layout is not the ABI.
4. Failure: define puzzle rejection separately from returned application errors.
   No unsupported panic or unwind path may disappear in lowering.
5. Cryptography: map SDK intrinsics to CLVM operators with exact input encodings
   and a pinned consensus dialect. Host reference implementations must agree.
6. Evaluation: preserve strictness and branch laziness, including failing
   expressions in unused branches. Optimization must preserve rejection behavior.
7. Resource use: bound compiler constant evaluation and runtime execution. Compile
   success does not establish termination or acceptable CLVM cost.

Use distinct types such as PuzzleHash, CoinId, Amount, PublicKey, and Condition
with controlled constructors. This can prevent domain mixups, but only where
raw bytes and unchecked conversion escape hatches are unavailable.

## Validation is the foundation of the tutorial system

Every example should carry original Chialisp source and provenance, Rust source,
generated Rue when available, compiler versions/options, curried arguments, input vectors,
serialized CLVM, disassembly, tree hash, serialized size, execution cost, output
conditions, and expected rejection cases. Store machine-readable reports and
generate side-by-side documentation from those reports.

Compare four executions on identical decoded inputs: native Rust, reference
Chialisp CLVM, Rust-generated CLVM without LIR optimization, and optimized CLVM.
Use unit vectors, boundary cases, malformed solutions, and property-based tests.
Apply explicit cost limits and consensus flags to every CLVM run. For actual
coin behavior, add a Chia simulator: returning conditions successfully does not
prove signatures, announcements, timelocks, or a spend bundle are valid.

Report three different results:

- Behavioral equivalence over the tested cases, including rejection behavior.
- Bytecode and disassembly differences, plus size and cost deltas.
- Tree-hash identity and compatibility with external hash commitments.

Different bytecode usually means a different puzzle hash. Reimplementing a CHIP
puzzle's behavior does not automatically make it a drop-in replacement in a
protocol that commits to its module hash. Do not rewrite canonical constants to
make tests pass. Record identity requirements and integration failures openly.

The test corpus provides evidence, not a general proof of compiler correctness.

## Deliver the goals in stages

### 0. Recover prior work and lock the experiment

Obtain the Trello-linked prototype source; record what it already supports.
Pin Rue by revision, select a compatible rustc-driver toolchain, and lock
dependencies. Build and run one upstream Rue example. Verify a tiny hand-built
LIR program with optimization both enabled and disabled. Exit: reproducible
commands and actual output/cost records, not an architecture diagram alone.
Compare generated Rue with direct or adapted Rue internals on the same tiny
puzzle. Select the simplest faithful integration based on retained compiler
passes, diagnostics, implementation effort, and maintenance. The first working
compiler may skip Rue source entirely.

### 1. Prove one complete Rust puzzle

Implement SDK boundary types, the rustc frontend, a minimal typed IR, and the
selected Rue backend integration for one delegated-condition or fixed-payment puzzle. Use the same source
for native execution and compilation. Add positive and compile-fail fixtures,
including a Rust borrow/type error to establish that normal Rust checking runs.
Exit: Rust, Chialisp, and optimized/unoptimized CLVM agree on valid and invalid
vectors; the report includes hashes, sizes, costs, and reproducible commands.

### 2. Expand semantics from tutorial needs

Treat [chialisp.com](https://chialisp.com/) as a primary source for the
tutorial corpus and language/VM reference material. Inventory its examples,
operators, conditions, costs, optimization, common issues, attacks and
countermeasures, debugging, primitives, and CLVM sections alongside the primer
and concepts. Use documented pitfalls as inputs to the error study and negative
test corpus. Record each source URL and revision when importing examples; verify
consensus-sensitive behavior against the pinned runtime and Chia implementation.

Add features only when demanded by executable examples. Start from the official
[Chialisp primer](https://chialisp.com/chialisp-primer/),
[concepts](https://chialisp.com/chialisp-concepts/), and
[Chia Academy](https://docs.chia.net/academy/chialisp/): modules, coins, currying,
signatures, delegated puzzles, and inner puzzles. Preserve any original lesson's
insecurity explanation, especially password-coin examples. Capture a versioned
inventory rather than promising an undefined set of “all tutorials.”

### 3. Publish the error study from executable evidence

For each claimed prevented error, include a Chialisp failure, the Rue equivalent
or its existing diagnostic, Rust's diagnostic or SDK rejection, and any residual
runtime obligation. Rue already has a type system: do not attribute its existing
protections uniquely to Rust.

Candidate incremental benefits are nominal domain types, exhaustive matches,
ownership/move checks where those features are supported, and reusable typed
APIs. Classify findings as compile-time impossible within the supported subset,
runtime checked, or still possible. Missing authorization, incorrect economic
logic, replay mistakes, and bad timelock choices remain possible without stronger
domain-specific rules. Generate the main-page summary only from verified cases.

### 4. Inventory and reimplement CHIP puzzles

Pin the [CHIPs repository](https://github.com/Chia-Network/chips) and follow each
proposal's reference implementations into canonical puzzle sources. Track CHIP
status, source revision/path/license, dependencies, module hashes, test vectors,
implementation state, and exclusions. Include superseded/grandfathered standards
explicitly rather than silently changing what “every known puzzle” means.

Implement simpler independent puzzles before singleton layers, CATs, NFTs, and
composed protocols. Reuse the same evidence report and simulator validation.
This is an ongoing versioned coverage inventory, not a claim of completeness
based on Rue's examples or the CHIP index alone.

### 5. Demonstrate Rust library reuse honestly

Separate host-side tooling (test generation, wallets, serialization, docs) from
on-chain code. Host tooling can use ordinary Rust libraries. On-chain reuse is
limited to reachable code supported by the compiler and SDK. First demonstrate
a small pure dependency, then publish a tested compatibility list; no blanket
promise about std or no_std libraries.

## Proposed repository structure

Create these as the first implementation lands, rather than adding empty crates:

```text
crates/clispox-driver/      rustc integration and diagnostics
crates/clispox-ir/          typed pure IR and validation
crates/clispox-rue/         Rue backend adapter or integrated compiler passes
crates/clispox-sdk/         puzzle ABI, domain types, intrinsics
crates/clispox-cli/         reproducible build / compare commands
tests/                     compile-fail and semantic regression cases
examples/                  paired Rust / Chialisp / vectors
corpus/                    versioned tutorial and CHIP inventories
docs/                      generated comparisons and error study
```

Keep rustc internals confined to the driver and Rue internals confined to the
backend. Each dependency upgrade must pass the semantic corpus before adoption.

## Outstanding decisions and honest limits

- The Trello-linked prototype is a reference to obtain, not inspected evidence.
- Exact rustc hooks, monomorphization access, and toolchain compatibility require
  the first feasibility spike.
- Full Rust support is not proposed. Supported semantics must grow with tests.
- The first backend experiment measures byte size, VM cost, output equivalence,
  and hash differences for one small policy. These findings do not establish
  performance or correctness for the future Rust compiler.
- Keep original source attribution and license records for copied tutorials and
  puzzle implementations. No license for this new project has been selected.


## Implementation update

The first comparative experiment favored direct LIR for a tiny policy. We now
use a pinned rustc 1.97.1 driver with package-scoped compiler-private access,
complete Rust analysis, and explicit borrow-check forcing. It emits validated
oxide-ir JSON. The backend enforces canonical typed ABI decoding, fixed-width
checked arithmetic, strict unused failures, lazy branches, and compile-time
expansion limits before Rue code generation. Unsupported MIR fails closed.

Nine source pairs are validated by the native source functions, independent
Chialisp compilation, both optimization settings, and 30 ledger scenarios.
`corpus/validation.json` contains the evidence; `corpus/proofs.json` records
source-derived IR-model proofs and attack replay. `www` renders these reports.
The imported Rue LIR includes a reproduced optimizer panic fix and retains 44
upstream/regression tests. The upstream report is xch-dev/rue#108.

Remaining work includes richer Rust/SDK types and libraries, canonical tutorial
and CHIP implementations, hash/curry/list intrinsics, cheaper sharing-preserving
lowering, broader byte/crypto proof semantics, and verified compiler-preservation
links. The initial teaching corpus does not complete the tutorial/CHIP inventory.
