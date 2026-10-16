# Rust authoring: Cargo integration, compiler boundaries, and proof tooling

Research date: 2026-10-16. The first Cargo-native authoring milestone is
implemented and tested: an ordinary library uses the real SDK dependency and
entry annotations, Cargo supplies features and generated code, and a pinned
nightly driver emits executable CLVM. This remains an unpublished WIP prototype.
The implemented scope and remaining acceptance criteria are separated below.
The Charon comparison is read-only research; no Lean proof pipeline is installed.

Immediate decision: retain the focused direct MIR adapter, let Cargo handle
crate compilation, and keep Rue behind the typed IR. Do not replace the
frontend with Charon for this subset. Revisit that choice with a
measured dependency/generic/reference experiment. Charon and hax remain useful
implementation precedents and candidates for the separate Lean proof path.

## Implemented authoring interface

The workspace contains a `cargo-chialisp-oxide` command, an ordinary `chialisp-oxide`
dependency, and a small procedural-macro crate re-exported by the SDK. A user
writes an ordinary Rust library with `#[chialisp_oxide::puzzle]` on exported puzzle
functions, runs normal `cargo check` and `cargo test`, then invokes `cargo chialisp-oxide
build` to produce CLVM artifacts. These crates are built from this repository;
none is published. The attribute preserves the original Rust function and adds
an export marker. Rustc performs the semantic checking and MIR extraction.
See the [working Cargo example](../examples/cargo-puzzles/README.md) and
[SDK scope](../crates/chialisp-oxide/README.md).

### Package, import, and command names

The user-facing package is `chialisp-oxide`; Rust code imports `chialisp_oxide`
and writes `#[chialisp_oxide::puzzle]`. Hyphens are our package-family choice,
not a Cargo requirement: the default Rust library name replaces them with
underscores. The existing [`chialisp` package](https://crates.io/crates/chialisp)
belongs to Chia Network's independent compiler, which this project already uses
as a reference dependency. Reusing that name would confuse the two compilers.
[Rust Reference: crate names](https://doc.rust-lang.org/reference/items/extern-crates.html#name-restrictions).

The facade re-exports the attribute from `chialisp-oxide-macros`, so puzzle
authors depend on the facade alone. This follows the established separation
illustrated by [`#[tokio::main]`](https://docs.rs/tokio/latest/tokio/attr.main.html)
and [`tokio-macros`](https://docs.rs/tokio-macros/latest/tokio_macros/).
The `cargo-chialisp-oxide` executable is discovered as `cargo chialisp-oxide`,
following [Cargo's custom-subcommand mechanism](https://doc.rust-lang.org/cargo/reference/external-tools.html#custom-subcommands).
The standalone research/validation executable is `chialisp-oxide`. Internal
packages share the `chialisp-oxide-*` prefix, with
`chialisp-oxide-reference` identifying the independent Chialisp adapter.

Live registry checks on 2026-10-16 returned an existing
[`chialisp` record](https://crates.io/api/v1/crates/chialisp) and HTTP 404
responses for [`chialisp-oxide`](https://crates.io/api/v1/crates/chialisp-oxide),
[`chialisp-oxide-macros`](https://crates.io/api/v1/crates/chialisp-oxide-macros),
and [`cargo-chialisp-oxide`](https://crates.io/api/v1/crates/cargo-chialisp-oxide).
Those point-in-time responses do not reserve names or
guarantee future publication. All project crates remain unpublished prototypes.

Cargo explicitly supports external subcommands and recommends driving its CLI
rather than linking the unstable Cargo library. The launcher uses `cargo
metadata --format-version=1 --no-deps` to discover workspace packages and select
one ordinary library target. Cargo itself resolves and checks dependencies during
the extraction build. Manifest, feature, and lock options are forwarded to the
relevant Cargo commands; package selection is explicit. The current manifest
does not record the complete resolved dependency graph or resolved feature set.
[Cargo external tools](https://doc.rust-lang.org/cargo/reference/external-tools.html),
[cargo metadata](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html).

The proc macro is an annotation mechanism, not a semantic authority. Attribute
macros operate on token streams and can retain the annotated item; rustc still
performs name resolution, type checking, trait checks, and borrow checking.
The driver discovers the expanded export marker and validates its actual typed
signature and reachable MIR. Native compilation works without enabling
compiler-private features in the user's crate.
[Rust Reference: procedural macros](https://doc.rust-lang.org/reference/procedural-macros.html).

Prefer a puzzle library crate within a normal Cargo workspace, alongside wallet
or host tooling. This keeps the constrained on-chain subset clear while host
tests and applications use ordinary Rust. Selecting a Cargo backend does not
make arbitrary dependencies valid on-chain.

## Relevant compiler precedents

Rust-GPU combines several components: `spirv-std` for shader-facing types and
intrinsics, `rustc_codegen_spirv` for the actual codegen backend, `spirv-builder`
for integration, and a Cargo command for setup/builds. Its builder invokes Cargo,
forwards features, uses an isolated target directory, selects a toolchain and
target specification, builds `core`, and passes the backend configuration.
This demonstrates that a target name is one part of a complete compiler
integration, not a replacement for one.
[Pinned Rust-GPU builder](https://github.com/Rust-GPU/rust-gpu/blob/57d07949423593dd701a77d9a78acb4ff7ed943e/crates/spirv-builder/src/lib.rs),
[Cargo GPU](https://github.com/Rust-GPU/rust-gpu/blob/57d07949423593dd701a77d9a78acb4ff7ed943e/crates/cargo-gpu/README.md).

Charon is the closer extraction precedent. Its Cargo launcher installs a rustc
wrapper and uses an explicit target to distinguish target crates from host
build scripts and proc macros. Its driver makes MIR available in dependency
metadata, identifies selected packages, and preserves the metadata artifacts
that later crates consume. These are concrete requirements for extracting
ordinary Cargo projects, not conveniences that a standalone source-file driver
can infer correctly itself.
[Pinned Charon launcher](https://github.com/AeneasVerif/charon/blob/ca34f9191e19f1edea2efd0c364d9344436a81d5/charon/src/bin/charon/main.rs),
[Pinned Charon driver](https://github.com/AeneasVerif/charon/blob/ca34f9191e19f1edea2efd0c364d9344436a81d5/charon/src/bin/charon-driver/driver.rs).

Hax also exposes Cargo commands. Its frontend launcher uses Cargo metadata,
`cargo check`, a workspace wrapper, an isolated output directory, and versioned
cache inputs. Its current recommended Lean path delegates to Charon and Aeneas.
This is further evidence that a Cargo command can provide a conventional Rust
experience without implementing a custom machine-code target first.
[Pinned Hax launcher](https://github.com/cryspen/hax/blob/f33e5c844497c057450ebf56828cbb191223e93a/cli/cargo-hax/src/cargo_hax.rs),
[Pinned Hax overview](https://github.com/cryspen/hax/tree/f33e5c844497c057450ebf56828cbb191223e93a#usage).

Aeneas functionalizes a supported safe-Rust subset and provides proof-assistant
backends, including Lean. External definitions can require explicit models.
It is a candidate for a separate functionalization/verification experiment as
our subset expands; adopting its Cargo interface does not establish that its
translation, integer model or generated functions already match CLVM.
[Pinned Aeneas scope and backends](https://github.com/AeneasVerif/aeneas/tree/8e62dc3dc765602833c86b68dceb18a475a50a8f#targeted-subset-and-current-limitations).

## Why not start with a custom rustc target?

A target JSON defines target properties. It does not add CLVM code generation.
The custom-backend mechanism instead loads a compiler-private dynamic library
implementing rustc's `CodegenBackend` interface. A complete CLVM target would
also need a deliberate target configuration, `core`/SDK strategy, artifact
linking, export collection and precise treatment of Rust's memory-oriented
features. Both the backend API and target JSON format are unstable.
[Custom targets](https://doc.rust-lang.org/rustc/targets/custom.html),
[Custom codegen backend](https://doc.rust-lang.org/unstable-book/compiler-flags/codegen-backend.html).

Our existing backend accepts a typed functional IR and emits CLVM through Rue.
The implemented Cargo-driven MIR extractor follows the extraction tools above.
A real `cargo build --target clvm` workflow remains
a possible later backend project. Until it exists, call the operation a CLVM
export through `cargo chialisp-oxide`; do not advertise a rustc target merely because
configuration contains the word `clvm`.

## Implemented Cargo/driver boundary

1. Cargo supplies the actual rustc arguments, including edition, features,
   `--extern`, search paths, build-script configuration, and generated sources.
   The wrapper accepts Cargo's rustc executable as its first argument and
   preserves normal compiler probes such as version and target queries.
2. An explicit host triple distinguishes target-library invocations from host
   build scripts and proc macros, which compile normally. Extraction therefore
   uses the host Rust target and its `cfg(target_*)` values; this is not a CLVM
   target specification.
3. The driver wraps target dependencies as well as the selected package and
   encodes dependency MIR. Existing wrapper configuration is rejected rather
   than composed: both environment variables and Cargo's effective build-table
   wrapper settings are checked. Compiler arguments remain separate arguments.
4. The selected package completes Rust analysis and explicit local borrow
   checks before annotated MIR is extracted. Cargo-mode callbacks continue
   metadata emission; the standalone harness retains its separate stop policy.
   CLVM artifacts are published only after the entire Cargo check succeeds.
5. Extraction uses an isolated Cargo directory keyed by the driver binary's
   SHA-256. A per-project file lock covers cleaning the selected extraction
   package and checking it again. Each invocation uses a fresh IR directory;
   cached dependency metadata cannot substitute for missing entry exports.
   Native build/intermediate directories are preserved. Successful builds write
   an immutable bundle and atomically replace `latest.json`; failed builds leave
   the previous pointer intact. `cargo chialisp-oxide check` does not publish a bundle.

The versioned manifest records package identity and paths, the full compiler
version, pinned toolchain, analysis target, driver hash, Rue revision, forwarded
Cargo options, and per-entry artifacts. It does not yet capture source hashes,
dependency/lockfile identities, build-script inputs, or the complete effective
cfg/feature environment. Identical artifacts on an unchanged rebuild are tested;
complete reproducible-build provenance remains work to do.
[Cargo launcher](../crates/cargo-chialisp-oxide/src/main.rs),
[wrapper protocol](../crates/chialisp-oxide-driver/src/cargo.rs).

Cargo documents wrapper argument conventions, workspace-only scope, nested
wrappers, encoded rustflags, primary-package identification and artifact paths.
[Cargo environment variables](https://doc.rust-lang.org/cargo/reference/environment-variables.html).

Dependency bodies are a separate capability from resolving their names.
`always_encode_mir` encodes MIR for all functions into metadata; its default is
off and it is an internal unstable option. Passing flags after `cargo rustc --`
affects only the final target, so it cannot by itself prepare dependency MIR.
Configure the relevant dependency invocations deliberately and reject missing
bodies.
[MIR metadata option in Rust 1.97.1](https://github.com/rust-lang/rust/blob/1.97.1/compiler/rustc_session/src/options.rs),
[cargo rustc argument scope](https://doc.rust-lang.org/cargo/commands/cargo-rustc.html).

Cross-crate execution currently admits vetted, non-generic `chialisp_oxide` functions
and structurally validated SDK newtypes. Other ordinary dependency calls remain
unsupported, even when Cargo can resolve and type-check them. Do not remove every
external-call gate as a shortcut.
For example, the current driver turns `AddWithOverflow` into checked arithmetic
because explicit `overflowing_*` library calls are rejected. Admitting those
library bodies without changing that lowering would miscompile valid wrapping
results. Broader libraries require explicit overflow-pair semantics or verified
guard-use restrictions, alongside each newly supported MIR construct.

Generic dependency calls require resolving concrete rustc `Instance`s and
instantiating bodies/types, not walking only source `DefId`s. Rustc's
monomorphization collector identifies reachable concrete instances and runs
before code generation. Our present non-generic subset should remain explicit
until this path has compile-fail and runtime evidence.
[Rustc monomorphization](https://rustc-dev-guide.rust-lang.org/backend/monomorph.html).

## Toolchain policy

The project now pins `nightly-2026-07-14` (`1.99.0-nightly`) with `rustc-dev` and
LLVM tools. The driver tests and real Cargo integration have passed on this
toolchain. No `RUSTC_BOOTSTRAP` setting is required; this replaces the earlier
stable-toolchain workaround. The compiler APIs remain unstable. Rust's
documentation discourages depending on bootstrap, and a dated nightly makes
that dependency explicit rather than making the compiler interface stable.
[Rust bootstrap policy](https://doc.rust-lang.org/unstable-book/compiler-environment-variables/RUSTC_BOOTSTRAP.html).

Rustc recommends its driver interface when possible; its internal APIs remain
unstable. The launcher selects the pinned Cargo and the actual pinned rustc
executable, overriding an unrelated `RUSTC` environment setting. The wrapper
verifies the compiler commit expected by the built driver before extraction.
Target dependencies use that compiler as well. Future toolchain upgrades must
repeat the driver, Cargo, corpus, and proof checks rather than changing the date
alone.
[Rustc driver guidance](https://rustc-dev-guide.rust-lang.org/rustc-driver/intro.html).

The project formerly called StableMIR is now `rustc_public`. It aims to expose a
public SemVer interface, but its current integration still requires nightly,
`rustc-dev`, and unstable compiler initialization. It is a candidate to reduce
our internal-API surface, not a stable drop-in dependency already adopted here.
Keep any experiment inside the driver; copy our owned IR out of the compiler
callback rather than retaining compiler handles across callbacks or threads.
[Pinned project status](https://github.com/rust-lang/rustc_public/blob/b37138687103d1b69952a64d825e67cf51adfb9c/README.md),
[Pinned integration guide](https://github.com/rust-lang/rustc_public/blob/b37138687103d1b69952a64d825e67cf51adfb9c/book/src/initial.md).

## Bounded Charon comparison: retain direct MIR for this subset

This comparison inspected source at Charon revision
`ca34f9191e19f1edea2efd0c364d9344436a81d5`, rather than installing or executing
Charon. No extraction compatibility, performance, or dependency-size benchmark
was run. The assessment below is an architectural judgment from those sources
and our current supported subset.

Charon would help with work that becomes substantial when we support ordinary
dependencies: semantic extraction across crate boundaries, trait/type information,
MIR cleanup, reachable item selection, and optional monomorphization. Its output
has both unstructured ULLBC and structured LLBC forms. A future adapter could
consume one of these without importing rustc handles into the backend. This
does not itself supply our CLVM layout, integer policy, decoder, SDK intrinsic
mapping, cost bounds, or Rue lowering.
[Pinned Charon overview](https://github.com/AeneasVerif/charon/blob/ca34f9191e19f1edea2efd0c364d9344436a81d5/README.md),
[Pinned extraction options](https://github.com/AeneasVerif/charon/blob/ca34f9191e19f1edea2efd0c364d9344436a81d5/charon/src/options.rs).

For our current first-order subset, the direct adapter already translates
the needed control flow and enforces strict failures. Adopting Charon now adds
another AST-to-oxide adapter and a new transformation boundary before it removes
many implemented obligations. Broader Charon feature coverage must not be
mistaken for broader supported CLVM semantics. Source-level diagnostics and the
adversarial corpus would have to survive the replacement unchanged.

The dependency tradeoff is concrete, although not measured in installed bytes:

- The audited toolchain is `nightly-2026-09-17` with compiler components,
  Rust sources, and Miri. That date belongs to this Charon revision, not to a
  verified chialisp-oxide configuration.
- Its Cargo launcher ensures an explicit target and attempts a full-MIR sysroot
  through Miri; failure can fall back to the ordinary sysroot. A chialisp-oxide adoption
  must reject any missing reachable body rather than treating that fallback as
  a complete library translation.
- `charon-lib` can build without compiler-private features using
  `--no-default-features`. The extraction binary still needs the pinned compiler.
  Defaults also enable MiniRust support; the manifest includes local helper
  crates and a Git tracing dependency. The JSON-library route reduces coupling
  to compiler handles, but does not remove upstream schema/toolchain maintenance.

[Pinned toolchain](https://github.com/AeneasVerif/charon/blob/ca34f9191e19f1edea2efd0c364d9344436a81d5/rust-toolchain),
[Pinned sysroot handling](https://github.com/AeneasVerif/charon/blob/ca34f9191e19f1edea2efd0c364d9344436a81d5/charon/src/bin/charon/toolchain.rs),
[Pinned Cargo manifest](https://github.com/AeneasVerif/charon/blob/ca34f9191e19f1edea2efd0c364d9344436a81d5/charon/Cargo.toml).

Charon calls its status beta and documents open bugs and planned API changes.
Its presets reconstruct arithmetic failure and panic paths, and it exposes an
explicit borrow-check bypass option. These are configurable analysis facilities,
not permission to weaken our source-language contract. Any later adoption must
keep Rust checking enabled, fail on incomplete extraction, pin the selected MIR
stage and transformations, and validate success and rejection against the
original source. The default Aeneas preset is not automatically the correct
compiler preset for CLVM.
[Pinned Charon limitations](https://github.com/AeneasVerif/charon/blob/ca34f9191e19f1edea2efd0c364d9344436a81d5/README.md#limitations),
[Pinned compiler callbacks](https://github.com/AeneasVerif/charon/blob/ca34f9191e19f1edea2efd0c364d9344436a81d5/charon/src/bin/charon-driver/driver.rs).

Reconsider replacement when the direct adapter's missing work is primarily
generic instances, external traits, or ownership/reference functionalization.
Compare both frontends on the same pinned Cargo package, features, source errors,
overflow controls, discarded failing computations, and malformed solutions.
Require a measured reduction in maintenance or needed functionality, with no
regression in the existing ABI and rejection corpus, before changing the default.

## Next Lean experiment, not an installed proof pipeline

The inspected hax release is `cargo-hax-v0.4.2`, commit
`5ccf89dd2e996c9f88f2267eaa2a64739c4ca41a`, published October 1, 2026. Its Lean
backend uses Charon and Aeneas. The release's tested default tool set is Aeneas
`build-183e4f0`, Charon `nightly-2026.09.02`, Lean `v4.31.0`, and hax Lean library
`v0.3.30`. These defaults differ from the newer Charon source audited above;
do not combine their revisions and assume compatibility.
[Pinned release](https://github.com/cryspen/hax/releases/tag/cargo-hax-v0.4.2),
[Pinned tested tool set](https://github.com/cryspen/hax/blob/5ccf89dd2e996c9f88f2267eaa2a64739c4ca41a/cli/cargo-hax/defaults.toml).

A bounded next experiment should place the bounded-counter policy and one typed
payment policy in a Cargo verification fixture, then extract them using that
release and its matching `hax-lib`. A committed
`hax.toml` scenario should select the exact package, entry closure, and features.
Use the release's checked tool manifest, retain the generated Lean project pins,
and explicitly list models of external definitions. Record source, Cargo.lock,
compiler, feature/cfg, extraction, and artifact identities. No hax, Charon,
Aeneas, OCaml, or Lean installation was performed for this comparison.
[Pinned scenario and version contract](https://github.com/cryspen/hax/blob/5ccf89dd2e996c9f88f2267eaa2a64739c4ca41a/docs/manual/tools.md).

Proof acceptance requires independently authored input/output and rejection
properties, completed Lean theorems, and a transitive axiom audit. A successful
`lake build` merely checks the declarations that were included: it does not
automatically establish panic freedom or prove a generated placeholder. Check
the expected theorem set, reject `sorryAx`, and report any external assumptions.
Regenerate extraction in CI and reject stale generated sources before checking
the proofs. Keep human proofs and external models outside overwritten extraction.
[Pinned Lean workflow](https://github.com/cryspen/hax/blob/5ccf89dd2e996c9f88f2267eaa2a64739c4ca41a/docs/manual/lean/quick_start.md),
[Lean axiom auditing](https://lean-lang.org/doc/reference/latest/Axioms/).

Creusot is another viable research comparison for source contracts: it translates
Rust into Coma and sends verification conditions through Why3/SMT. It is not a
Lean backend, and no Creusot proof is included here.
[Official Creusot tutorial](https://guide.creusot.rs/tutorial.html).

## Source proofs and backend correctness remain separate

A proof about Aeneas-extracted Rust semantics does not prove the artifact produced
by our separate MIR adapter. Conversely, our SMT results describe chialisp-oxide IR under
typed input assumptions, not all Rust source behavior. Connecting those models
requires an explicit relation between source values, chialisp-oxide values, and CLVM trees,
including rejection and integer overflow. An eventual shared LLBC extraction
could reduce duplicated frontend work, but would still need a preservation link
through chialisp-oxide lowering, Rue optimization, serialization, and ABI decoding.

Continue native/Chialisp/optimized/unoptimized execution comparisons and mutation
replay while that link is developed. Preserve the distinctions in
[verification.md](verification.md): executed cases, bounded models, model proofs,
and compiler-preservation proofs. Neither Cargo integration nor a source-level
Lean theorem establishes universal puzzle security or canonical hash compatibility.

## Executed evidence for the first Cargo milestone

The [Cargo integration test](../crates/cargo-chialisp-oxide/tests/cargo_workflow.rs) builds
the real [consumer library](../examples/cargo-puzzles/src/lib.rs) using its
separate Cargo manifest, checked-in lockfile, and path SDK dependency. Driver
tests cover the compiler boundary separately. The validated milestone includes:

- Two annotated exports: `pay` uses nominal SDK types and real SDK constructors;
  `guarded_add` returns a checked integer sum. Native tests cover the same policy functions. An
  unexported function uses closures, allocation, and formatting normally.
- A host build script writes an `OUT_DIR` source file. The `strict-cap` Cargo
  feature changes its limit, and executing the emitted CLVM confirms the changed
  acceptance boundary.
- Optimized and unoptimized CLVM produce the exact expected serialized payment
  conditions and arithmetic result. The expected bytes are independently
  specified in the test. A padded integer solution is rejected. The separate
  paired corpus continues to provide native-Rust/Chialisp differential evidence;
  the Cargo integration test does not itself compile a Chialisp reference.
- An unchanged rebuild publishes a fresh bundle with identical entry artifacts.
  A distinct forwarding driver binary gets a distinct dependency-cache namespace.
  The user's separate intermediate directory and sentinel remain untouched.
- Repeated release-profile builds regenerate exports, and a foreign configured
  build target does not displace the explicit host analysis target.
- An invalid `RUSTC` environment override does not displace the pinned compiler.
  Both Cargo-configured wrapper forms are rejected with a specific diagnostic.
- Rust type errors and unsupported CLVM operations fail without replacing the
  last successful bundle pointer. A successful `cargo chialisp-oxide check` also leaves
  that pointer unchanged. Driver tests separately cover borrow checking,
  domain-type swaps, unsupported signatures, and annotation diagnostics.

The initial command intentionally selects one ordinary library package. Existing
compiler wrappers, user-selected custom targets, and workspace-wide extraction
are unsupported. External calls are restricted to the concrete SDK subset;
generics, traits, references, and arbitrary dependency bodies remain future work.

An additional manual smoke test used a temporary virtual workspace with two
library members. The selected package had a custom library name and renamed
the SDK dependency to `clvm`. Both build/check commands worked with explicit
`-p selected-puzzles --features strict-cap --locked --offline` from the root,
and without `-p` from the selected member. Only its annotated export was emitted.
Two native tests passed; emitted CLVM produced the independently encoded
`CREATE_COIN` result at amount 100 and rejected 101. This expands executed
compatibility evidence, but is not yet a checked-in CI regression fixture.

## Remaining acceptance criteria

- Promote the manual renamed-dependency/virtual-workspace/member-selection
  smoke test to a checked-in regression fixture; extend it with feature-disabled
  entries and unrelated workspace package cases.
- Add explicit missing/deleted-export and compiler-version mismatch controls,
  and broader concurrent invocation tests, alongside the current fresh-directory,
  driver-hash, feature-switch, and unchanged-rebuild controls.
- Demonstrate any newly admitted dependency on native Rust and independent
  Chialisp/reference vectors, including strict failure and integer overflow.
  Generic execution requires concrete rustc instance/type substitution support.
- Capture source/dependency/lockfile identities, effective features/cfgs, SDK
  identity, and build-script input/generated-source identities in reports. Two
  isolated builds with the same complete inputs must reproduce serialized
  artifacts and tree hashes. Cargo.lock alone is insufficient provenance.
- Evaluate a real rustc `CodegenBackend` and CLVM target only if it improves this
  working Cargo interface enough to justify its additional runtime, linking,
  target-layout, and compiler-integration obligations.
- Run the pinned Charon/hax comparison and complete the source/Lean proof
  experiment before advertising those workflows. Connect source and backend
  models explicitly before claiming compiler preservation.

Reference projects provide design precedents. New capabilities still require
executed evidence in this repository before they are advertised as supported.
