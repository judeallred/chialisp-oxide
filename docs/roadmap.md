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
  the canonical `p2_conditions` program factory, with
  side-by-side Chialisp/Rust/Rue sources and separately compiled Rue/Chialisp, the actual Rust functions as native
  oracles, 122 explicit and 7,960 generated integer/tree vectors. Canonical closed
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
- Typed VM failures distinguish program rejection from resource/infrastructure
  failures. Ledger negatives require exact consensus codes; genuine incomplete
  and wrong signatures reach transaction validation instead of signer preflight.
- Source-derived SMT queries with independent specifications, nonvacuity,
  counterexample replay, mutation control, and fail-closed solver handling.
- Shared declared Cargo input/export provenance in validation and proof reports,
  checked against current local files before publishing documentation.
- Four Rue optimizer regressions/fixes with pinned source provenance and upstream
  issues [108](https://github.com/xch-dev/rue/issues/108),
  [109](https://github.com/xch-dev/rue/issues/109),
  [110](https://github.com/xch-dev/rue/issues/110), and
  [111](https://github.com/xch-dev/rue/issues/111).
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

1. Extend the implemented runtime-call IR and checked Rust recursion to exact
   published list helpers. The `last_inner` follow-up needs narrow empty-atom
   matching and an explicit empty/improper-list contract. Accumulating reverse
   or append needs a genuine ownership design. Preserve strict binding/order,
   pre-lowering resource bounds and current model/optimizer exclusions; see
   [runtime calls](runtime-calls.md).
2. Expand the Cargo SDK beyond transparent domain wrappers to typed
   structures/enums, immutable lists,
   hashing/currying intrinsics, and carefully scoped library support.
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
4. Extend formal modeling to byte encodings/cryptographic assumptions, state
   transitions, and compiler preservation. Evaluate Lean for stable IR rules.
5. Add larger adversarial fuzzing, upgrade/fork matrices, and independent audits.

## Hosting

The owner made the repository public and enabled Pages. Use GitHub Actions as the
Pages source. `.github/workflows/pages.yml` builds www with pnpm and deploys
www/dist to https://judeallred.github.io/chialisp-oxide/. Verify the actual
successful deployment before reporting the site as live.
