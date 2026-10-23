# Current status

**WIP prototype — not ready for real funds or production use.** This page is
the short current-status summary. Dated measurements and the record of each
closed transition live in [roadmap-history.md](https://github.com/judeallred/chialisp-oxide/blob/main/docs/roadmap-history.md); the
functionality map is [chialisp-functionality.md](https://github.com/judeallred/chialisp-oxide/blob/main/docs/chialisp-functionality.md).

## Current evidence

The latest local capture covers 35 annotated exports of the real
`chialisp-oxide-puzzles` Cargo library: the retained 23-export
delegated-conditions corpus, the three pinned Using Modules programs through
declared signed 64-bit typed adapters, the original delegated-puzzle fixture
for the reviewed `apply` bridge, the `curry-hash` fixture checked against the
pinned upstream curry include, the canonical Currying page module with its
page-argument instances cross-checked and executed in five builds, the
compiled curry factory with second-stage applications, and the inner-puzzle
commitment fixture whose curried instances run only the committed program, and
the original boomerang payment with fixed terms committed before funding,
the original precommit claim/registry pair, and the canonical genesis-by-coin-ID CAT TAIL.
Its repeated report records 807 named and 27,281 generated inputs across native Rust and five
CLVM builds (Rust off/on, independent Chialisp, Rue off/on). The generic
corpus append root (`scripts/corpus_append.py`, profile
`corpus/corpus-append.json`) admits the current report, restores the retained
[23-export checkpoint](https://github.com/judeallred/chialisp-oxide/blob/main/corpus/checkpoints/p2dc-append-validation.json)
byte-for-byte and projects the older history; new exports extend profile data
instead of adding migration modules.

Sixty-two complete-spend scenario groups run for each selected Rust, Chialisp or Rue build
through chia-wallet-sdk's simulator (186 ledger scenario runs), plus the
dedicated pay-to-singleton (110 submissions), delegated-conditions (60) and
password-instance (21) and boomerang-instance (145) ledger suites, each with exact typed consensus
rejections and in-memory rollback checks. A separate actual Rust source-control
experiment uses nine native output observations, eighteen primary-VM runs and
eighteen SDK submissions: removing a returned authorization or deadline
condition admits a forbidden spend under the mutant’s own committed instance.
These are policy mistakes that ordinary Rust, Clippy and the subset permit,
not additional errors Rust makes impossible. The executable error study holds
eleven studies: seven native compile failures, one native overflow check, one
compiled strict-ABI rejection and two negative controls that compile
everywhere. None of these finite checks establishes full Chialisp parity,
compiler preservation or production readiness.

The separate precommit source-control lane records four native output
observations, eight primary-VM observations and eight SDK submissions. Removing
the minimum-age condition permits an early claim; removing the returned
signature condition permits an unsigned claim. Each mutant is funded under its
own reveal and paired with an independently compiled Chialisp counterpart.
These observations remain separate from the base precommit pair's 66 grouped
scenario runs and 96 transaction attempts. That lane includes six repetitions
of the Chialisp-only naive baseline, accounting for 12 of those attempts; the
paired policies account for the other 60 groups and 84 attempts.

The [CAT TAIL tutorial](https://judeallred.github.io/chialisp-oxide/tutorials/cat-genesis-tail/)
adds five local CAT2 lifecycle groups and 75 SDK submissions: 20 accepted,
35 exact typed consensus rejections and 20 runtime umbrellas. Each build funds
its own TAIL asset. Its separate actual Rust source-control packet records
39 native witnesses, 115 raw VM observations, 46 SDK submissions and 50
component VM observations. Neither lane establishes Sage or live Testnet11
acceptance; the canonical CAT2 outer layer remains a host counterpart.

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
- Bounded, output-only standard Rust `Option<T>` with nil/singleton encoding
  and optional `Cons` tails, described in [optional-lists.md](https://judeallred.github.io/chialisp-oxide/reference/optional-lists/).
  Version 6 preserves strict construction and helper passing; it does not add
  optional inputs, general enums, `Vec` or trait APIs. This capability has
  separate constructor regressions. The original
  [boomerang payment](https://judeallred.github.io/chialisp-oxide/tutorials/boomerang/)
  uses the capability with its own committed-instance and ledger evidence.
- An original [precommit pair](https://github.com/judeallred/chialisp-oxide/blob/main/docs/precommit.md)
  (`precommit-claim`, `precommit-registry`) for
  [#17](https://github.com/judeallred/chialisp-oxide/issues/17): a salted,
  owner-bound commitment that a one-name registry slot can accept only after a
  two-block age, with a permissionless refund after eight blocks. Each recorded
  ledger build of one side runs eleven scenarios against the independently compiled
  Chialisp reference of the other side. Named `BlockHeight`/`BlockAge`,
  announcement and own-coin-ID helpers make unit and identifier swaps Rust type
  errors. A reusable `Precommit<T>` helper, a message-based variant and the
  dynamic-registry case remain open.
- Optimizations on by default, each with retained before/after baselines:
  checked constants, forward success facts, branch-aware intervals, bounded
  helper inlining, runtime frame elision, guarded projection forwarding and
  compact integer decoding. See [optimization notes](https://github.com/judeallred/chialisp-oxide/blob/main/docs/runtime-calls.md) and the
  history for measured sizes and costs. Optional-capable programs currently
  skip Rust IR refinement and inlining; no cost improvement is claimed for
  optional outputs.
- Canonical ports with pinned provenance: three Intro expressions, three Using
  Modules programs, the Currying module, `p2_conditions`, five published
  helpers through explicit adapters, the singleton launcher, the password
  module, pay-to-singleton, delegated conditions and the genesis-by-coin-ID CAT
  TAIL. Original teaching
  fixtures are labeled as such and never count as protocol ports.
- Source-derived SMT models with independent specifications, nonvacuity,
  counterexample replay and mutation controls; Lean algorithm theorems with a
  finite execution bridge; explicit acyclic scalar tables. All models reject
  hash, apply and optional capability and prove nothing about compiler
  preservation.
- Typed VM failure codes separating rejection from resource and infrastructure
  failures; ledger negatives require exact consensus codes.
- Bounded source receipts in default Cargo builds, including exact raw
  selected-library reads, generated files and named launcher snapshots. Explicit
  `--artifact-only` builds retain the older bundle format. The offline registry
  imports complete immutable bundles and verifies package, blob, tree and
  structural ABI identities. Local `add` selects exact packages into a portable
  dependency lock; the host consumer executes retrieved bytes. These records do not authenticate sources,
  capture a complete build graph or prove source/IR/CLVM equivalence.
- Bounded offline reveal inspection with exact canonical uncurry, a retained
  Coinset-reported mainnet standard puzzle checked through independent SDK
  codecs, and explicit retrieved-byte standard and `p2_puzzle_hash` host bindings. Host VM
  conditions do not establish signatures, chain authentication or complete spends.
- Explicit-lock generation of local Rust helpers that construct and apply the
  retrieved program. These helpers use the ordinary checked source path and
  preserve an opaque whole-tree environment. The bounded generation profile,
  native Rust checks, subset checks and actual paired CLVM builds are separate;
  generated comments and names grant no nominal ABI or compiler trust.
- A normal Cargo standard-wallet wrapper applying the exact registry-retrieved
  module, with source-aware bundles in both compiler modes. Its separate SDK
  scenario records 18 submissions: three accepted spends, twelve exact typed
  rejections and three runtime umbrellas; one native output and six primary-VM
  observations stay separate. These are local simulator runs, not live spends.
- Searchable Astro/Starlight documentation generated from source and evidence,
  published-source discovery across 98 pinned repository trees, and seven
  upstream Rue regressions filed with fixes.

## Next work

The owner's October 20 priority change puts authoring, validation, puzzle coverage
and documentation ahead of further optimization. The
[open issues](https://github.com/judeallred/chialisp-oxide/issues) are part of this
backlog; this ordering does not mark their acceptance criteria complete.

1. Keep agent discovery
   ([#25](https://github.com/judeallred/chialisp-oxide/issues/25)) current: the
   [authoring guide](https://judeallred.github.io/chialisp-oxide/reference/agent-guide/)
   and generated Markdown have deterministic freshness/discovery checks, and
   actual Codex and Claude Code fresh-clone trials are recorded in
   [the discovery report](https://github.com/judeallred/chialisp-oxide/blob/main/docs/agent-documentation.md).
   Those trials are observations, not a guarantee for every tool. The new
   [input](https://judeallred.github.io/chialisp-oxide/validation/inputs/),
   [output](https://judeallred.github.io/chialisp-oxide/validation/outputs/)
   and [review](https://judeallred.github.io/chialisp-oxide/validation/review/)
   guides address [#24](https://github.com/judeallred/chialisp-oxide/issues/24)
   using recorded corpus witnesses. Chialisp-specific highlighting
   ([#8](https://github.com/judeallred/chialisp-oxide/issues/8)) has pinned
   official provenance and actual token/contrast checks.
2. Make the tooling useful end to end: the
   [offline registry prototype](https://judeallred.github.io/chialisp-oxide/reference/package-registry/)
   imports immutable Cargo bundles and verifies exact package, artifact and
   structural ABI identities without running a compiler or VM. Work on
   [#21](https://github.com/judeallred/chialisp-oxide/issues/21) now has reviewed
   standard/p2-puzzle-hash host bindings and a Cargo standard-wallet wrapper
   with actual local SDK spends. Arbitrary nominal dependency ABIs and broader
   registry integration remain open; source attachments and build declarations
   alone do not establish compilation or source authenticity. The
   [local lifecycle](https://judeallred.github.io/chialisp-oxide/reference/lifecycle/)
   now connects fresh checks to a shared simulator CLI/GUI. Continue the explicit-network lifecycle
   ([#12](https://github.com/judeallred/chialisp-oxide/issues/12)). Live wallet
   demonstrations require actual testnet evidence, not simulated transaction IDs.
3. The local CAT issuance and transfer workflow
   ([#13](https://github.com/judeallred/chialisp-oxide/issues/13)) now has paired
   Cargo exports and five-build CAT2 lifecycle evidence. Funded Testnet11
   issuance and Sage send/receive evidence remain open. Continue with action-layer
   demonstrations ([#14](https://github.com/judeallred/chialisp-oxide/issues/14)),
   the XCHandles case study ([#15](https://github.com/judeallred/chialisp-oxide/issues/15)),
   then authenticated continuations ([#16](https://github.com/judeallred/chialisp-oxide/issues/16)).
   Build on the original precommit/wait/reveal demo
   ([#17](https://github.com/judeallred/chialisp-oxide/issues/17)) while retaining
   its canonical-port and reusable-abstraction gaps. Derive new
   capabilities from these concrete ports; preserve honest gaps and model scope.
4. Use that foundation for the pot game
   ([#10](https://github.com/judeallred/chialisp-oxide/issues/10)), raffle
   ([#11](https://github.com/judeallred/chialisp-oxide/issues/11)) and bandit
   ([#20](https://github.com/judeallred/chialisp-oxide/issues/20)), with explicit
   randomness, withholding, liveness and refund assumptions. Build the Solana
   ([#18](https://github.com/judeallred/chialisp-oxide/issues/18)) and EVM
   ([#19](https://github.com/judeallred/chialisp-oxide/issues/19)) learning tracks
   from working examples, explaining differences between accounts and coins.
5. Continue manifest-driven evidence tooling, broader adversarial fuzzing,
   fork matrices and state-transition models. Keep SHA outside proofs that do
   not model it, and compiler preservation separate from model equivalence.
6. Keep optimization ([#6](https://github.com/judeallred/chialisp-oxide/issues/6))
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
