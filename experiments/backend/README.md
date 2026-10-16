# Experiment 001: reuse Rue directly or emit Rue source?

Run from the repository root:

```sh
cargo test --workspace --locked
cargo run --locked -p chialisp-oxide-backend-experiment > /tmp/oxide-backend-report.json
python3 scripts/check-proofs.py
```

The first program is an intentionally unauthenticated capped-payment policy. It
accepts one integer atom in a proper list, in the range 0–1000, and emits a fixed
recipient CREATE_COIN. Do not use it to lock funds. This is backend infrastructure,
not a Rust-source compiler or a claim of puzzle security.

## Observed results

Measured locally with Rust 1.97.1, pinned Rue revision
245c107a8e795229ea08097142f005a4c0d91075, clvmr 0.21.0, MEMPOOL_MODE, and input
`(1)`. Costs are VM execution only. Static-pair optimization is disabled in both
paths, and Rue automatic inlining is held enabled across its two builds.

- Handwritten reference CLVM: 214 bytes; cost 3,046.
- Rue source without LIR optimization: 315 bytes; cost 5,063.
- Rue source with LIR optimization: 277 bytes; cost 3,989.
- Direct LIR without optimization: 214 bytes; cost 3,046.
- Direct LIR with optimization: 188 bytes; cost 2,749.

Direct unoptimized LIR happens to be byte-identical to the handwritten reference.
The optimized paths have different tree hashes despite matching tested behavior.
The checked-in `report.json` contains the complete artifacts and sampled runs;
regenerate it with the command above before updating measurements.

The direct route is promising for this policy. It already uses upstream compiler
source as pinned Rust dependencies and requires no Rue text round trip. No
upstream files are copied or modified yet. Keep the source route as a differential
oracle while extending to functions, environments, control flow, and checked
integer arithmetic; those features could change the tradeoff.

## Verification performed

Eight tests cover 1,041 consecutive integers, 256 random signed 64-bit inputs,
256 random valid amounts, 512 random bounded-depth solution trees, malformed
vectors, large integers, padded encodings, and cost exhaustion. Each normal vector
runs through all five backends and compares full serialized outputs against the
native numeric policy and independent expected condition construction. Unexpected
cost exhaustion fails the comparison. Randomized cases vary between runs;
proptest shrinks and persists failures for regression.

A mutation test changes the executable reference's upper bound and confirms that
amount 1001 becomes incorrectly accepted. Z3 separately checks two model-level
claims over unbounded integers and detects a missing-guard mutation. See
[verification strategy](../../docs/verification.md) for scope and proof gaps.

## Next experiment

Introduce the smallest typed IR and rustc-driver extraction that compiles a
checked Rust function to this same backend, including negative Rust compile cases.
Keep ABI decoding explicit and reject unsupported MIR. Extend the corpus before
claiming Rust ownership, integer, or library semantics are preserved. Then add a
compiled Chialisp reference and simulator-level authorization tests.
