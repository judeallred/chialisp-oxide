# Spend through the delegated or hidden tutorial policy

**WIP prototype. Not ready for real funds or production use.** Public fixture
keys are disposable teaching data. Keep native authoring checks, CLVM template
runs, committed instances and complete consensus spends distinct.

Start with a normal Cargo library depending on the unpublished `chialisp-oxide`
facade. Enable its optional `alloc` feature for this port's `OwnedTree` output,
as the paired corpus Cargo library does; `vm` remains a native-test dependency,
not the CLVM extraction feature. The annotated export is the actual
`chialisp_oxide_puzzles::standard_delegated_hidden::puzzle` in
`puzzles/standard-delegated-hidden/puzzle.rs`. Its original reference is the
complete pinned tutorial fence at `docs/primitives/standard-transactions.md`,
lines53–119, revision `dcf06c14836c7ad3f486a5bea36c5a812fd9a5e8`.
The unchanged source spells `condition_codes.clvm`; this port explicitly resolves
that name to `condition_codes.clib` at the declared `chia_puzzles` revision.
This does not establish which include revision the tutorial originally used.

The source is a tutorial standard-wallet policy. Its compiled module identity
must be measured; the port does not inherit the canonical Chia standard module's
bytes or hash, and one module does not complete the whole tutorial page.

## Commit a synthetic key before funding

The raw module receives four positional fields:

```text
(SYNTHETIC_PUBLIC_KEY original_public_key delegated_puzzle solution)
```

It ignores any extra proper or improper tail after the fourth field. The selected
solution is one field, rather than the entire dotted environment. Before funding,
curry the intended synthetic public key into each actual compiled module. The
resulting reveal hash commits both that module and that key. Funding an uncurried
module would leave the key under the spender's control.

Let `P` be the intended hidden program, `H` its SHA256 tree hash, and `K` the
original public key. The synthetic public key is:

```text
K_synthetic = point_add(K, pubkey_for_exp(sha256(K || H)))
```

The unchanged tutorial feeds the exact digest atom to `pubkey_for_exp`. That atom
has signed CLVM integer semantics before reduction modulo the BLS group order;
a host spelling using unsigned integer conversion can produce a different key.
The public fixture file includes positive and negative digest cases derived
independently with the pinned SDK. Native raw group helpers execute the VM; they
are not an independent cryptographic implementation.

## Choose the delegated path

Supply nil as `original_public_key`. The module evaluates the selected program
on `solution`, then prepends:

```text
(AGG_SIG_ME K_synthetic SHA256TREE(delegated_puzzle))
```

The signer is the holder of the synthetic secret key. Consensus augments this
message with the spent coin ID and network-specific signature domain. Returning
the condition does not sign it. The delegated program's hash is signed; the
solution's hash is not separately signed. The selected program must enforce any
needed restrictions on its solution. Two solutions to the identity program return
different trees while retaining the same program-hash signature message.

Nil is the only false value. Nonempty zero atoms such as `0x00` and `0x0000` select
the hidden branch; their numeric value does not turn them into delegated spends.
The raw delegated template permits arbitrary synthetic-key trees, including pairs;
that is original VM behaviour, not a claim that such trees are valid BLS keys or
valid consensus conditions.

## Reveal the hidden program

Supply the original public key and the committed hidden program. The module
checks that their synthetic-key calculation equals the committed synthetic key
and returns the applied result without adding a signature condition. A different
program, even one with equivalent execution but different atom bytes, changes its
tree hash and fails that relation.

This branch adds no synthetic signature. The hidden program must enforce the
intended authorization itself. An identity or fixed-payment program is a useful
public simulator fixture, but knowledge of that program and original public key
would suffice to use it; it is not a secret or a production recovery policy.

The original classic `defun-inline` is a macro, so the hidden-key check runs
before the hidden program is selected. A failed relation therefore skips that
program, including a failing or diverging one. On the delegated branch the
applied result is evaluated before the signature condition is prepended. Extra solution data and ignored environment
tails still receive complete bounded native codec preflight. Host codec or VM
resource failures are inconclusive, not successful malicious-input rejections.

## Reproduce each layer deliberately

```sh
cargo test -p chialisp-oxide-puzzles --test standard_tutorial --locked
cargo chialisp-oxide build --manifest-path puzzles/Cargo.toml --locked
```

The first command executes actual native Cargo policy calls. The second separately
checks the reachable compiler subset and produces CLVM artifacts. Select the
named export in the complete bundle rather than treating every export as a coin
policy. The manifest's named expected vectors preserve arbitrary outputs,
noncanonical atom bytes, ignored tails, invalid points and both branches.

Template runs compare actual Rust Cargo, unchanged independently compiled Chialisp
and independently authored Rue. They do not verify BLS or submit complete spends.
The separate simulator evidence below binds each actual module, curry, exact
input coin, signature message, complete outputs, consensus failure code and
unchanged rejected state. Its downloads keep complete spends separate from
template runs. These exercises use no user wallet, provider or live network.

Check the retained simulator packet and its corruption controls separately:

```sh
python3 -B experiments/standard-tutorial/check.py --self-test
```

This checks recorded evidence; it does not submit or re-execute spends. The
packet inventory below includes the actual source, requests and repeated reports.
