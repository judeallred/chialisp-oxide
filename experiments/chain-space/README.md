# Repeated puzzle reveals on mainnet (#22)

**WIP measurement. Observations of one pinned block range, not a protocol proposal.**

`measure.py` reads a pinned range of mainnet transaction blocks from the public
Coinset full-node API (`https://api.coinset.org`), records a SHA-256 digest of
every response, and analyzes each spend:

- puzzle reveal bytes, and the module hash after uncurrying the standard curried
  form, named from the workspace's pinned `chia-puzzles` 0.20.3 constants;
- module bytes versus curried-argument bytes, and solution bytes;
- module bytes repeated **within** a block (what generator backreferences can
  already remove) and repeated **across** blocks in the range (what a
  hypothetical consensus-level "module by hash" reference would additionally
  remove);
- each block's actual generator size, charged at 12,000 cost per generator byte.

```sh
python3 -B experiments/chain-space/measure.py fetch --start 9500000 --count 150 --output target/chain-space-9500000
python3 -B experiments/chain-space/measure.py analyze --capture target/chain-space-9500000 --report experiments/chain-space/report.json
```

The raw responses (about 32 MB) stay in the ignored capture directory. The
committed [`report.json`](report.json) keeps every response digest, so a rerun
against the same heights can be compared byte for byte.

## What already exists (pinned `chia-consensus` 0.36.1)

- Both `run_block_generator` and `run_block_generator2` deserialize the block
  generator with `node_from_bytes_backrefs`, so CLVM serialization
  backreferences deduplicate repeated subtrees **within one block**.
- With the `SIMPLE_GENERATOR` flag set, a generator that lists references to
  earlier blocks' generators is rejected with `TooManyGeneratorRefs`, so
  cross-block generator references are not available under that rule.
- `cost_per_byte` is 12,000. The `hard_fork_height` of 5,496,000 in that crate is
  its `TEST_CONSTANTS` value; mainnet activation heights live in
  `chia-blockchain` and are not pinned here.

## What the pinned range shows

Heights 9,500,001 to 9,500,432 (150 transaction blocks, 2,726 spends):

- **Solutions, not reveals, dominate.** Solutions are about 90% of raw spend
  bytes. Almost all of that is standard-puzzle solutions carrying their
  delegated condition programs; a few batch spends exceed 150 KB each.
- Module code is about 73% of reveal bytes. Repeats within the same block are
  about 53% of reveal bytes; repeats across blocks add about 20%.
- Actual generators come to about 94% of raw spend bytes in this range, so
  little compression was applied.

The cross-block figure is a projection for a protocol change that does not
exist. It would need a CHIP, a canonical module registry (overlapping #21) and
node storage and validation work, and its benefit is bounded by reveal bytes,
which are about a tenth of spend bytes here. Delegated-puzzle solutions are a
larger target than module reveals.

## Limits

One recent range of 150 transaction blocks is not a trend over time, and the
Coinset API is a third-party provider whose responses are not authenticated
here. Only the outermost curried module is uncurried; inner layers (for example
the standard puzzle inside a CAT) are counted as arguments.
