---
title: Errors that Rust makes impossible
description: Execute Chialisp counterexamples that pass limited tests, inspect real Rust compiler failures, and distinguish type guarantees from policy correctness.
---

Four faulty Rust programs in this study cannot compile with their declared types
and names. Their Chialisp counterparts compile, pass two deliberately limited
exact-output tests each, and then fail a distinguishing input. A fifth study
shows a policy bug that **Rust also compiles**.

“Impossible” has a precise scope here: these ill-typed or unresolved programs
cannot become executable Rust through the tested compiler. It does not mean
that every program written in Rust implements its intended policy. Deliberate
conversions, weaker types, wrong constants, missing authorization and an
incorrect boolean expression can all reintroduce bugs.

These are original, intentionally faulty teaching fixtures, separate from the
canonical tutorial ports. The [executed error-study report](../../reports/error-study.json)
records every asserted result, actual VM output and cost, Rust diagnostic,
corrected native test, Rue outcome, and declared input SHA-256. The fixture
manifest is `experiments/error-study/studies.json`.

## What actually ran

The study uses the pinned real `rustc 1.99.0-nightly`, the actual
`chialisp-oxide` SDK crate, Chia's `chialisp 0.4.6` classic compiler, and
`clvmr 0.21.0` with `MEMPOOL_MODE`. Each Chialisp fixture is independently
compiled and executed. Rue is separately compiled at the project's pinned
revision with LIR optimization disabled and enabled; its measured outcomes
are reported below.

The Rust failures come from normal rustc analysis, not a source-text matcher.
Corrected fixtures compile as real native test binaries and run eight tests
in total. **These host-only Rust studies do not establish Rust-to-CLVM
preservation or transaction admission.** The puzzle laboratory and formal
pages describe those separate checks.

The weak suites are concrete, reproducible and intentionally incomplete. Each
asserts the entire expected output tree or rejection, rather than accepting
anything that executes. Passing two such tests says nothing about an untested
branch, a different identifier value, or an excluded approval code.

## Coin ID used as a payment destination

The Chialisp bug returns a coin ID where the policy requires a recipient puzzle
hash. Both values are 32-byte atoms:

```clojure
(mod (RECIPIENT COIN_ID AMOUNT)
  (list (list 51 COIN_ID AMOUNT)))
```

The limited tests use identical recipient and coin-ID fixtures: 32 bytes of
`0x11` with amount 7, then 32 bytes of `0x22` with amount 9. Both exact-output
assertions pass. The missed witness uses recipient `0x42…42`, coin ID
`0x99…99`, and amount 7. The program still executes successfully but pays the
wrong destination in its returned condition.

The Rust attempt uses the SDK's distinct nominal domain types:

```rust
pub fn puzzle(
    _recipient: PuzzleHash,
    coin_id: CoinId,
    amount: Amount,
) -> (CreateCoin,) {
    (create_coin(coin_id, amount),)
}
```

Real rustc reports **E0308**, expected `PuzzleHash`, found `CoinId`. The
corrected native test deliberately uses different bytes for the two identities
and checks the opcode, recipient and amount.

This protection depends on choosing distinct domain types. Both fields are
public: explicitly writing `PuzzleHash(coin_id.0)` can recreate the mistake.
The tested Rue counterpart uses `Bytes32` for both meanings and reproduces the
missed witness in both optimizer modes. A second, actually compiled Rue control defines separate `StudyCoinId` and
`StudyPuzzleHash` structs. Rue rejects the same direct swap in both modes with
“Cannot assign `StudyCoinId` to `StudyPuzzleHash`.” This confirms analogous
nominal domain protection in Rue; the comparison is about these concrete API
encodings, not a unique capability proved for Rust.

## Untested branch returns a condition instead of a condition list

The true branch returns `((51 recipient amount))`. The false branch omits one
list layer:

```clojure
(mod (PAY RECIPIENT AMOUNT)
  (if PAY
    (list (list 51 RECIPIENT AMOUNT))
    (list 51 RECIPIENT AMOUNT)))
```

Both limited tests select the true branch, with amounts 7 and 9. The witness
selects the false branch. The VM returns `(51 recipient 7)`, which differs from
the independently specified `((51 recipient 7))`. This is an observed return-tree
error; this experiment does not submit the malformed condition list as a spend.

The Rust function declares `-> (CreateCoin,)` but its erroneous else branch
returns `CreateCoin`. rustc reports **E0308** even though no test has selected
that branch. Rust requires an `if` expression's returning branches to agree
with the declared result type. [Rust Reference: if expressions](https://doc.rust-lang.org/reference/expressions/if-expr.html#type)

The corrected native test checks both branches. Rue also rejects the faulty
source in both modes: its diagnostic says the union of `CreateCoin` and a
one-element condition list cannot be assigned to `List<Condition>`. This is a
shared static-type benefit, not a Rust-only advantage.

## Approval code two is accidentally treated as true

This policy permits only approval code `1`. The faulty Chialisp uses truthiness:

```clojure
(mod (APPROVED RECIPIENT AMOUNT)
  (if APPROVED (list (list 51 RECIPIENT AMOUNT)) (x)))
```

The limited suite verifies code `1` produces the expected payment and code `0`
rejects. Both tests pass. The missed witness supplies `2`, which also produces
a payment even though the independent policy expects rejection.

The equivalent Rust attempt writes `if approved` with `approved: u64`.
rustc reports **E0308**, expected `bool`, found `u64`. Rust does not implicitly
convert an integer condition into a boolean. [Rust E0308](https://doc.rust-lang.org/error_codes/E0308.html)

Rust makes that implicit conversion impossible; **it does not choose the right
replacement predicate**. `approved != 0` would compile and reproduce the bug.
The corrected fixture explicitly asserts `approved == 1` at runtime. Its three
native tests cover codes `1`, `0` and `2`. This is not an ABI-decoder test or a
compile-time proof of the approval policy.

Rue also rejects the attempted `Int` condition in both modes, requiring a type
guard before comparison with `Bool`. This protection is shared.

## A misspelled name survives an untested classic Chialisp branch

The intended alternative branch returns `AMOUNT`; the faulty source says `amout`:

```clojure
(mod (PRIMARY AMOUNT)
  (if PRIMARY AMOUNT amout))
```

The two limited tests select the primary branch and return 7 and 9 as expected.
The missed input `(() 7)` selects the alternative branch. In the tested classic
compiler path, the unresolved bare name becomes the atom `amout`: the VM returns
serialized hex `85616d6f7574` instead of integer 7 (`07`). This is measured classic
compiler behavior, not a claim about every Chialisp dialect or linter.

The Rust alternative branch also says `amout`, which has no declaration.
rustc reports **E0425**, cannot find value `amout` in this scope. Name resolution
checks this branch before any test runs. The corrected native test evaluates
both branches. [Rust E0425](https://doc.rust-lang.org/error_codes/E0425.html)

Rue rejects the same undeclared symbol in both modes. A misspelling that happens
to name another valid variable can still compile in either typed language;
strong domain types and distinguishing tests remain useful.

## Negative control: missing policy cap still compiles

The independently stated policy accepts counters below 1000 and returns the
increment. Both faulty implementations forget the guard:

```clojure
(mod (COUNTER) (+ COUNTER 1))
```

```rust
pub fn puzzle(counter: u64) -> u64 {
    counter + 1
}
```

The limited Chialisp tests assert `0 → 1` and `7 → 8`. Both pass. The missed
witness supplies `1000`: execution returns `1001` instead of rejecting.
**Rust and Rue both compile this bug.** Native Rust's limited test also passes;
the expanded native assertion `puzzle(1000) <= 1000` fails when actually executed.
Rue's compiled output reproduces the failure with optimization off and on.

The corrected Rust fixture adds `assert!(counter < 1000)`. Its native tests check
the accepted boundary and the rejected witness. The stronger property depends
on the specification and guard, not on `u64` alone. The separate
[bounded-counter model](../formal/) checks an independently stated property of
actual source-derived IR and detects a removed guard; its proof scope still
excludes general compiler preservation.

## Guarantees beyond this study

The default project ABI rejects malformed list shapes, noncanonical booleans,
padded numbers, out-of-range integers and incorrect fixed byte lengths at
**runtime**. Those are compiler-generated boundary checks; an attacker can
construct a malformed tree after Rust compilation. The explicit ignored ABI
for genuinely nullary exports is a separate contract.

Checked arithmetic, strict failure propagation and lazy branches are backend
obligations with optimizer-on/off regressions. Native Rust overflow behavior
also depends on compiler settings; this study explicitly enables overflow
checks. [Rust Reference: overflow](https://doc.rust-lang.org/reference/expressions/operator-expr.html#overflow)

A wrong well-typed recipient, missing signature, incorrect commitment, missing
conservation condition or unsafe economic policy can still compile. Transparent
SDK wrappers add no runtime domain tag or authentication proof. References and
borrows are not currently part of accepted puzzle code, so native ownership
checks are not presented as an on-chain ownership API.

## Reproduce and inspect

From the repository root:

```sh
# Execute the actual compilers, VM cases, compile failures and native tests;
# compare the resulting evidence with the committed report.
python3 scripts/check-error-study.py

# Check declared source hashes and recorded claim consistency without rustc.
python3 scripts/check-error-study.py --check

# Exercise stale-evidence, false-result and wrong-diagnostic controls.
python3 scripts/check-error-study.py --self-test

# After intentionally reviewing changed evidence, regenerate it.
python3 scripts/check-error-study.py --write
```

The execution command uses the pinned toolchain and locked offline Cargo graph,
then links its small study bridge against artifacts identified by Cargo's JSON
output. It compiles the committed Rust fixtures directly, without generating a
duplicate policy or invoking a Rust-looking DSL. Diagnostics normalize repository
paths and runtime thread IDs; reports retain no temporary build paths or timing
measurements. Declared source hashes prevent publishing stale evidence but do
not constitute a complete reproducible-build or compiler-correctness proof.
