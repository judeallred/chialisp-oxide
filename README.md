# chialisp-oxide

Compile a supported subset of real Rust into CLVM using Rue's optimizer and
backend. Test puzzles against native Rust, independently compiled Chialisp, and
the chia-wallet-sdk transaction simulator.

## Try it

Install Rust with rustup and Z3 (`brew install z3` on macOS, `apt install z3` on
Ubuntu). The checked-in toolchain installs Rust 1.97.1 and rustc-dev. The pinned
compiler-private driver uses package-scoped `RUSTC_BOOTSTRAP`; ordinary puzzle
source is checked as stable Rust, including its type and borrow checks.

```sh
./scripts/oxide build puzzles/capped-payment/puzzle.rs --output target/capped
./scripts/oxide run target/capped.clvm.hex --solution '(500)'
./scripts/oxide validate --output corpus/validation.json
```

Build emits typed IR, serialized CLVM hex, disassembly, size, and tree hash. The
backend directly integrates Rue compiler source; generated Rue is optional.

## What is validated

Nine paired Rust/Chialisp examples: fixed and capped payments, split payments,
timelocks, one/two signatures, owner/recovery branches, a boolean gate, and a pure
bounded counter. The corpus checks native execution, optimized/unoptimized CLVM,
73 explicit vectors, 3,063 generated integer cases, and 30 complete-spend scenarios.
Some examples are deliberately permissionless. The boolean gate is not
identity/authentication. See each puzzle's security scope before using it.

```sh
cargo build --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 corpus/validate_inventory.py
python3 scripts/check-proofs.py
python3 scripts/check-ir-proofs.py --output corpus/proofs.json
```

The verifier generates SMT from actual source-derived IR, checks independently
specified bounds and acceptance, detects a removed guard, and replays witnesses
through native Rust and both CLVM optimization modes. These are **IR-model
proofs**, not proofs of compiler preservation or absence of all security flaws.

## Documentation

The searchable Astro/Starlight site is in [www](www). It includes tutorials,
source comparisons, a puzzle laboratory, measured bytecode/cost differences,
verification results, supported Rust/ABI references, and a pinned coverage
inventory. Run `pnpm --dir www install && pnpm --dir www dev` for a local preview.
GitHub Pages deployment builds www with pnpm and deploys www/dist through GitHub
Actions at https://judeallred.github.io/chialisp-oxide/.

- [Project goals](goals.md) and [approach](docs/compiler-approach.md)
- [Verification strategy](docs/verification.md)
- [Rust frontend and limitations](crates/oxide-driver/README.md)
- [Source-derived verifier](crates/oxide-verifier/README.md)
- [Simulator and consensus scope](crates/oxide-simulator/README.md)
- [Initial backend experiment](experiments/backend/README.md)
- [Chialisp tutorials/reference](https://chialisp.com/)

The current compiler rejects loops, recursion, allocation, external calls,
references, structs/enums, casts, division/remainder, and other unsupported MIR.
The 98-entry tutorial/CHIP inventory is a roadmap; it is not a claim that all
standard puzzles have been reimplemented. No production security audit has been
completed. The vendored Rue identity-product fix is reported upstream as
[xch-dev/rue#108](https://github.com/xch-dev/rue/issues/108), with a regression test
and patch.
