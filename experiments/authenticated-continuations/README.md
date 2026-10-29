# Ordered payout continuation ABI/hash spike

**WIP experiment. Not ready for use or real funds.** This is an original teaching
policy for issue #16. It is not an automatic compiler continuation feature,
a generic `Step<State>` API, an XCHandles port, or a general deployment workflow. The separate ledger subcrate records finite
complete-spend observations.

The ordinary Cargo library exports one annotated `step` function. Seven native tests and six independent Python host-oracle tests check the
recipe. Actual CLVM subset admission and two source-aware builds pass; the
retained Rust off/on and independently compiled Chialisp programs pass 36 raw
environments each, for 108 primary-VM observations. Native checking, subset
admission and VM replay remain separate evidence. The ledger milestone below adds three four-step simulator chains, source
controls and actual generator costs. Shared CLI/GUI planning, submission and
durable node recovery remain unimplemented. Issue #16 remains open.

## Concrete contract

A state is `Terms = (PuzzleHash, Message32, u8)`, serialized as one proper
three-element list and curried as **one argument**. The fields are the selected
module's tree hash, remaining queue head, and remaining item count. A solution is
one proper two-element `Opening = (PuzzleHash, Message32)` list: the next
recipient and next queue digest. The uncurried environment is exactly
`(terms opening)`. Curried callers supply exactly `(opening)`. All byte fields
are atoms of exactly 32 bytes. Integer fields use canonical unsigned encodings;
counts admitted by the policy are 1 through 8 inclusive.

Each item pays exactly 1,000 mojos. There is no controller reserve, remainder,
abort, authorization key, fee branch, singleton, dynamic apply, iterator or
implicit suspension point. A separate sponsor may pay fees. Every nonterminal
spend emits exactly one payout and one successor; a terminal spend emits one
payout and no successor. `ASSERT_MY_AMOUNT` requires exactly `1000 * remaining`
mojos. The output is the explicit proper condition list represented by
`Cons<AssertMyAmount, Cons<CreateCoin, Option<CreateCoin>>>`.

The fixed byte commitments are:

- `end = SHA256(ASCII("chialisp-oxide/continuation/v1/end:1000"))`.
- `head = SHA256(ASCII("chialisp-oxide/continuation/v1/item:1000") || recipient[32] || next[32])`.

No CLVM serialization, hex text, numeric conversion, extra separator or trailing
zero is included in those preimages. The domain tags have 39 and 40 bytes,
respectively. Fixed-width fields make the encoding unambiguous. This commits
order and exact recipient bytes under SHA collision/preimage assumptions; no
compiler or finite test proves those assumptions.

For `remaining == 1`, the opening must end at `end`. Otherwise `next != end`,
and the successor commits `(same module, next, remaining - 1)`. Its amount is
`1000 * (remaining - 1)`. Its puzzle hash is calculated using the standard curry
of the selected module with that complete next Terms list. The count's CLVM
atom payload is exactly one byte for this finite range. The native helper selects its literal atom hash from an eight-entry table
(dynamic byte-array construction is not currently admitted); the independent Python oracle spells the curry tree separately.
The eight-entry `remaining_budget` table equals the mathematical multiplication
for every admitted count. It avoids assuming integer-widening casts that the
current CLVM subset rejects; ordinary native Rust accepts those casts.

## Funding and module identity boundary

Before funding, the host must select an actual admitted serialized module,
compute its tree hash, reconstruct the **complete** ordered queue, check its
length is 1..=8, commit its resulting head/count, and require the funded coin's
puzzle hash and amount to match this recipe. `funding_hash` checks the module
field against a caller-supplied selected hash and derives the initial hash. It
does **not** admit artifact bytes, authenticate the selected hash, inspect the
whole queue, or authorize funding. There is no live funding command here.

The on-chain template has no intrinsic way to recognize an arbitrary
caller-supplied self-module hash. Changing it can still produce valid template
conditions for another successor. That is a funding-preflight counterexample,
not an expected template panic. Once a correctly verified initial state is
funded, the derived successor preserves the same selected module. A valid
prefix with a false remaining count may run until a later terminal mismatch and
strand funds; this is why initial complete-queue checking is essential.

Anyone can advance a correctly formed opening. They can race or accelerate the
fixed work, but not select its recipients. No one is guaranteed to advance it.
There is no liveness, inclusion, finality, cross-block atomicity or refund
promise. A later abort policy needs separately committed terms and evidence.

## Reproduce the recipe checks

```sh
cargo test --manifest-path experiments/authenticated-continuations/Cargo.toml --locked
cargo clippy --manifest-path experiments/authenticated-continuations/Cargo.toml --all-targets --locked -- -D warnings
python3 -B -m unittest discover -s experiments/authenticated-continuations -p test_oracle.py
python3 -B experiments/authenticated-continuations/oracle.py
```

`fixtures.json` uses an all-`aa` **fictional** module hash solely to compare hash
recipes. It is not the identity of a compiled program. Three fixture transitions
pay the all-`11`, all-`22` and all-`33` hashes in that order. The native test checks
the fixed oracle queue and curry hashes, output amounts, no terminal successor,
skipped/reordered/injected items, wrong next digest, early/excess termination,
count bounds, the complete budget table, and wrong selected-module preflight.
The Python tests independently check the same host-level contract. These are
native/host observations, not six or seven SDK spend groups.

`reference.clsp` is original, self-contained independent Chialisp source written for this repository; no upstream module is copied. It validates the same exact list/atom/count contract
before emitting conditions. Its helper `compile_reference` uses the repository's
pinned independent module compiler, not the raw-expression API:

```sh
cargo run --manifest-path experiments/authenticated-continuations/Cargo.toml --locked \
  --example compile_reference -- /tmp/continuation-reference.clvm.hex
```

Compilation is separate from executing its serialized result. The retained IR
is also validated and recompiled with the actual Rust verifier, in both modes,
requiring exact artifact bytes and tree hashes. This is an executed compiler
consistency check, not a compiler-preservation proof. The retained checkpoint replays the compiled reference and both Rust modes
against the declared raw-tree suite. The separate ledger milestone below adds
complete SDK spends, full simulator preservation after rejections and actual
generator measurements. No general batch-size or block-fit guarantee follows. SHA and optional outputs remain
outside the current proof models.

## Retained finite VM checkpoint

`evidence/receipt.json` indexes exact source and retained file hashes, both
complete schema-2 build bundles, the independently compiled reference, explicit
vectors, typed outcomes and costs. Those source attachments are bounded selected
library reads, not complete build graphs. Host paths and tool hashes retain the
original capture context; they do not authenticate installed executables or
prove compiler preservation. Run the reader and optional actual retained replay:

```sh
python3 -B experiments/authenticated-continuations/check_retained.py
python3 -B experiments/authenticated-continuations/check_retained.py \
  --replay-cli target/debug/chialisp-oxide --verifier target/debug/chialisp-oxide-verifier \
  --output target/continuation-replay-new
```

The optimized Rust module is 7,967 bytes; its first/second/terminal template VM
costs are 92,068 / 90,527 / 27,489. Rust off is 43,233 bytes at
201,680 / 197,144 / 53,212. Chialisp is 1,449 bytes at
54,399 / 54,399 / 20,359. These are primary `clvmr 0.21.0` template costs under
`MEMPOOL_MODE`, with a 10,000,000 per-case cap, not total generator or ledger
costs. The modules have different identities. None of these runs funds a coin.

Four environments accept in each mode: the three linked fixture steps and the
explicit changed-module counterexample. The other 32 environments reject. The
Rust-off strict decoder reports `InvalidOpArg` when an extra list element is
supplied as the environment, Terms or Opening tail; optimized Rust and the
reference raise for those three cases. All other retained negative results are
`Raise`. The checker requires each exact declared code; resource or
infrastructure outcomes fail the gate. Malformed shapes are VM inputs, not
native Rust values.

The initial source tried a native-valid integer widening cast and a dynamic
one-byte array. Actual subset checks rejected both. Literal finite budget and
count-atom tables now implement this bounded policy without adding compiler
semantics or relaxing guards. The native tests check every entry. Those rejected
source attempts are not admitted frontend features.

## Finite complete-spend milestone

The separate `ledger` Cargo package reconstructs a four-item queue independently
with `sha2`, constructs each actual curried reveal and funds it in the pinned
TESTNET11 SDK simulator. Each of the three builds (Rust off/on and independent
Chialisp) mines four transactions, pays all four recipients, creates exactly
three actual successor coins and ends with no continuation. It charges zero
fees; fee sponsorship and another batch size remain unimplemented.

The base lane contains three scenario runs and **45 SDK submissions**: 12
accepted, 21 rejected with exact intended consensus codes and 12 explicit
`GeneratorRuntimeError` observations. The exact rejections cover unrelated
nonempty BLS aggregates (`BadAggregateSignature`), incorrect coin amounts
(`AssertMyAmountFailed`), valid reveals that differ from the committed one
(`WrongPuzzleHash`), and consumed-coin resubmissions (`DoubleSpend`). Every
rejected submission, including each runtime umbrella, checks complete simulator
equality in memory. No unstable SDK Debug state is retained. Runtime umbrellas
remain inconclusive as policy-rejection evidence; the independent primary-VM
lane supplies the separately typed opening failures.

Each build writes and reparses a public cursor after the first step. The
simulator remains in memory. After another permissionless advancement consumes
the cursor's coin, the harness reconciles its exact consuming spend and child
without another push. This is an in-process checkpoint/reconciliation
experiment, not crash-safe process or node recovery and not shared CLI/GUI
resume support.

Actual uncompressed generator totals for all four accepted steps are:

- Rust on: 32,684 charged bytes and 405,112,339 total cost.
- Rust off: 173,748 charged bytes and 2,098,234,081 total cost.
- Chialisp: 6,612 charged bytes and 92,128,540 total cost.

The corresponding measured backreference totals are 14,908 bytes / 191,800,339
cost, 22,444 / 282,586,081 and 5,952 / 84,208,540. These are actual encoded
generators under the pinned previous-height rules, not whole-block observations
or a guarantee for other batches. The reviewed shared measurement module checks
expanded generator equality, BLS verification, exact total-cost budgets and
one-less `CostExceeded` controls. Conditions-helper calls skip BLS; generator,
helper and SDK submission counts remain distinct. Only batch size one is
measured. Existing backend optimization differs between modes; optional Rust IR
rewrite support or universal cost improvement is not claimed.

Four actual-source controls remove opening authentication, remove the terminal
sentinel, omit the amount assertion, or replace the derived successor with an
attacker's fixed puzzle hash. Each original and mutant funds its own selected
reveal. The separate lane has **8 native observations, 16 primary-VM observations
and 16 SDK submissions**: 10 accepted, two exact amount rejections and four
explicit runtime umbrellas. The counterexamples show injected payouts,
premature termination, a one-mojo fee from an overfunded coin, and diverted
successor funds. Native syntax/types do not enforce those policy invariants.
The finite controls do not establish absence of other failures.

Reproduce the actual retained SDK scenarios independently:

```sh
cargo build --manifest-path experiments/authenticated-continuations/ledger/Cargo.toml --locked
python3 -B experiments/authenticated-continuations/check_ledger.py
python3 -B experiments/authenticated-continuations/check_ledger.py \
  --runner experiments/authenticated-continuations/ledger/target/debug/chialisp-oxide-continuation-ledger \
  --verifier target/debug/chialisp-oxide-verifier \
  --output target/continuation-ledger-replay-new
```

`check_ledger.py` alone checks complete packet/source consistency. Its explicit
runner option executes SDK submissions and requires byte-identical reports;
repeated runs add no distinct coverage. `source_controls.py --help` describes
the fresh native/compiler/VM/source-control capture. Fresh capture admits the
complete source-aware export inventory with independently queried compiler and
driver identities before selection, checks source windows, validates recorded
VM/native witnesses independently, and uses precharged streaming stdout/stderr
caps with deadlines and process-group cleanup. Retained readers never open captured host paths. Structural
bundle admission and source consistency do not authenticate execution or prove
IR/CLVM equivalence. Eight constructed bundle/VM corruptions and eight constructed ledger/native/VM
witness corruptions are separate from actual source mutations, native
observations, VM runs and ledger attempts. Six transport controls separately
check precharged stdout/stderr caps, deadlines and process-group cleanup.

Issue #16 remains open for a reusable reviewed SDK interface, funding-plan
admission, a second explicit batch size, cost planning, and shared CLI/GUI
submission/monitoring with durable interruption recovery. This original demo
requires neither singleton support nor dynamic apply. Larger generators,
iterators, automatic suspension or general enum support would need separately
specified compiler semantics and evidence.
