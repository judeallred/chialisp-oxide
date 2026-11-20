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
python3 -B experiments/chain-space/measure.py analyze --capture target/chain-space-9500000 --report experiments/chain-space/ranges/9500000/report.json
python3 -B experiments/chain-space/measure.py storage --capture target/chain-space-9500000 --report experiments/chain-space/ranges/9500000/storage.json
```

The raw responses (about 32 MB) stay in the ignored capture directory. The
committed `ranges/<start>/report.json` files keep every response digest, so a rerun
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

## Local storage (#23)

`measure.py storage` compares a local spend table (reveal plus solution bytes
per spend) stored raw, zstd-compressed (level 3), and as a module-deduplicated
store: each distinct uncurried module once, plus a 32-byte module hash, curried
arguments and solution per spend. Every reveal is rebuilt byte for byte from
the stored module and arguments. See `ranges/<start>/storage.json`.

Over the same range, zstd alone shrinks the table to about 73% of raw size and
module deduplication alone to about 93%, but deduplicating before zstd saves
only about 0.2% more: general compression already captures almost all repeated
module bytes. The range has only 13 distinct modules. This covers a spend table
only, not a node database, block bodies or byte-exact generator reconstruction
with its backref encoding, and the full node's actual schema and compression are
not pinned here.

## Three ranges (`trend.json`)

The same measurements over three pinned 150-transaction-block ranges:

| Range start | Spends | Generator / raw spend bytes | Solutions / raw | In-block repeat / reveal | Cross-block repeat / reveal | zstd / raw | Dedup then zstd / zstd |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 5,000,000 | 32,231 | 101.0% | 28.4% | 71.4% | 3.2% | 9.5% | 98.5% |
| 7,500,000 | 6,965 | 18.1% | 8.5% | 27.8% | 2.9% | 5.0% | 96.6% |
| 9,500,000 | 2,726 | 94.5% | 90.1% | 52.9% | 19.5% | 72.7% | 99.8% |

Whether a block's generator was compressed depends on the block it came from,
not only on its date: the 7,500,000 range was heavily compressed and the recent
range barely. Across all three, deduplicating modules before zstd saves at most
a few percent, and the cross-block share is small except in the recent range,
where large standard solutions dominate. Three ranges are samples, not a
continuous trend.

## Limits

Three ranges of 150 transaction blocks each are samples, not a continuous trend, and the
Coinset API is a third-party provider whose responses are not authenticated
here. Only the outermost curried module is uncurried; inner layers (for example
the standard puzzle inside a CAT) are counted as arguments.
