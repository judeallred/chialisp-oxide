# Readable Rust puzzles with explicit wire contracts

The puzzle body should describe its spending policy: who signs, which coin
amount it expects, who receives the outputs, and when spending is allowed.
Protocol discriminants belong in the SDK's condition constructors. Fixture keys
belong in a separate data module. Policy amounts, recipients, and times need
names at the call site.

The current teaching fixtures use ordinary Rust constants, functions, inherent
methods, and type aliases. For example, the capped-payment policy is expressed
as:

```rust
const COIN_AMOUNT: Amount = Amount::from_mojos(1_000);
const PAYMENT_CAP: Amount = COIN_AMOUNT;
const RECIPIENT: PuzzleHash = PuzzleHash([0x42; 32]);

pub type PaymentConditions = (AssertMyAmount, CreateCoin);

pub fn puzzle(amount: Amount) -> PaymentConditions {
    assert!(amount.is_at_most(PAYMENT_CAP));
    (
        assert_my_amount(COIN_AMOUNT),
        create_coin(RECIPIENT, amount),
    )
}
```

`Amount::from_mojos` states the monetary unit. `Timestamp::from_unix_seconds`
states that a time is absolute; it does not mean a delay after coin creation.
Extracting `.mojos()` for checked integer arithmetic is explicit. The SDK does
not yet provide overloaded arithmetic operators, generic condition collections,
or general trait-based on-chain APIs.

These names do not add hidden policy checks. `PuzzleHash`, `PublicKey`, and
`Message32` distinguish source-level domains, while ABI decoding validates their
byte lengths. A `PublicKey` wrapper does not prove that its bytes represent a
valid BLS key. Amount and timestamp constructors accept their full `u64` range;
the puzzle must still express its own bounds. Public fields and tuple result
aliases permit deliberate unwrap/rewrap and raw construction.

## Optional output lists

Ordinary `core::option::Option<T>` now has a bounded output encoding for admitted
payloads: `None` becomes nil, and `Some(value)` becomes a one-element list.
`Cons<First, Option<Last>>` can therefore return a fixed first element with an
optional last element. A tuple still retains every field; a tuple containing
`None` does not omit that field. See [optional-lists.md](optional-lists.md) for
the exact representation and examples bound to the Cargo integration fixture.

This supports output construction and opaque helper passing, with drop-free
payloads and strict initializer evaluation. It does not admit optional exported
inputs, Option inspection or methods, general enums, `Vec`, or trait-based
collection APIs. Optional-capable programs conservatively skip Rust IR
refinement and inlining, so this feature carries no cost-improvement claim.
The [boomerang lesson](https://judeallred.github.io/chialisp-oxide/tutorials/boomerang/)
uses this capability to return a signed claim or an unsigned refund. Its
committed instances and actual SDK submissions have separate evidence; the
constructor tests alone establish neither authorization nor a complete spend.

## Inspiration from Rue and the Rust wallet SDK

Rue's condition library uses named structures with default opcode fields. Its
delegated-condition example constructs `AggSigMe` from a public key and a message;
its royalty example constructs `CreateCoin` from named puzzle-hash, amount, and
memo fields. The author does not repeat protocol opcode literals in those policy
expressions. The inspected revision is
`245c107a8e795229ea08097142f005a4c0d91075`, also the current Rue main revision
when checked on 2026-10-16.
[Rue condition types](https://github.com/xch-dev/rue/blob/245c107a8e795229ea08097142f005a4c0d91075/crates/rue-compiler/src/std/conditions/types.rue),
[delegated conditions](https://github.com/xch-dev/rue/blob/245c107a8e795229ea08097142f005a4c0d91075/examples/puzzles/p2_delegated_conditions.rue),
[royalty split](https://github.com/xch-dev/rue/blob/245c107a8e795229ea08097142f005a4c0d91075/examples/puzzles/royalty_split.rue).

Chia Wallet SDK 0.34.0 similarly declares typed conditions with constant opcode
fields and generates constructors whose parameters omit those constants. Its
condition names and ordinary Rust constructor style are useful precedents.
The inspected release commit is
`4ac2d5d499305a21219cc49f2d2d7692b23f75af`.
[Typed conditions](https://github.com/xch-dev/chia-wallet-sdk/blob/4ac2d5d499305a21219cc49f2d2d7692b23f75af/crates/chia-sdk-types/src/condition.rs),
[constructor generation](https://github.com/xch-dev/chia-wallet-sdk/blob/4ac2d5d499305a21219cc49f2d2d7692b23f75af/crates/chia-sdk-types/derive/src/impl_conditions.rs).

That wallet SDK's fluent `Conditions` builder stores `Vec<Condition<T>>` and uses
generic conversions and iterators. It is a host-side spend-building API; bringing
that implementation into our current restricted compiler would require general
allocation, enum/list representation, and generic/trait semantics that are not
implemented. Fixed tuples of named condition results retain their explicit
proper-list ABI; `Cons` and bounded output Option offer explicit list
construction without adopting that builder. A general collection API needs its
own semantics and cost experiments.
[Conditions builder source](https://github.com/xch-dev/chia-wallet-sdk/blob/4ac2d5d499305a21219cc49f2d2d7692b23f75af/crates/chia-sdk-types/src/condition/list.rs).

## One domain implementation for both authoring paths

The real Cargo SDK re-exports
[domain.rs](../crates/chialisp-oxide/src/domain.rs). The complete teaching corpus is
now the `chialisp-oxide-puzzles` Cargo library and depends on that SDK; its native oracle
and CLVM extraction use the same library. Standalone compiler regressions can
reach the same definitions through [sdk/chialisp_oxide.rs](../sdk/chialisp_oxide.rs), which is no
longer the corpus route. There is no second set of domain declarations or
condition encoders. Cargo users mark exports with `#[chialisp_oxide::puzzle]`.

The corpus declares package/export/input provenance in
[puzzles/authoring.json](../puzzles/authoring.json). Validation and proof reports
record and recheck those local input hashes. The measured readability snapshot
below predates this Cargo-corpus migration; its historical byte/cost results
are retained as recorded.

Named `const` values still pass through rustc's type checking and constant
evaluation. Supporting a byte-array constant does not admit arbitrary memory,
pointers, references, or general constant aggregates into the CLVM ABI. The
driver must reject unsupported constant representations explicitly.

Fixed example destinations and known test public keys are fixture data, not
wallet configuration or a key-management interface. Giving them names improves
reviewability without changing their role or making an unauthenticated payment
policy require authorization.

## Validate a source-only readability change

Capture the last validated corpus report before editing the authoring code, then
regenerate it by executing the native, reference, optimized/unoptimized CLVM,
and ledger checks. The comparison utility accepts an explicit baseline:

```sh
git show HEAD:corpus/validation.json > /tmp/oxide-before-refactor.json
# Refactor the Rust sources and SDK helpers.
cargo build --workspace --locked
./scripts/chialisp-oxide validate
python3 scripts/check-readable-puzzles.py --baseline /tmp/oxide-before-refactor.json
python3 scripts/check-ir-proofs.py
python3 scripts/check-proofs.py
```

[check-readable-puzzles.py](../scripts/check-readable-puzzles.py) permits changes
only to the embedded puzzle source and dependency-source snapshots. It compares
all other report data exactly, including serialized CLVM bytes, tree hashes,
sizes, disassembly, native/reference outcomes, execution costs, vector coverage,
and ledger scenario results. It also checks that the current report's sources
and dependency hashes match the working files. Its mutation controls run with
`--self-test`.

The baseline is supplied for a particular refactor. There is no permanent byte
freeze: a deliberate compiler or optimizer change should establish and review
its own new evidence. Running the script without `--baseline` checks report
consistency and source freshness only; it does not compare previous artifacts.
The script reads reports and does not execute the compiler itself.

If introducing readable helpers also fixes redundant compiler work, byte and
cost identity may intentionally change. For example, a total getter or condition
constructor needs no additional evaluation solely to preserve failure: it cannot
fail on already-evaluated, validated typed operands. The bounded MIR totality
analysis runs after successful lowering and conservatively retains uncertain
operations. Each fallible source definition retains its evaluation; later copies,
tuple packing, and total forwarding calls do not repeat it merely because the
compiler substitutes the earlier expression. A helper that can reject, or an
earlier argument computation that can fail, must remain strict. This compiler
change needs its own discarded-call, ignored-failing-argument, transitive-helper,
overflow, and lazy-branch regressions in both optimization modes.

Review such changes using the explicit semantic comparison mode:

```sh
python3 scripts/check-readable-puzzles.py \
    --baseline /tmp/oxide-before-refactor.json \
    --comparison semantic --changes /tmp/oxide-codegen-changes.json
```

This mode permits Rust-generated bytes/hash/size/disassembly and Rust VM
cost/diagnostic changes. It preserves all recorded native and compiled outcomes,
input vectors and coverage, ledger results, and the independent Chialisp
artifacts/runs exactly. It lists changed byte sizes and counts of cost/diagnostic
changes; the JSON includes old/new tree hashes and per-case measurements. Cost
exhaustion cannot replace a recorded policy rejection. A changed tree hash is a
changed puzzle identity, even if all tested outputs agree.

Identical compiled bytes preserve the compiled program and its ABI. When bytes
differ, recorded comparisons establish agreement over those cases; they do not
prove equivalence for every possible input or every execution-cost budget.
Independent expected condition trees and native/reference tests check that the
named helpers express the intended wire contract. A text search for helper
spellings would establish neither property. These are artifact identity or
executed regression results, not a general proof of compiler preservation or
complete puzzle security. Keep the distinctions in [verification.md](verification.md).

## Measured migration snapshot

The 2026-10-16 comparison uses the committed report at
`33bfee5c8ecc12abeed2942bf2ec05711c331649` as its baseline. Both reports use
`nightly-2026-07-14`, rustc `1.99.0-nightly (daf2e5e18 2026-07-13)`, and Rue
`245c107a8e795229ea08097142f005a4c0d91075`. Serialized Rust CLVM sizes changed as
follows, with LIR optimization off and on respectively:

- Capped payment: **2,027 → 1,547 bytes**; **1,785 → 1,325 bytes**.
- Split payment: **6,107 → 3,017 bytes**; **5,625 → 2,635 bytes**.
- Bounded counter: **2,747 → 2,237 bytes**; **2,385 → 1,895 bytes**.

These three fixtures have changed CLVM tree hashes in both settings. The other
six fixtures retain identical Rust bytecode and hashes. The reductions come
from correcting redundant evaluation in the compiler, while preserving the
native policies. For example, the optimized split's recorded middle-vector cost
fell from **24,780 to 12,614**; its returned condition bytes are unchanged.

Independent semantic comparison passed for all nine fixtures: native/reference/
compiled outputs and rejection outcomes, 73 explicit vectors, 3,063 generated
integer cases of coverage, independent Chialisp artifacts and measurements, and
30 ledger scenario results remain unchanged. Rust execution costs and a few VM
diagnostic details changed as recorded. This is measured regression evidence,
not universal equivalence or compatibility with earlier puzzle-hash commitments.
The current measurements are in [corpus/validation.json](../corpus/validation.json).
