# Complete generator cost experiment

**WIP prototype. Not ready for use with funds or production puzzles.**

This independent Cargo package measures exact accepted transactions selected by
`cases.json` from the current validated corpus. Adding another measured port is
data work, rather than another per-port benchmark. The compiler and its existing
historical evidence remain unchanged.

It runs three distinct contexts using chia-consensus 0.36.1, clvmr 0.16.2 and
TESTNET11 constants, at each fixture's declared previous transaction height:

- `get_conditions_from_spendbundle`: no signature verification; byte charge is
  `(uncompressed generator length - 2) * 12,000`.
- `run_block_generator2` on the actual uncompressed generator: signature
  verification enabled; charges every generator byte and the quote's 20 VM cost.
- The same generator API on actual backreference serialization: signature
  verification enabled; charges the actual compressed length. The expanded trees
  must be byte-identical. No block references are supplied.

Every generator case checks a valid but unrelated nonempty BLS signature and,
when different from the real aggregate, the identity signature. These must fail
with exactly `BadAggregateSignature`. Running at exactly the measured total must
succeed; one cost unit below must fail with exactly `CostExceeded`. These last
checks are resource controls, never policy rejection evidence. Per-spend costs,
coin IDs and total addition/removal amounts agree across contexts, with the
explicit quote overhead. BLS verification adds no separately metered cost here;
these are resource units, not wall-clock measurements or fee quotes.

```sh
cargo build --manifest-path experiments/spend-costs/Cargo.toml --locked
python3 experiments/spend-costs/check.py --self-test \
  --execute experiments/spend-costs/target/debug/chialisp-oxide-spend-costs \
  --output target/spend-cost-execution
```

The output directory must be new. Exact selected inputs, output, logs and a
binary-hash receipt are preserved there. Re-execution must reproduce all
committed results; `--record` deliberately installs a new capture after all
checks. With no `--execute`, the reader only checks current-source admission,
source/input fingerprints, independently reconstructed uncompressed generator
bytes, bounded backreference decoding/tree equality, cost arithmetic and recorded
control fields. The independent codec has literal value/virtual-stack/path,
malformed-input and expanded-work controls. It does not execute the VM,
verify BLS or authenticate a past run. `--self-test` adds constructed corruption
controls, not new on-chain cases.

The current selection has 35 separately admitted bundles: four delegated
conditions scenarios and three singleton lifecycle phases, each in all five
builds. The two unsafe raw delegated-key choices are intentionally admitted
policy counterexamples. Their costs do not imply safe ownership.

This is a pinned pre-hard-fork-2 TESTNET11 experiment, not a mainnet or fork
matrix. The generator API checks conditions and signatures, not coin-store
existence, spent state, birth heights or the complete ledger. The original SDK
admissions remain separate. Compiler preservation, exhaustive authorization,
security and installed dependency/binary authentication are not established.
