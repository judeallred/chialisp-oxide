# Iteration status

## Working and tested

- Pinned rustc driver: complete analysis/borrow checking, typed MIR, strict
  checked arithmetic/branch semantics, explicit unsupported diagnostics, and
  compiler resource limits. The driver is compiler-private and host checked.
- An unpublished Cargo SDK and marker macro, multiple annotated exports, normal
  native checks/tests, and Cargo-driven CLVM extraction with real features and
  generated source. The paired corpus is the real `chialisp-oxide-puzzles` Cargo library;
  native execution, CLVM extraction, and proof replay use that same package.
- Nominal one-field transparent wrappers and typed condition helpers in the SDK
  crate; domain swaps fail Rust checking while
  wire representations stay explicit.
- Pure typed IR and direct Rue LIR generation, canonical untrusted ABI decoding,
  checked fixed-width bounds, byte arrays, tuples, booleans, and exact arity.
- An explicit SHA-256 bridge admits one reviewed safe native implementation by
  actual signature and complete typed-MIR closure/profile fingerprint. Cargo
  extraction and the fresh corpus report cover all twenty-one annotated exports,
  including the raw-domain `sha256tree` helper adapter, singleton launcher and
  original password tutorial module.
  Immutable fixed-byte-array references coerce to slices for lengths up to 256;
  this does not admit general casts, mutable slices or arbitrary hash libraries.
- Ten original teaching examples, three exact canonical Intro expressions, and
  the canonical `p2_conditions` program factory, five pinned published helpers
  through explicit module adapters, the singleton launcher and original password
  module, with
  side-by-side Chialisp/Rust/Rue sources and separately compiled Rue/Chialisp, the actual Rust functions as native
  oracles, 389 explicit and 20,796 generated integer/tree/byte vectors. Canonical closed
  expressions explicitly ignore environments; default typed ABIs remain strict.
- Forty complete-spend scenario groups run separately for Rust, Chialisp, and Rue
  (120 ledger scenario runs) through chia-wallet-sdk's simulator, plus
  standalone regressions for signatures/replay/messages/conservation/rollback.
- Hash-helper comparisons use a recorded ten-million cost ceiling; all other
  examples retain one million. Five deep inputs additionally run in all five
  builds at one million as 25 separate typed resource observations, outside
  passing coverage totals. The compatibility gate preserves all eighteen prior
  ports' artifacts and observations and passes 149 migration controls.
- The singleton launcher retains exact source/include/license/commit/tree
  provenance and its published 175-byte artifact. Independent Chialisp reproduces
  that identity; optimized Rust is 755 bytes with a different hash. Executed
  output and spend agreement is separate from module identity and the unported
  remainder of the singleton lifecycle.
- The exact password tutorial module preserves its raw environment and returns
  arbitrary condition trees. Separate fixed-hash curry instances record 48
  case groups in five builds, three varying-hash controls and 21 actual ledger
  submissions. Unsigned destination retargeting succeeds; currying does not
  repair that policy. Dedicated consensus rejections and broad runtime-umbrella
  observations have separate credit. The complete tutorial's funding/RPC/support
  workflow and a typed public instance-authoring API remain unimplemented.
- Borrowed raw atom/pair trees preserve byte contents and improper tails.
  Optimized Rust reproduces the published factory module's bytes and hash;
  its actual IR rewrite has a separate structural-model equivalence check.
- Checked Rust self/mutual and non-tail recursive calls lower once per function
  into version-4 typed tables and strict lexical bindings. Ordered failure and
  divergence controls, fresh callee scopes and pre-lowering resource bounds are
  tested; existing proof models explicitly reject runtime IR.
- Ordinary Rust empty-slice patterns distinguish nil from nonempty zero-byte
  atoms. The published `last_inner` helper has pinned source/license/span and
  wrapper checks, proper/improper-list vectors and arbitrary returned trees.
- Opt-in `OwnedTree` adds restricted standard-Box pair construction and
  by-value recursive outputs. The published `prefix` adapter preserves arbitrary
  heads/tails; native whole-tree and result-depth limits remain oracle errors.
  Actual Cargo tests cover moves, borrows, snapshots, cleanup and failure order.
- Primitive immutable byte-slice and identical fixed-byte-array `==`/`!=` use
  actual resolved standard `PartialEq` identities, with byte-exact ABI/strictness
  tests and a pinned published inequality helper. General traits, structural
  tree equality and mixed slice/array comparison remain unsupported.
  Primitive immutable byte ordering now separately checks standard `PartialOrd`
  identities and uses unsigned lexicographic `>s`; the published insertion-sort
  helper combines this operation with owned recursive outputs. Ordered-byte
  model proofs remain unsupported.
- Lean algorithm models have 21 fixed theorem statements, kernel/axiom checks,
  corruption controls and a finite execution bridge. They do not establish
  Rust/MIR/IR/CLVM preservation; recursive IR remains outside the SMT models.
- Typed VM failures distinguish program rejection from resource/infrastructure
  failures. Ledger negatives require exact consensus codes; genuine incomplete
  and wrong signatures reach transaction validation instead of signer preflight.
- Source-derived SMT queries with independent specifications, nonvacuity,
  counterexample replay, mutation control, and fail-closed solver handling.
  The strict byte/Boolean gate extracts actual Cargo IR for the original
  inequality policy and five source controls; two further ABI controls are
  explicitly synthetic. All eight models retain off/on artifacts and selected
  VM replays; original-policy witnesses also use all five language builds.
  This whole-environment Seq8 contract is separate from earlier manual IR tests
  and does not prove the Rust/IR/CLVM translation. See the
  [byte proof guide](https://judeallred.github.io/chialisp-oxide/verification/byte-proofs/).
- Shared declared Cargo input/export provenance in validation and proof reports,
  checked against current local files before publishing documentation.
- Target-unit pinned sysroot/host-target checks, early backend-override rejection
  and bounded actual loaded-library checks cover core/alloc/std and primitive
  language/diagnostic owners, including aliases and dependency metadata.
  Native-valid relocation and response-routing controls preserve the separate
  trusted-installation assumption; these are not binary authentication.
- Six Rue optimizer regressions/fixes with pinned source provenance and upstream
  issues [108](https://github.com/xch-dev/rue/issues/108),
  [109](https://github.com/xch-dev/rue/issues/109),
  [110](https://github.com/xch-dev/rue/issues/110),
  [111](https://github.com/xch-dev/rue/issues/111),
  [112](https://github.com/xch-dev/rue/issues/112), and
  [113](https://github.com/xch-dev/rue/issues/113).
- Searchable Astro/Starlight documentation in www, generated from source/evidence.
- Executable error studies: intentionally limited Chialisp tests miss concrete
  bugs rejected by native Rust type/name checking; a missing-cap negative
  control keeps policy mistakes and shared Rue guarantees explicit.
- Published-source discovery across 98 pinned repository trees and 3,582 selected
  files, with path/role/license provenance and offline tree reconstruction.
  This is source discovery, not 3,582 validated or distinct puzzles.
- An explicit functionality/operator map separates native expressibility,
  implemented compiler support, witnesses, activation rules, and missing proofs.

## Next compiler and puzzle work

Built-in fixed-width integer masks (`&`, `|`, `^`, `!`) and strict Boolean
`&`/`|`/`^` now lower actual MIR and have separate scalar SMT checks. They are
a prerequisite for broader algorithms, not general RustCrypto support. The
implemented [reviewed SHA bridge](hashing.md) hashes exact atom bytes and returns
32 bytes through strict version-4 runtime bindings. Installed rustc/std and
native-object consistency remain trusted; all proof models reject hash nodes.
The fresh twenty-one-port report records native, five-build differential and ledger
agreement. Local macOS verification passes 459 workspace tests and all-target
Clippy with warnings denied. The hash gate passes 168 corruption controls and
22,285 retained-artifact VM replays. Historical migration gates retain the
earlier reports; a separate closed transition reviews the optimized Rust
changes without changing raw IR, unoptimized artifacts or other languages.
Source-derived proof, error-study,
Lean and site gates also pass. These finite checks do not establish full
Chialisp parity, compiler preservation or production readiness.

The password module adds no compiler exception. It uses ordinary tree matches,
the reviewed hash helper and primitive byte equality. Its 21 instance submissions
are additional observations, separate from the 40 ordinary scenario groups.
The independent password checker checks exact source extraction, the complete
raw-tree relation, quoted curry trees, instance hashes, typed outcomes, state
rollback and finite evidence corruption controls. Retained-byte replay does not
resubmit consensus transactions or prove preservation. Fresh extraction retains
all twenty-one ports' raw IR and compares both modes with the reviewed report.

The cost iteration adds checked constant folding, forward execution-success
facts, total alias frame elision and exact constructor checks. All three closed
Intro expressions now match the original Chialisp bytes and 20-unit VM cost.
The signed 21-puzzle baseline and reviewed optimized outputs remain available
in `corpus/cost-optimization-baseline.json`; source IR and unoptimized artifacts
stay exact. This is finite regression evidence, not compiler preservation.

The password experiment still exposes runtime overhead: optimized Rust is
459 bytes versus Chialisp's 31 and optimized Rue's 213. Its named valid curried
spend costs 8,132 VM units versus 1,321 and 3,937 respectively. Those finite
measurements include the instance wrapper but exclude bundle/condition costs.
Any optimization must retain exact raw input domains and strict failure behavior;
narrowing the original ABI or discarding required work would change the program.

1. Extend the implemented runtime-call IR and checked Rust recursion to further
   published list helpers. `last_inner` now has empty-atom matching and an
   explicit empty/improper-list contract through a visible module adapter.
   The owned `prefix` adapter now exercises a genuine owned result; further
   helpers, byte storage and collection operations remain work. Preserve strict binding/order,
   pre-lowering resource bounds and current model/optimizer exclusions; see
   [runtime calls](runtime-calls.md).
   Continue reducing measured runtime overhead without moving failures past
   divergence or duplicating evaluation. Frame elision reduced the optimized
   `last_inner` adaptation from 743 to 369 bytes; the Chialisp helper adapter is
   71 bytes and Rue is 125. On `(1 2 3)`, optimized Rust cost fell from 25,933
   to 11,568, compared with Chialisp's 2,236 and Rue's 3,814. These are pinned
   measurements checked by `scripts/check-runtime-frames.py`, not general
   performance guarantees. Unused total discriminant bindings and broader
   sharing remain opportunities; first establish totality with bounded analysis.
2. Expand the Cargo SDK beyond transparent domain wrappers to typed
   structures/enums, immutable lists,
   further reviewed primitives, currying, and carefully scoped library support.
   The current `OwnedTree::Atom` borrows immutable bytes. It cannot return a
   newly computed local digest inside an arbitrary condition tail: Rust rejects
   that escaping borrow. An owned atom representation needs explicit native
   and CLVM semantics before porting `p2_delegated_conditions`; replacing its
   cons structure with a proper tuple would change the original contract.
   The pinned `p2_singleton` module is a nearer protocol candidate because its
   fixed three-condition result can own digest arrays. It still needs actual
   Cargo admission, raw-domain differential cases, instance hashes and real
   announcement composition tests. A synthetic announcement counterpart alone
   would not complete the singleton lifecycle.
   The legacy totality cache still treats slice-pattern metadata conservatively,
   so an inlined atom helper can retain a redundant evaluation sequence. Any
   refinement must consult the validated pattern plan rather than declaring
   arbitrary pointer/length metadata total.
3. Migrate actual canonical tutorials and CHIP puzzle implementations, recording
   exact source/license/revision, module identity constraints, behavior, and
   integration compatibility. The current original teaching examples do not
   count as completed CHIP ports; the page/CHIP inventory has 98 source entries.
   The separate example inventory records exact pinned primer/concept snippets
   and workflow gaps. Three closed Intro expressions have finite differential
   validation with their original environment behavior preserved. Their parent
   page remains partial; the other nine programs, variant, and supporting
   workflows remain unrun. Next ports need explicit semantic contracts before
   expanding parameter, list, recursion, cryptographic, or evaluation support.
   The singleton launcher source port does not complete the singleton protocol;
   continuation/lineage, curried-instance compatibility and wallet integration
   require their own witnesses and adversarial complete-spend tests.
4. Extend the implemented source-derived strict byte-argument/Boolean model
   beyond the current exact-inequality contract to broader byte operations,
   cryptographic assumptions, state transitions and compiler preservation.
   SHA remains explicitly outside current models: do not substitute an
   uninterpreted or collision-free function and call that a cryptographic proof.
   Preserve independent whole-environment specifications, nonvacuity and actual
   source-control/native/retained-artifact replays. Preserve the enforced
   [pinned-toolchain boundary](toolchain-trust.md) for custom sysroots and
   loaded replacement standard libraries while extending primitive support.
   Installed toolchain integrity, broader plugin configuration and build inputs
   remain trusted; authenticated complete-build provenance remains open. Connect the
   implemented Lean algorithm models to source-derived semantics rather than treating a manual
   model and finite execution bridge as a compiler theorem.
5. Add larger adversarial fuzzing, upgrade/fork matrices, and independent audits.

## Hosting

The owner made the repository public and enabled Pages. Use GitHub Actions as the
Pages source. `.github/workflows/pages.yml` builds www with pnpm and deploys
www/dist to https://judeallred.github.io/chialisp-oxide/. Verify the actual
successful deployment before reporting the site as live.
