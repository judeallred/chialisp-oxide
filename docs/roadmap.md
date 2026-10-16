# Iteration status

## Working and tested

- Pinned rustc driver: complete analysis/borrow checking, typed MIR, strict
  checked arithmetic/branch semantics, explicit unsupported diagnostics, and
  compiler resource limits. The driver is compiler-private and host checked.
- An unpublished Cargo SDK and marker macro, multiple annotated exports, normal
  native checks/tests, and Cargo-driven CLVM extraction with real features and
  generated source. The paired corpus is the real `oxide-puzzles` Cargo library;
  native execution, CLVM extraction, and proof replay use that same package.
- Nominal one-field transparent wrappers and typed condition helpers in the SDK
  crate; domain swaps fail Rust checking while
  wire representations stay explicit.
- Pure typed IR and direct Rue LIR generation, canonical untrusted ABI decoding,
  checked fixed-width bounds, byte arrays, tuples, booleans, and exact arity.
- Nine teaching examples, independently compiled Chialisp references, the actual
  Rust functions as native oracles, 73 explicit and 3,063 generated vectors.
- Thirty complete-spend scenarios through chia-wallet-sdk's simulator, plus
  standalone regressions for signatures/replay/messages/conservation/rollback.
- Source-derived SMT queries with independent specifications, nonvacuity,
  counterexample replay, mutation control, and fail-closed solver handling.
- Shared declared Cargo input/export provenance in validation and proof reports,
  checked against current local files before publishing documentation.
- Rue identity-product optimizer panic regression/fix, source provenance, plus strict truthiness/failure preservation; upstream issues
  https://github.com/xch-dev/rue/issues/108 and https://github.com/xch-dev/rue/issues/109.
- Searchable Astro/Starlight documentation in www, generated from source/evidence.

## Next compiler and puzzle work

1. Improve expression sharing so checked arithmetic and helper inlining do not
   repeatedly expand subexpressions; preserve strict failures through optimizer
   rewrites. Measure against independent reference compilers as corpus grows.
2. Expand the Cargo SDK beyond transparent domain wrappers to typed
   structures/enums, immutable lists,
   hashing/currying intrinsics, and carefully scoped library support.
3. Migrate actual canonical tutorials and CHIP puzzle implementations, recording
   exact source/license/revision, module identity constraints, behavior, and
   integration compatibility. The current original teaching examples do not
   count as completed CHIP ports; the page/CHIP inventory has 98 source entries.
   The separate example inventory records exact pinned primer/concept snippets
   and workflow gaps. Closed Intro expressions are first candidates; their
   original environment behavior must remain distinct from the typed Rust ABI.
4. Extend formal modeling to byte encodings/cryptographic assumptions, state
   transitions, and compiler preservation. Evaluate Lean for stable IR rules.
5. Add larger adversarial fuzzing, upgrade/fork matrices, and independent audits.

## Hosting

The owner made the repository public and enabled Pages. Use GitHub Actions as the
Pages source. `.github/workflows/pages.yml` builds www with pnpm and deploys
www/dist to https://judeallred.github.io/chialisp-oxide/. Verify the actual
successful deployment before reporting the site as live.
