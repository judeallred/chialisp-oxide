# Draft: reusable continuation SDK surface (#16)

**Draft for owner review, 2026-11-16. Not implemented. WIP; not ready for use.**

## Why now

Issue #16's exploration deliverables are mostly met: a design doc, three
concrete policies (batch-one payout queue, batch-two queue, spuds), mined
simulator chains, actual-source mutation controls, generator measurements and
a Testnet11 trial. The open deliverable is *a prototype SDK type*. All three
policies hand-write the same three pieces of scaffolding:

| Repeated piece | batch-one | batch-two | spuds |
|---|---|---|---|
| Successor identity: `Terms` proper-list tree hash via chained `pair_hash`, then `curried_puzzle_hash(module, ·)`, plus `funding_hash` module check | yes | yes | yes |
| Literal 1..=8 tables mapping an integer to its atom hash (`count_atom_hash`, `round_atom_hash`) | yes | yes | yes |
| Literal tables mapping an integer to an `Amount` (`remaining_budget`, `pot`) | yes | yes | yes |
| Hash-chain queue: domain-tagged `item(recipient, next)` and `terminal()` | yes | yes | no |

The registry and precommit recipes also compute curried successor hashes by
hand, so the first two pieces are general-purpose, not continuation-specific.

## What changed since the policies were written

- **Integer casts landed** (bce88e2a, version-7 IR). `remaining as u64` is now
  admitted, so the `Amount` tables can be replaced by checked arithmetic, for
  example `Amount::from_mojos(PAYOUT.mojos() * remaining as u64)`. Cast-capable
  programs skip optional Rust IR refinement and inlining, and current proof
  models reject the capability. Both are acceptable for these policies but
  should be measured.
- **The integer-atom tables still need a primitive.** The subset cannot build
  `[x]` dynamically, so `atom_hash(&[x])` is not expressible. In CLVM the
  strictly decoded integer already *is* its canonical atom, so the tree hash
  is a single `(sha256 (q . 1) x)`. Zero is nil, which hashes the same way.

## Proposal (smallest useful surface)

1. **Canonical unsigned integer atom hash kernel.** Add
   `chialisp_oxide::curry::{u8_atom_hash, u16_atom_hash, u32_atom_hash, u64_atom_hash}`.
   Natively they compute minimal two's-complement bytes then `atom_hash`. In
   CLVM they lower to `sha256 1 x` through a reviewed kernel. This is a
   SHA/kernel admission change, so it follows
   [hash-primitive-boundary.md](agent-rules/hash-primitive-boundary.md): exact
   signature and identity admission, an independent implementation pin,
   padded/negative/boundary vectors (0, 127, 128, 255, 2^63, u64::MAX), strict
   v4 evaluation and explicit proof-model rejection. It is the only new
   compiler-facing piece.
2. **Inherent `tree_hash` methods on facade atoms.** `PuzzleHash`,
   `Message32`, `PublicKey` and `Amount` (via the u64 kernel) get
   `fn tree_hash(self) -> [u8; 32]`. They are ordinary concrete helpers over
   `atom_hash`. No traits or generics are needed, which the subset deliberately
   does not admit.
3. **`chialisp_oxide::continuation` helpers.** These are concrete functions,
   not a generic `Step<State>` type:
   - `list_hash_2/3/4(a, b, ...)`: tree hash of a proper list from element
     hashes (replaces the hand-chained `pair_hash`);
   - `queue_item(domain: &[u8], recipient: PuzzleHash, next: Message32)` and
     `queue_end(domain: &[u8])`;
   - `self_curried(module: PuzzleHash, terms_hash: [u8; 32]) -> PuzzleHash`
     and host-only `funding_hash(selected, module, terms_hash)`.

   An author's `instance_hash` then reads
   `self_curried(module, list_hash_3(module.tree_hash(), head.tree_hash(), u8_atom_hash(remaining)))`.
4. **Defer a derive/attribute macro** that would generate `instance_hash` from
   `type Terms = (...)`. Its expansion would be plain checked Rust, but it
   chooses element hashers from syntactic type names. Revisit after two or
   more corpus users exist.

## First consumer: a corpus port, not another bespoke experiment

Write `ordered-payout` as an ordinary `chialisp-oxide-puzzles` export using
the helpers. It has a variable count (1..=`MAX_ITEMS`, wider than 8 now that
casts work) and batch size one. Its manifest declares provenance, an
independently compiled Chialisp reference, positive and adversarial vectors
(skip, reorder, inject, wrong next state or module, early end, excess
continuation, amount and count boundaries, padded and oversized atoms) and an
oracle in `scripts/port_oracles.py`. It then gets the shared
`check-port-evidence.py` gates instead of new per-experiment scripts. The
multi-block simulator chain reuses the existing stateful-ledger runner.

The retained batch-one, batch-two and spuds experiments stay unchanged as
historical evidence. The new export is a new policy with a new module hash,
not a re-port of them.

## Landing plan (one full recapture each)

1. Kernel, `tree_hash` methods and continuation helpers, with native tests,
   Chialisp differential vectors and kernel admission tests. Land the pending
   `check-evidence-freshness.py` and capture-script guard in the same
   recapture.
2. `ordered-payout` corpus export, manifest, reference, oracle, vectors, SDK
   chain and both-mode costs (bytes and VM cost against the batch-one
   experiment).
3. Fold this draft into [continuations.md](continuations.md). Update #16's
   compiler-feature list: integer atom hash (new); currying, casts and
   optional outputs (existing); no traits, generics or dynamic apply required.

## Explicitly out of scope

Automatic splitting, yield/generator lowering, a generic `Step<State>` enum,
abort or refund branches, reorg recovery and Testnet11 runs through the new
helpers. Cost planning across batch sizes remains host-side
(`plan_costs.py`); a CLI planner belongs to #12.

## Decisions (owner delegated, 2026-11-16)

1. Add the narrow unsigned integer-atom-hash kernel under the hash-primitive
   boundary rules, instead of keeping literal tables.
2. The first consumer is a paired-corpus export with manifest-declared
   evidence, not a new isolated experiment.
3. `MAX_ITEMS` is 64. Both-mode bytes and VM costs are recorded against the
   batch-one experiment so that any regression stays visible.
