# Counter actions: state across singleton spends

**WIP prototype. Not ready for use or real funds.** These are finite local
simulator examples. They require no wallet, funding or live Testnet11 connection.
The fixture signing keys are public test data.

The ordinary Cargo library `chialisp-oxide-puzzles` exports two original actions:
[checked increment](https://github.com/judeallred/chialisp-oxide/blob/main/puzzles/counter-increment/puzzle.rs)
and [signed reset](https://github.com/judeallred/chialisp-oxide/blob/main/puzzles/counter-reset/puzzle.rs).
Native execution links that same library. Independently authored Chialisp and
Rue sources implement the same finite action contract. The enclosing action
layer, finalizer, singleton launcher and singleton top layer remain canonical
host components with separate provenance.

An action returns new state and conditions. Its standalone output is not a
complete coin spend. The host commits the actual action artifacts to a Merkle
root, funds their own singleton instance, submits a complete bundle and spends
the actual resulting child. Each language keeps its own module and instance
hashes.

## State and authorization

The shared `StateTruth<'a>` is `Cons<Tree<'a>, u64>`: the explicit improper pair
`(ephemeral . count)`. The ephemeral tree preserves arbitrary atom bytes and
improper tails. Count is a canonical unsigned 64-bit CLVM integer. Nil means
zero; padded, negative, oversized and pair counts reject. These finite Rust
bounds adapt the canonical outer layer's arbitrary state domain.

Increment returns `((ephemeral . count + 1) . ())` and rejects overflow. Reset
validates the old count, returns zero, and emits `agg_sig_me(owner, RESET_INTENT)`.
The owner is curried as one 48-byte atom before its action hash is committed.
A well-shaped key is not proof of ownership: the simulator separately verifies
the actual aggregate signature over the coin and Testnet11 domain.

These exports use the default exact typed environment ABI. Increment takes one
truth argument; reset takes owner and truth. Their argument environments are
proper lists even though the truth and action result use explicit `Cons` pairs.
After currying the owner, each action expects nil action solution. Extra
arguments and improper argument-list tails reject. The canonical outer wire
format has its own separately measured ignored fields.

The fixed reset intent authorizes reset within this coin. It does not commit to
an entire batch or action order. From count five, increment then reset ends at
zero; reset then increment ends at one. Both can be authorized. Two resets emit
two identical signature requirements: one signature is insufficient.

## Build ordinary Rust, then check the CLVM subset

Use the checkout's dated nightly and unpublished path dependencies. Start with
the [Cargo authoring guide](https://judeallred.github.io/chialisp-oxide/start/quickstart/)
when creating your own library. From this repository:

```sh
cargo build --locked -p cargo-chialisp-oxide -p chialisp-oxide-driver -p chialisp-oxide-cli
export PATH="$PWD/target/debug:$PATH"
cargo check -p chialisp-oxide-puzzles --locked
cargo test -p chialisp-oxide-corpus --locked native::tests::counter_actions_preserve_opaque_trees_and_validate_unused_old_counts
cargo chialisp-oxide check --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked
cargo chialisp-oxide build --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked --output-dir target/counter-corpus-on
cargo chialisp-oxide build --manifest-path puzzles/Cargo.toml --package chialisp-oxide-puzzles --locked --no-optimize --output-dir target/counter-corpus-off
```

Ordinary Cargo checks native Rust. The additional compiler command checks the
supported CLVM subset and emits immutable complete bundles. The exports are
`chialisp_oxide_puzzles::counter_increment::puzzle` and
`chialisp_oxide_puzzles::counter_reset::puzzle`. A consumer admits every bundle
export before selecting either action. A module tree hash, serialized blob
digest, package envelope ID and curried singleton puzzle hash are different
identities.

## What the paired corpus checks

The manifests declare 46 increment and 53 reset environments. Across Rust off/on,
independently compiled Chialisp, and Rue source off/on, these give 495 named
raw-action VM observations. Each case declares its independent expected
acceptance and exact output. Native ABI rejection, source overflow panics and
typed VM failure codes remain separate observations. Resource or infrastructure
failures never establish policy rejection.

The main protocol lane selects one action's artifact in each of five builds and
uses the other action's independently compiled Chialisp reference. It commits
the selected pair before funding at state five. Increment creates state six,
then its actual child increments to seven. Reset creates state zero, then its
actual child increments to one. The ten selected-side builds have 60 SDK
submissions: 30 accepted, 20 exact typed consensus rejections and ten separately
resolved runtime umbrellas. Their 70 primary component VM observations are a
separate count.

Wrong membership is submitted while the Eve coin is still unspent, so its
failure cannot be explained by replay. Reset's missing and nonempty wrong-key
signatures are actual rejected submissions. A consumed bundle is submitted
again and must return the exact `DoubleSpend` code. Every rejected submission
requires full in-memory simulator equality; unstable debug text is not a state
serialization format.

The additional dedicated capture exercises both selected actions together,
batch order, proof reuse, duplicate signatures, malformed membership and
lineage, claimed amounts, and the canonical outer layer's permissive fields.
Its independently funded source mutations are separate from the main corpus
groups. An increment that returns its old count can be consensus accepted while
violating the intended counter policy. Constructing and discarding the reset
condition permits an unsigned reset. Each mutant commits and funds its own root
and uses the other side's independently compiled Chialisp reference; it never
borrows an original coin with a mismatched reveal.

## Canonical host components and source scope

The action layer is 630 bytes, with tree hash
`afa03f2903f9e4a293523237799074b13ab2361f250df10de5df884b0b09a22b`.
The default finalizer is 494 bytes, with tree hash
`9d274de59aeaa128d1b0a802b52911838420ef9720d605c78942e8dadfc0810b`.
Their exact SDK bytes match the published Rue counterparts retained from
[slot-machine at 6edc94856306d0f77e065c6c99aacf1055e51e2f](https://github.com/Yakuhito/slot-machine/tree/6edc94856306d0f77e065c6c99aacf1055e51e2f).

The retained upstream license is Prosperity Public License 3.0.0 while its Cargo
metadata says MIT. The SDK snapshot has its own Apache-2.0 license. These facts
do not establish an upstream relicense or a legal compatibility conclusion.
The original teaching actions have separate source provenance. Historical exact
Rue 0.9 source reproduction is labeled separately from fresh current action
compilation and simulator execution.

Each Merkle leaf commits an actual uncurried increment or owner-curried reset
tree hash. The finalizer recreates the action inner puzzle with the new state;
the singleton layer commits that inner hash together with its launcher identity.
The finalizer's condition list starts with the recreated coin, followed by the
action conditions in its canonical order. The singleton prepends amount and
parent assertions. The child coin ID also commits
its actual parent and amount. A matching puzzle hash alone is not a coin ID.

The primary VM is `clvmr 0.21.0`; the local SDK simulator uses `clvmr 0.16.2`,
`chia-consensus 0.36.1` and Testnet11 constants. Their protocols and cost limits
are recorded independently. A matching source window or byte-identical repeat
does not authenticate execution or prove compiler preservation.

## Remaining registry and slot work

The counter is the minimum lesson in
[issue #14](https://github.com/judeallred/chialisp-oxide/issues/14), not its whole
completion. The registry example still needs separately created slot coins,
singleton/slot message binding, actual updates and a submitted slot double-spend.
Replaying a consumed counter bundle does not satisfy that slot requirement.
These examples are not XCHandles, CATalog or a production protocol, and they
establish no live-network acceptance or general security proof.
