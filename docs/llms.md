# Writing puzzles with chialisp-oxide

> **WIP prototype. Not ready for use. Do not use for real funds or production
> puzzles.** Fixture secret keys are public. Tests and model checks do not
> establish production safety.

This is the guide for **using the SDK**. Compiler contributors also read
[AGENTS.md](https://github.com/judeallred/chialisp-oxide/blob/main/AGENTS.md). The public documentation has a
[curated Markdown index](https://judeallred.github.io/chialisp-oxide/llms.txt),
[complete rendered Markdown](https://judeallred.github.io/chialisp-oxide/llms-full.txt)
and an alternate Markdown link in each page's head.

## Start from an ordinary Cargo library

The facade package is `chialisp-oxide`; import it as `chialisp_oxide`. All project
crates are unpublished. Use a path dependency from a checkout, or a reviewed
Git dependency pinned to a full commit; do not invent a crates.io release.
Use `examples/cargo-puzzles/Cargo.toml` and its real library as the consumer
template. Use the checkout's `rust-toolchain.toml`: currently
`nightly-2026-07-14`, with matching `rustc-dev` and `llvm-tools-preview`.

From the repository root:

```sh
cargo build --locked -p cargo-chialisp-oxide -p chialisp-oxide-driver
export PATH="$PWD/target/debug:$PATH"
cargo check --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo test --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo chialisp-oxide check --manifest-path examples/cargo-puzzles/Cargo.toml --locked
cargo chialisp-oxide build --manifest-path examples/cargo-puzzles/Cargo.toml --locked
```

Mark exported free functions with `#[chialisp_oxide::puzzle]`. Ordinary Cargo
checks native Rust. The additional command checks the reachable CLVM subset
and builds immutable artifact bundles. It is not a `rustc --target clvm`
backend. Preserve Cargo features, build scripts and generated code; never work
around a subset rejection by disabling policy assertions. See
[quickstart](https://judeallred.github.io/chialisp-oxide/start/quickstart/) and
[CLI](https://judeallred.github.io/chialisp-oxide/reference/cli/).

## Use readable, typed policy

This complete export is copied verbatim from
[`puzzles/signature-payment/puzzle.rs`](https://github.com/judeallred/chialisp-oxide/blob/main/puzzles/signature-payment/puzzle.rs)
and checked for freshness by the site tests. It requires the crate-root
[`fixture_keys` module](https://github.com/judeallred/chialisp-oxide/blob/main/puzzles/fixture_keys.rs) used by the real corpus.
Keep that module for simulator experiments only. Replace the named policy
constants and public key deliberately when designing another policy.

```rust
// AGG_SIG_ME binds the owner's signature over the message to this coin.
use crate::fixture_keys;

use chialisp_oxide::{
    AggSigMe, Amount, AssertMyAmount, CreateCoin, Message32, PublicKey, PuzzleHash, agg_sig_me,
    assert_my_amount, create_coin,
};

const OWNER: PublicKey = PublicKey(fixture_keys::OWNER_PUBLIC_KEY);
const COIN_AMOUNT: Amount = Amount::from_mojos(1_000);
const RECIPIENT: PuzzleHash = PuzzleHash([0x42; 32]);

pub type SignedConditions = (AggSigMe, AssertMyAmount, CreateCoin);

#[chialisp_oxide::puzzle]
pub fn puzzle(message: Message32) -> SignedConditions {
    (
        agg_sig_me(OWNER, message),
        assert_my_amount(COIN_AMOUNT),
        create_coin(RECIPIENT, COIN_AMOUNT),
    )
}
```

`Amount::from_mojos` and `Timestamp::from_unix_seconds` make units explicit.
Use `create_coin`, `assert_my_amount`, `agg_sig_me` and nominal types such as
`PuzzleHash`, `PublicKey` and `Message32` instead of magic opcode tuples.
This example fixes the recipient and amount in code; an attacker can select
the message, but consensus still requires the owner's coin-bound signature.
A well-shaped `PublicKey` or `PuzzleHash` is not evidence of ownership or a
recoverable destination. The laboratory records
[real sources and VM/ledger cases](https://judeallred.github.io/chialisp-oxide/lab/signature-payment/).

## Check the supported subset before designing an abstraction

Use supported fixed-width integers, booleans, tuples, fixed arrays, named
constants, ordinary assertions, `if`/`match` and reviewed SDK helpers. Checked
arithmetic rejects overflow; it does not wrap. Native Rust may accept code that
the CLVM frontend rejects. Consult the
[current subset](https://judeallred.github.io/chialisp-oxide/reference/supported-rust/)
instead of inferring support from a successful `cargo check`.

- Prefer plain `assert!(condition)`; formatted panic/assertion messages bring
  unsupported formatting calls into the reachable graph.
- Use supported helpers or explicit supported recursion instead of assuming
  iterator chains, arbitrary trait dispatch or cross-crate libraries lower.
- The reviewed borrowed `Tree` and optional `alloc`/`OwnedTree` representations
  are specific exceptions, not support for arbitrary references or allocators.
- Prefer ordinary implicit `Copy` reuse of SDK values to `.clone()`. Do not
  replace a rejected operation with unchecked arithmetic or permissive parsing.
- Use a small repro for a missing construct. Extend the compiler with explicit
  semantics and regressions rather than pretending a Rust-looking DSL is Rust.

## Separate decoding, commitments and policy

The default [typed ABI](https://judeallred.github.io/chialisp-oxide/reference/abi/)
checks exact argument lists, canonical scalar encodings, widths and shapes at
runtime. Rust checks the native source; callers still supply arbitrary CLVM
trees. `Tree` preserves raw bytes and improper tails. `Cons<H, T>` validates
its declared pair shape, without implying a proper-list tail. Explicit
`environment = "raw"` and nullary `environment = "ignored"` have different
contracts; do not add them merely to suppress a failing vector.

The solution is untrusted. A curried value is committed by that particular
coin's puzzle hash, but its creator can choose a bad value. Check reveals against
committed hashes before applying inner programs. Program identity alone does
not authorize a spender. Hash exact structured encodings; do not concatenate
variable-length fields ambiguously or confuse an atom with a number.

Returned conditions are policy. Review signature binding, coin context,
recipient, output amounts, fees, time units and every branch. Returning an inner
program's conditions delegates authority to it. Typed constructors do not
enforce conservation, signatures, CAT rules or singleton lineage automatically.
Start with [input validation](https://judeallred.github.io/chialisp-oxide/validation/inputs/),
[output validation](https://judeallred.github.io/chialisp-oxide/validation/outputs/)
and the [review checklist](https://judeallred.github.io/chialisp-oxide/validation/review/).

## Test a policy at several boundaries

1. Run native Cargo tests, then the CLVM subset check and builds. Retain both
   optimization modes, artifact bytes, tree hashes and requested Cargo options.
2. Start from `puzzles/<slug>/puzzle.json`. Declare provenance, source dependencies,
   positive and adversarial environments and an independent oracle. Check empty,
   extra and improper arguments; pairs where atoms are expected; padded numbers;
   width limits; policy boundaries; wrong commitments and authorization.
3. Compare native Rust and all five VM builds: Rust off/on, independent Chialisp,
   Rue off/on. Preserve and explain deliberate ABI adapters. Matching sample
   outputs does not make different module hashes interchangeable.
4. For a coin puzzle, run complete signed bundles through chia-wallet-sdk's
   simulator. Inspect exact children and fees. Submit bad signatures to consensus;
   signer preflight is not a consensus test. Require the intended typed error and
   full simulator equality after each rejected submission. Cost/resource failures
   are inconclusive, not evidence that an attack was rejected.
5. Add source-mutation controls when meaningful: removing a policy check should
   expose a counterexample. A passing happy path does not test the missing guard.

For a new corpus port run `python3 scripts/check-port-evidence.py --self-test --replay`
alongside the documented repository gates. Native checking, subset admission,
runtime decoding, ledger execution, bounded models and compiler preservation
are different claims. Do not claim full Chialisp parity, exhaustive security or
production readiness. Unsupported protocol features must remain visible gaps.
