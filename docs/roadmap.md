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

The owner's October 20 priority change puts authoring, validation, puzzle coverage
and documentation ahead of further optimization. The
[open issues](https://github.com/judeallred/chialisp-oxide/issues) are part of this
backlog; this ordering does not mark their acceptance criteria complete.

1. Finish the external-tool acceptance for agent discovery
   ([#25](https://github.com/judeallred/chialisp-oxide/issues/25)): the
   [authoring guide](https://judeallred.github.io/chialisp-oxide/reference/agent-guide/)
   and generated Markdown now have deterministic freshness/discovery checks,
   but two actual fresh-clone agent-tool trials remain outstanding. The new
   [input](https://judeallred.github.io/chialisp-oxide/validation/inputs/),
   [output](https://judeallred.github.io/chialisp-oxide/validation/outputs/)
   and [review](https://judeallred.github.io/chialisp-oxide/validation/review/)
   guides address [#24](https://github.com/judeallred/chialisp-oxide/issues/24)
   using recorded corpus witnesses. Chialisp-specific highlighting
   ([#8](https://github.com/judeallred/chialisp-oxide/issues/8)) has pinned
   official provenance and actual token/contrast checks.
2. Build the reclaimable boomerang payment
   ([#9](https://github.com/judeallred/chialisp-oxide/issues/9)), including both
   authorization paths, boundary probes and complete simulator spends.
3. Make the tooling useful end to end: a local package registry with source and
   artifact identity ([#21](https://github.com/judeallred/chialisp-oxide/issues/21)),
   then the explicit-network CLI/GUI lifecycle
   ([#12](https://github.com/judeallred/chialisp-oxide/issues/12)). Live wallet
   demonstrations require actual testnet evidence, not simulated transaction IDs.
4. Expand stateful composition with canonical CAT issuance
   ([#13](https://github.com/judeallred/chialisp-oxide/issues/13)), the XCHandles
   case study ([#15](https://github.com/judeallred/chialisp-oxide/issues/15)),
   authenticated continuations ([#16](https://github.com/judeallred/chialisp-oxide/issues/16)),
   precommit/wait/reveal ([#17](https://github.com/judeallred/chialisp-oxide/issues/17))
   and action-layer demonstrations
   ([#14](https://github.com/judeallred/chialisp-oxide/issues/14)). Derive new
   capabilities from these concrete ports; preserve honest gaps and model scope.
5. Use that foundation for the pot game
   ([#10](https://github.com/judeallred/chialisp-oxide/issues/10)), raffle
   ([#11](https://github.com/judeallred/chialisp-oxide/issues/11)) and bandit
   ([#20](https://github.com/judeallred/chialisp-oxide/issues/20)), with explicit
   randomness, withholding, liveness and refund assumptions. Build the Solana
   ([#18](https://github.com/judeallred/chialisp-oxide/issues/18)) and EVM
   ([#19](https://github.com/judeallred/chialisp-oxide/issues/19)) learning tracks
   from working examples, explaining differences between accounts and coins.
6. Continue manifest-driven evidence tooling, broader adversarial fuzzing,
   fork matrices and state-transition models. Keep SHA outside proofs that do
   not model it, and compiler preservation separate from model equivalence.
7. Keep optimization ([#6](https://github.com/judeallred/chialisp-oxide/issues/6))
   and the chain/node storage studies
   ([#22](https://github.com/judeallred/chialisp-oxide/issues/22),
   [#23](https://github.com/judeallred/chialisp-oxide/issues/23)) at a lower priority.
   The signed generator benchmark remains the baseline: the ordinary delegated
   owner spend costs 9,025,107 units in optimized Rust versus 6,530,606 for
   Chialisp with backreferences. Continue measuring full spends as well as
   module bytes; retain current correctness and regression gates.

## Hosting

The owner made the repository public and enabled Pages. GitHub Actions is the
Pages source: `.github/workflows/pages.yml` builds www with pnpm after the
compiler verification workflow succeeds for the same main commit and deploys
www/dist to https://judeallred.github.io/chialisp-oxide/. Verify the actual
deployment and the live evidence endpoint before reporting the site as live.
