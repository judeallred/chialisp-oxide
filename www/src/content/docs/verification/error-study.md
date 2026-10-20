---
title: Errors that Rust makes impossible
description: Execute Chialisp counterexamples that pass limited tests, inspect real Rust compiler failures, and distinguish type guarantees from policy correctness.
---

Seven faulty Rust programs in this study cannot compile with their declared
types and names. Their Chialisp counterparts compile, pass two deliberately
limited exact-output tests each, and then fail a distinguishing input. Two
further studies show mistakes that compile in Rust but are caught by a later
layer: a native overflow check and the compiled strict ABI, each credited
through an executed corpus case. Two negative controls show policy bugs that
**Rust also compiles**.

“Impossible” has a precise scope here: these ill-typed or unresolved programs
cannot become executable Rust through the tested compiler. It does not mean
that every program written in Rust implements its intended policy. Deliberate
conversions, weaker types, wrong constants, missing authorization and an
incorrect boolean expression can all reintroduce bugs. Every study names the
layer that catches it: native Rust type or name checking, a native overflow
check, the compiled strict ABI or checked arithmetic, or none.

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
Corrected fixtures compile as real native test binaries. Studies whose
compiled layer is the project compiler cite an executed case of the canonical
`module-add` port from the corpus report, where the independent Chialisp
module accepts the witness and the Rust CLVM rejects it in both optimizer
modes; the gate re-reads that case from the current report. **These host-only Rust studies do not establish Rust-to-CLVM
preservation or transaction admission.** The puzzle laboratory and formal
pages describe those separate checks.

The weak suites are concrete, reproducible and intentionally incomplete. Each
asserts the entire expected output tree or rejection, rather than accepting
anything that executes. Passing two such tests says nothing about an untested
branch, a different identifier value, or an excluded approval code.

## Coin ID used as a payment destination

The Chialisp bug returns a coin ID where the policy requires a recipient puzzle
hash. Both values are 32-byte atoms:

```clsp
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

```clsp
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

```clsp
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

```clsp
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

## A deposit silently grows past 64 bits

The policy treats balances as 64-bit amounts and requires an overflowing
deposit to reject. CLVM integers are unbounded:

```clsp
(mod (BALANCE DEPOSIT)
  (+ BALANCE DEPOSIT))
```

The limited tests add `7 + 1` and `18446744073709551614 + 1`, which reaches
the width limit exactly. Both pass. The witness adds `1` to
`18446744073709551615`: the VM returns `18446744073709551616` instead of
rejecting.

The Rust source `balance + deposit` over `u64` **compiles**. It is compiled
here with overflow checks enabled, so the native witness test, marked
`#[should_panic]`, actually passes: the fixed-width sum panics instead of
growing. The corrected fixture uses `checked_add`, which rejects regardless of
compiler settings. The compiled layer is credited through the referenced
corpus case `overflow rejects in Rust; the original grows`, where the Rust
CLVM rejects and the independently compiled Chialisp module accepts the
unbounded sum. Rue `Int` is unbounded by design and reproduces the growth in
both modes.

## A nonempty atom passes as a nonempty approvals list

The policy pays only when the approvals list is nonempty. Chialisp truthiness
treats any nonempty atom as true:

```clsp
(mod (APPROVALS RECIPIENT AMOUNT)
  (if APPROVALS (list (list 51 RECIPIENT AMOUNT)) (x)))
```

The limited tests pass `(1)` and `()`. Both behave as specified. The witness
passes the single byte `0x00`, which is not a list: the program pays anyway.

The Rust attempt writes `if approvals` over a borrowed `Tree`. rustc reports
**E0308**, expected `bool`, found `&Tree`. The corrected fixture matches on
`Tree::Pair` and its native tests cover a one-element list, the empty list
and the zero byte. Rue rejects the untyped condition in both modes, requiring
a type guard before comparison with `Bool`; this protection is shared.

## Recipient and change addresses swapped

Both addresses are 32-byte puzzle hashes, so a swap type-checks nowhere by
itself:

```clsp
(mod (RECIPIENT CHANGE AMOUNT CHANGE_AMOUNT)
  (list (list 51 CHANGE AMOUNT) (list 51 RECIPIENT CHANGE_AMOUNT)))
```

The limited tests use identical recipient and change fixtures, so the
swapped outputs equal the expected ones. The witness uses distinct addresses
and observes the two payments sent to the wrong parties.

The Rust fixture declares `Recipient(PuzzleHash)` and `Change(PuzzleHash)`
wrappers and routes each through its own helper. Passing `change` where
`Recipient` is required reports **E0308**. The guarantee relies on using the
wrappers consistently; unwrapping both first would compile. The Rue
translation uses `Bytes32` for both and swaps them; the coin-ID study's Rue
struct control shows the same distinction can be encoded there.

## A padded amount atom is accepted as an amount

The policy requires canonical minimal integer atoms. Chialisp passes any atom
bytes through:

```clsp
(mod (RECIPIENT AMOUNT)
  (list (list 51 RECIPIENT AMOUNT)))
```

The limited tests pay `7` and `300`. The witness supplies `0x0007`, a padded
seven: the program returns a condition carrying the padded atom instead of
rejecting.

Nothing in the Rust source changes between the faulty and corrected fixtures,
and native Rust cannot express a padded seven at all. The layer that catches
this is the decoder generated for the default strict typed ABI, **a runtime
ABI rejection, not a compiler guarantee**. It is credited through the
referenced corpus case `padded atom rejects in Rust; the original reads it as
one`, where the Rust CLVM rejects in both modes while the independent
Chialisp module reads the atom as an integer. An explicit ignored or raw
environment would not add this check. Rue `Int` carries the padded bytes
through in both modes.

## An extra solution element is silently ignored

The policy requires exactly two solution elements:

```clsp
(mod (RECIPIENT AMOUNT)
  (list (list 51 RECIPIENT AMOUNT)))
```

The limited tests pass two elements. The witness appends a third, `99`, and
the program pays as if it were absent.

A Rust signature fixes the argument count. The faulty fixture includes a host
caller that passes a third field, and rustc reports **E0061**, this function
takes 2 arguments but 3 were supplied. On chain the solution is
attacker-controlled rather than a Rust caller, so the layer that matters
there is the compiled strict ABI's proper-list and arity check, credited
through the referenced corpus case `extra argument ignored by the original`.
Positional Rue parameters ignore extra elements like Chialisp; a rest
pattern with an explicit shape assertion can reject them.

## Negative control: missing policy cap still compiles

The independently stated policy accepts counters below 1000 and returns the
increment. Both faulty implementations forget the guard:

```clsp
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
on the specification and guard, not on `u64` alone.

## Negative control: missing signature requirement still compiles

The policy requires payments above 100 mojos to carry an `AGG_SIG_ME`
condition over a fixture message. Both faulty implementations omit the
branch:

```clsp
(mod (PUBKEY RECIPIENT AMOUNT)
  (list (list 51 RECIPIENT AMOUNT)))
```

The limited tests pay `7` and `100`, and both pass. The witness pays `101`:
the expected output has two conditions, the observed output one.
**Rust and Rue both compile this bug.** The native limited suite passes and
the expanded native assertion that a signature is present fails when
executed. The corrected fixture returns `Some(agg_sig_me(..))` above the
threshold; an `Option<AggSigMe>` return type did not choose that policy. The separate
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
SDK wrappers add no runtime domain tag or authentication proof. The frontend
now accepts restricted borrowed trees, immutable byte references and owned-tree
moves. Rust checks that source, but these tests do not prove preservation of
its ownership guarantees through the complete compiler pipeline.

The [published byte-key insertion helper](/chialisp-oxide/tutorials/insertion-sort-by-keys/)
provides another concrete policy boundary: Rust compiles it, but the type checker
does not prove that its existing sequence is sorted. An executed unsorted-input
counterexample places a new entry before an existing equal-key entry in native
Rust and all five language builds. Stronger collection invariants need an
explicitly validated interface and appropriate proofs.

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
