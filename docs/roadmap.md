# Current status

**WIP prototype — not ready for real funds or production use.** This page is
the short current-status summary. Dated measurements and the record of each
closed transition live in [roadmap-history.md](https://github.com/judeallred/chialisp-oxide/blob/main/docs/roadmap-history.md); the
functionality map is [chialisp-functionality.md](https://github.com/judeallred/chialisp-oxide/blob/main/docs/chialisp-functionality.md).

## Current evidence

The latest local capture covers 31 annotated exports of the real
`chialisp-oxide-puzzles` Cargo library: the retained 23-export
delegated-conditions corpus, the three pinned Using Modules programs through
declared signed 64-bit typed adapters, the original delegated-puzzle fixture
for the reviewed `apply` bridge, the `curry-hash` fixture checked against the
pinned upstream curry include, the canonical Currying page module with its
page-argument instances cross-checked and executed in five builds, the
compiled curry factory with second-stage applications, and the inner-puzzle
commitment fixture whose curried instances run only the committed program.
Its repeated report records 587 named and 25,222 generated inputs across native Rust and five
CLVM builds (Rust off/on, independent Chialisp, Rue off/on). The generic
corpus append root (`scripts/corpus_append.py`, profile
`corpus/corpus-append.json`) admits the current report, restores the retained
[23-export checkpoint](https://github.com/judeallred/chialisp-oxide/blob/main/corpus/checkpoints/p2dc-append-validation.json)
byte-for-byte and projects the older history; new exports extend profile data
instead of adding migration modules.

Forty complete-spend scenario groups run separately for Rust, Chialisp and Rue
through chia-wallet-sdk's simulator (120 ledger scenario runs), plus the
dedicated pay-to-singleton (110 submissions), delegated-conditions (60) and
password-instance (21) ledger suites, each with exact typed consensus
rejections and in-memory rollback checks. The executable error study holds
eleven studies: seven native compile failures, one native overflow check, one
compiled strict-ABI rejection and two negative controls that compile
everywhere. None of these finite checks establishes full Chialisp parity,
compiler preservation or production readiness.

## What works

- Pinned rustc driver with complete analysis and typed MIR; strict checked
  arithmetic, branch semantics, explicit unsupported diagnostics and resource
  limits. The driver is compiler-private and host checked under the enforced
  [pinned-toolchain boundary](https://github.com/judeallred/chialisp-oxide/blob/main/docs/toolchain-trust.md).
- An unpublished Cargo SDK and marker macro: nominal domain wrappers, typed
  condition helpers, borrowed `Tree`, opt-in `OwnedTree`, explicit
  `Cons<First, Rest>` pairs, the reviewed [SHA-256 bridge](https://github.com/judeallred/chialisp-oxide/blob/main/docs/hashing.md), the
  reviewed [`apply` bridge](https://github.com/judeallred/chialisp-oxide/blob/main/docs/dynamic-apply.md) and [currying helpers](https://github.com/judeallred/chialisp-oxide/blob/main/docs/currying.md).
- Typed IR, Rue LIR and static pair codegen with canonical untrusted ABI
  decoding, fixed-width bounds, byte arrays, tuples, booleans, exact arity,
  checked Rust recursion through version-4 function tables, empty-slice
  patterns, primitive byte equality/ordering and bitwise integer operations.
- Optimizations on by default, each with retained before/after baselines:
  checked constants, forward success facts, branch-aware intervals, bounded
  helper inlining, runtime frame elision, guarded projection forwarding and
  compact integer decoding. See [optimization notes](https://github.com/judeallred/chialisp-oxide/blob/main/docs/runtime-calls.md) and the
  history for measured sizes and costs.
- Canonical ports with pinned provenance: three Intro expressions, three Using
  Modules programs, the Currying module, `p2_conditions`, five published
  helpers through explicit adapters, the singleton launcher, the password
  module, pay-to-singleton and delegated conditions. Original teaching
  fixtures are labeled as such and never count as protocol ports.
- Source-derived SMT models with independent specifications, nonvacuity,
  counterexample replay and mutation controls; Lean algorithm theorems with a
  finite execution bridge; explicit acyclic scalar tables. All models reject
  hash and apply nodes and prove nothing about compiler preservation.
- Typed VM failure codes separating rejection from resource and infrastructure
  failures; ledger negatives require exact consensus codes.
- Searchable Astro/Starlight documentation generated from source and evidence,
  published-source discovery across 98 pinned repository trees, and seven
  upstream Rue regressions filed with fixes.

## Next work

1. Further pair-aware optimization to close the remaining size gap on real
   ports: explicit pairs now fold and refine through the owned passes
   (delegated conditions 629 → 425 bytes), but continuation frames in v5
   entries and pair-constructing helpers remain unelided and optimized Rust is
   still several times canonical Chialisp; judge by serialized bytes and full
   spend-bundle cost.
2. Migrate further canonical tutorial and CHIP implementations with explicit
   semantic contracts, module-identity records and complete-spend tests; the
   inventory has 98 source entries and both parent tutorial pages remain
   partial.
3. Generalize per-port evidence tooling onto manifest-driven checkers so new
   ports need data files, not scripts, and keep CI wall time bounded.
4. Extend the source-derived models beyond exact byte contracts toward state
   transitions and, eventually, compiler preservation; keep SHA outside the
   models rather than assuming a collision-free function.
5. Larger adversarial fuzzing, upgrade/fork matrices and independent audits.

## Hosting

The owner made the repository public and enabled Pages. GitHub Actions is the
Pages source: `.github/workflows/pages.yml` builds www with pnpm after the
compiler verification workflow succeeds for the same main commit and deploys
www/dist to https://judeallred.github.io/chialisp-oxide/. Verify the actual
deployment and the live evidence endpoint before reporting the site as live.
