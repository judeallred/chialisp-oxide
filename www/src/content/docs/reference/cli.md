---
title: Compiler commands
description: Build and run serialized CLVM, compare optimization modes, and regenerate the full validation report.
---

`./scripts/oxide` builds the pinned driver and CLI before invoking the command.
Run it from the checked-out repository. For repeated local experiments after
a successful build, `target/debug/oxide` invokes the same CLI directly.

## Build

```sh
./scripts/oxide build puzzles/capped-payment/puzzle.rs --output target/capped
```

The `--output` value is a file prefix. The command emits:

- `.ir.json`: typed pure IR after Rust checking and supported-feature lowering.
- `.clvm.hex`: serialized CLVM, suitable for the `run` command.
- `.clvm.txt`: disassembly of the generated program.
- `.artifact.json`: serialized byte count, tree hash, backend options, hex, and
  disassembly. The artifact is also printed to standard output.

Disable Rue LIR optimization for a comparison build:

```sh
./scripts/oxide build puzzles/capped-payment/puzzle.rs --output target/capped-unoptimized --no-optimize
```

Both modes retain ABI checks and checked integer semantics. Turning off the
optimizer does not disable Rust analysis or change the intended puzzle policy.

## Run

```sh
./scripts/oxide run target/capped.clvm.hex --solution '(500)' --max-cost 1000000
./scripts/oxide run target/capped.clvm.hex --solution '(1001)' --max-cost 1000000
```

The first input satisfies the cap; the second must reject. `run` prints a JSON
execution report with outcome, returned successful cost when available, and
diagnostics. Rejection exits with status 2. The default cost ceiling is
1,000,000; set it explicitly in reproducible experiments.

A CLVM result is not a ledger transaction. `run` cannot establish that
`AGG_SIG_ME`, amount assertions, or time conditions are satisfied in a real
spend bundle. Use the simulator scenarios and corpus validation for that level.

## Validate the registered corpus

```sh
./scripts/oxide validate --output corpus/validation.json
```

This extracts IR from each registered Rust fixture, compiles optimized and
unoptimized programs, compiles its Chialisp reference independently, compares
named and generated inputs against native execution, and validates the actual
optimized coin puzzles in the ledger simulator. A divergence fails the run.

The generated report embeds the exact sources and runtime versions used. The
documentation build refuses stale source strings, changed manifest cases,
disagreed backend outcomes, failed ledger checks, or stale formal source hashes.

## Inspect frontend IR directly

The lower-level driver selects the `puzzle` free function by default:

```sh
cargo build -p oxide-driver --locked
target/debug/oxide-driver puzzles/bounded-counter/puzzle.rs --output target/counter.ir.json
```

The driver accepts `--entry NAME` for another entry function. That is a driver
option, not a supported `oxide build` flag. Keep rustc internals isolated in the
driver when investigating diagnostics or extending the subset.
