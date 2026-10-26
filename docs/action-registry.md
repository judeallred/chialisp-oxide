# Registry actions and separately consumed slots

**WIP prototype. Not ready for use or real funds.** This original, bounded
two-name teaching example combines Rust actions with canonical action-layer,
finalizer, singleton and SLOT hosts. Current evidence comes from the real Cargo
library, independently compiled Chialisp and Rue, retained VM executions and
local SDK journals. Earlier target-only research remains historical. The
published evidence gates require fresh complete bundles, source identities,
protocol records and independently checked downloads. This is not a CATalog
or XCHandles port and adds no live Testnet11 acceptance.

The ordinary `chialisp-oxide-puzzles` library contains three annotated
exports: `registry_initialize::puzzle`, `registry_set_alpha::puzzle` and
`registry_set_beta::puzzle`. Their independently authored Chialisp modules and
Rue sources are separate programs. The canonical action layer, default
finalizer, singleton and SLOT remain external host components; the Rust actions
are not ports of those modules, CATalog or XCHandles.

## Fixed state and exact arguments

The registry has exactly two names. Their 32-byte identifiers are SHA256 of
`chialisp-oxide/registry/v1/alpha` and
`chialisp-oxide/registry/v1/beta`. A value is exactly 32 opaque bytes. An entry
is the improper pair `(name32 . value32)`; its commitment is that pair's CLVM
tree hash. Initial values are 32 zero bytes.

State is `(initialized_bool . (alpha_commitment32 . beta_commitment32))` and
action truth is `(ephemeral_tree . state)`. Rust uses nested `Cons` values and
the nominal `EntryCommitment` wrapper. Bool accepts only nil or atom `01`.
All argument environments are exact proper lists, even though truth, state and
results contain improper pairs. Every action checks that ephemeral is nil and
returns atom `01`, restricting this policy to one registry action per controller
spend. Its opaque-tree input still requires complete host resource preflight
before native policy execution.

The initializer curries the actual SLOT first-curry hash. Its remaining action
solution is nil. It requires uninitialized state, replaces both commitments
with the fixed initial entries, and creates their zero-mojo slots. Initialization
is deliberately permissionless: it cannot choose another owner or initial value.

Each setter curries the owner public key and then the SLOT first-curry hash.
Its remaining solution is the proper list `(old_value32 new_value32)`. It
requires initialized state and checks the selected old commitment, changes only
that entry, and preserves its peer. It emits, in order:

1. A nil puzzle-to-puzzle SEND_MESSAGE to the old slot puzzle hash, mode 18.
2. CREATE_COIN for the replacement slot, amount zero, with the first-curry hash
   as its one hint.
3. AGG_SIG_ME for the owner over `SHA256(domain32 || old_value32 || new_value32)`.

Alpha and beta have different signature domains. The actual signature also
binds the current controller coin ID and Testnet11 additional data. A correctly
sized public key alone does not establish authority. The SDK must submit actual
missing and nonempty wrong-key signatures and check their typed rejection;
signer preflight is not consensus evidence.

The named condition constructors are ordinary SDK Rust bodies. Raw condition
tuples belong in that SDK implementation and independent wire checks. The
authoring adaptation removes five custom assertion messages and changes one
`panic!(literal)` to `panic!()`. It preserves predicates, order, ABI, constants
and all 35 independent vectors, while intentionally changing native panic text.
It introduces no compiler exception or resource-limit increase.

## Creator proof and current spender are different bindings

The canonical 311-byte SLOT has tree hash
`8ab9f3b57a65d7f7a0810f79f7bc1e96bda680d16dd6f51d51e868e13fc7bbb3`.
Its first curry commits controller information and nonce. Controller information
is `(singleton_top_hash . tree_hash(singleton_struct))`; the second curry
commits the entry-hash atom. Curry hash arithmetic uses the **tree hash of that
atom**, rather than treating its payload as an already hashed argument.

The nonce is atom hex `434f5852454731`, decimal string `18946064195602225`.
It exceeds JavaScript's safe integer range; metadata must preserve a string or
BigInt. The slot body ignores nonce and value-hash arguments during execution,
while their bytes remain committed by the puzzle reveal. The original Rust
policy's exact typed bounds do not become additional canonical SLOT guards.

Each slot retains the proof of its actual creator singleton. Its solution is
`(compact_creator_proof . current_spender_inner_hash32)`, with proof
`(creator_parent32 . (creator_inner_hash32 . creator_amount))`. SLOT emits an
ASSERT_MY_PARENT_ID for the reconstructed creator coin, followed by a mode-18
RECEIVE_MESSAGE from the full current controller puzzle hash. Mode 18 binds
puzzle hashes, not coin IDs. It is a consensus message, not a legacy announcement.

The lifecycle must initialize both slots, update alpha, and then consume beta's
older slot. Beta's creator proof still identifies the initializing Eve, even
though the current spender is a later controller coin. Replacing it with current
singleton lineage would test the wrong relationship.

## Isolating a consumed slot

Setters permit `old_value == new_value`. Such a no-op consumes and replaces a
slot with the same puzzle hash and amount, but a different parent and coin ID.
The controller state and puzzle hash also stay equal while its coin ID changes.
This permits a slot-only spentness comparison:

1. Consume live alpha slot A1 in a no-op, producing controller C4 and slot A2.
2. Prepare a valid, freshly signed C4 no-op. Pair it with already consumed A1
   and A1's correct creator proof. Independently execute both components and
   check their exact conditions and matching messages before submission.
3. Require actual SDK `DoubleSpend`, complete in-memory simulator equality and
   C4 still unspent after rejection.
4. Reuse the same C4 spend and aggregate signature with live A2 and A2's own
   correct creator proof. Require acceptance and the actual resulting children.

Replaying a consumed controller bundle is a different test. A stale inner hash
that equals the current one after a no-op is also not a wrong-spender test;
that negative needs a genuinely different puzzle hash. Wrong creator, reveal,
message and signature controls must isolate their defects and record actual
typed outcomes. Resource or infrastructure failures establish no policy rejection.

Separate source mutations must fund their own roots. Dropping SEND should
permit an update without consuming the old slot; check its actual unspent state.
Dropping CREATE should consume the old slot without producing its replacement;
query the actual children. These are accepted policy mistakes, not compiler
failures. Chialisp source mutations provide no Rust source-mutation credit.

## Fresh integration and remaining scope

The manifests retain 17 initializer, nine alpha and nine beta vectors with
independently specified acceptance and exact outputs: 12 accept and 23 reject.
The same 35 cases run natively and in five independently selected language and
optimization builds, giving 175 template VM observations. Native input decoding,
template executions, composed SDK submissions and record corruption controls
remain separate evidence lanes. Repeats and retained-artifact replays add no cases.

The base protocol has five ordinary groups with 55 SDK submissions and 95
component primary observations; five adversarial groups have 50 submissions and
90 primaries. Their separate setup has ten submissions and 15 primaries, and
causal ephemeral checks add 15 primaries without SDK submissions. Thus the base
has 115 SDK submissions and 215 primary observations, with typed rejection and
runtime-umbrella lanes kept distinct. Showing this same group on four pages
never multiplies its coverage.

For ordinary authoring, use the pinned nightly and unpublished path dependencies:

```sh
cargo check -p chialisp-oxide-puzzles --locked
cargo chialisp-oxide check --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked
cargo chialisp-oxide build --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked --output-dir target/registry-corpus-on
cargo chialisp-oxide build --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked --no-optimize --output-dir target/registry-corpus-off
```

Native Cargo checking and CLVM subset checking remain distinct. Every immutable
bundle export must be admitted before selecting the three registry actions.
The shared transport is `registry-slot-sidecars/v1`: three exact-role
corpus references identify one index, which binds five ordinary and five
adversarial journals. All eleven fixed-name files, fifteen artifacts and the
complete triplet must pass admission before a selected view is returned. Each
member is bounded to 2 MiB and the aggregate to 16 MiB. The main validation
report uses one-space indentation without dropping data; its complete current
capture must also fit the unchanged 16 MiB report limit.

The shared journal must be fully read and validated through its bounded packet;
a digest or compact cross-link alone is insufficient. Full journal transcripts
and all vectors remain required under the existing capture bounds.

Canonical source provenance retains the Slot Machine Prosperity Public License
3.0.0 versus Cargo MIT discrepancy and the wallet SDK's separate Apache-2.0
license. The SDK license establishes no upstream relicense; original actions
inherit no upstream grant. Historical canonical Rue reproduction remains
separate from fresh integration execution.

Finite local results establish no general namespace uniqueness, BLS theorem,
compiler-preservation proof, authenticated execution or production readiness.
There is no funded live Testnet11 workflow here. Issue completion is reviewed against the full deliverables separately from any
one test lane or this page.


## Source mistakes that still compile

The separate `chialisp-oxide-registry-controls` Cargo package contains four exact
source variants of the original alpha/beta setters. Each removes one returned
SEND or CREATE condition and changes its local return type accordingly. The
shared production recipe and original exports remain unchanged. Exact source
recipes and the unchanged copied recipe are checked before extraction.

Four linked original calls and four actual mutant calls establish eight native
output observations. Four mutants in two Rust modes add eight retained template
VM executions. Each SDK group constructs its own action root before funding,
composes the selected mutant with independently compiled Chialisp partners,
initializes both zero-value slots, and submits a correctly signed update.

The eight groups have 16 accepted setup submissions with 24 component primaries,
then eight accepted omission submissions with 12 component primaries: 24 SDK
submissions and 36 primary observations in total. The actual old/peer slot state
and complete controller children are queried after acceptance. A second full
journal must be byte-identical. These counts are separate from the base record.

`scripts/run-registry-source-controls.py` coordinates ordinary native checks,
a subsequent LAST host build, and capture. The capture validates and recompiles
all 40 baseline exports and all four control exports in both modes before any
control template or SDK execution. The bounded packet retains complete schema-2
bundles, exact raw artifacts and commands, source/native windows, independent
expectations, and both SDK journals. Its reader checks current complete base
source/native/protocol evidence before considering the retained capture; it
never follows captured host paths. Retained record checks are not fresh VM or
SDK execution, source authenticity, binary authentication or compiler preservation.
