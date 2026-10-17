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
- Ten original teaching examples, three exact canonical Intro expressions, and
  the canonical `p2_conditions` program factory and four pinned published helpers
  through explicit module adapters, with
  side-by-side Chialisp/Rust/Rue sources and separately compiled Rue/Chialisp, the actual Rust functions as native
  oracles, 282 explicit and 14,088 generated integer/tree/byte vectors. Canonical closed
  expressions explicitly ignore environments; default typed ABIs remain strict.
- Thirty-one complete-spend scenario groups run separately for Rust, Chialisp, and Rue
  (93 ledger scenario runs) through chia-wallet-sdk's simulator, plus
  standalone regressions for signatures/replay/messages/conservation/rollback.
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
a prerequisite for broader algorithms, not support for RustCrypto or SHA256.
Hashing needs the explicit reviewed primitive boundary described in
[the compiler approach](compiler-approach.md#cryptographic-primitives-and-native-identity).

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
   hashing/currying intrinsics, and carefully scoped library support.
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
   expanding parameter, list, recursion, hashing, or evaluation support.
4. Extend the implemented source-derived strict byte-argument/Boolean model
   beyond the current exact-inequality contract to broader byte operations,
   cryptographic assumptions, state transitions and compiler preservation.
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
