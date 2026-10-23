# XCHandles and CATalog precommit research

**WIP research, reviewed October 22, 2026.** This is source discovery and a
limited policy reading for [#17](https://github.com/judeallred/chialisp-oxide/issues/17).
It adds no executed upstream puzzle, port, deployed-module identity or security
proof. Our [one-name teaching pair](precommit.md) is original work and does not
implement either registry.

## Exact source and license

Both protocols are published in
[`Yakuhito/slot-machine`](https://github.com/Yakuhito/slot-machine/tree/6edc94856306d0f77e065c6c99aacf1055e51e2f):

- Commit: `6edc94856306d0f77e065c6c99aacf1055e51e2f`.
- Root Git tree: `7ceb5d708945ff039f3a7e217fbbfa6960840b7f`.
- [Source inventory](precommit-research-sources.json): exact selected paths,
  Git blob identities, byte counts and SHA-256 digests, checked against the
  retrieved bytes. These are source identities, not CLVM tree hashes.

The pinned [root license](https://github.com/Yakuhito/slot-machine/blob/6edc94856306d0f77e065c6c99aacf1055e51e2f/LICENSE.md)
is **Prosperity Public License 3.0.0**, identifying Mihai Dancaescu (yakuhito)
through FireAcademy SRL. It allows noncommercial use and a limited commercial
trial, with notice requirements when distributing covered material. The
[README](https://github.com/Yakuhito/slot-machine/blob/6edc94856306d0f77e065c6c99aacf1055e51e2f/README.md)
explicitly describes the license change. In contrast,
[`Cargo.toml`](https://github.com/Yakuhito/slot-machine/blob/6edc94856306d0f77e065c6c99aacf1055e51e2f/Cargo.toml)
still says `MIT`; that conflicting metadata does not establish permission to
reuse the puzzle sources under MIT. This repository retains descriptions and
provenance only, with no copied upstream implementation. Any future port needs
an explicit licensing decision before copying or adapting covered material.

## Shared coin: wait and controller authorization

The [Chialisp precommit](https://github.com/Yakuhito/slot-machine/blob/6edc94856306d0f77e065c6c99aacf1055e51e2f/puzzles/singleton/precommit.clsp)
curries the controller singleton identity, minimum relative block height,
payout destination, refund destination and committed value. It asserts its
amount and minimum age, creates the full amount at the selected destination,
and requires `RECEIVE_MESSAGE` mode 19, binding sender puzzle, receiver puzzle
and receiver amount. Message 1 selects payout; 0 selects refund. The sender
puzzle is reconstructed from the controller singleton structure and supplied
inner puzzle hash. Both paths require the same relative age. The
[Rue counterpart](https://github.com/Yakuhito/slot-machine/blob/6edc94856306d0f77e065c6c99aacf1055e51e2f/rue-puzzles/singleton/precommit.rue)
expresses that same intended structure; source similarity is not artifact
equivalence or executed validation.

The committed value need not be inspected by the coin body: it contributes to
the curried puzzle identity which the registry action reconstructs. The
controller's matched message authorizes consumption. This differs from trusting
a solution-supplied coin identifier or an unbound announcement.

## CATalog: knowledge of the hidden TAIL

The [registration action](https://github.com/Yakuhito/slot-machine/blob/6edc94856306d0f77e065c6c99aacf1055e51e2f/puzzles/actions/catalog/register.clsp)
reconstructs the committed value's tree hash from the TAIL hash and a hash of
the initial NFT owner puzzle hash plus CAT-maker hash/solution. Its comment
describes the full opening as a TAIL paired with that context hash. There is no
separate salt parameter in this construction; the intended secret is the
unpublished TAIL. The action verifies the current CAT-maker reveal, derives the
payment-wrapped precommit puzzle, and sends message 1 to that puzzle at the
registry's registration price. Neighbor slots enforce insertion of a previously
unregistered TAIL hash; the resulting NFT is bound to the committed destination.

The [refund action](https://github.com/Yakuhito/slot-machine/blob/6edc94856306d0f77e065c6c99aacf1055e51e2f/puzzles/actions/catalog/refund.clsp)
authorizes message 0 when the maker or price no longer matches, or when an
existing registration slot proves the TAIL was registered. The last case spends
and recreates that slot with an incremented counter. This is invalidity-based
recovery mediated by the registry, not an unconditional timeout refund.

## XCHandles: secret, pricing and singleton approvals

The [commitment helper](https://github.com/Yakuhito/slot-machine/blob/6edc94856306d0f77e065c6c99aacf1055e51e2f/include/xchandles.clib)
hashes a structured tree containing CAT-maker hash/solution, pricing-puzzle
hash/solution, handle plus secret, and owner plus resolved singleton launcher
IDs. Unlike CATalog, it has an explicit secret for a publicly guessable name.

The [registration action](https://github.com/Yakuhito/slot-machine/blob/6edc94856306d0f77e065c6c99aacf1055e51e2f/puzzles/actions/xchandles/register.clsp)
checks handle identity, neighboring slots, current maker/pricing reveals and a
zero current expiration. Pricing returns payment and registration duration;
the action asserts the chosen start time and creates an expiring record. It
reconstructs the payment precommit and sends message 1 for the calculated
amount. Mode-18 messages additionally require approval from the committed owner
singleton and, when different, the resolved singleton. This is not a direct
public-key signature check in the registration action; authorization continues
through those singleton spends.

The [refund action](https://github.com/Yakuhito/slot-machine/blob/6edc94856306d0f77e065c6c99aacf1055e51e2f/puzzles/actions/xchandles/refund.clsp)
checks the committed maker/pricing reveals and reconstructs the precommit.
Changed maker/pricing or incorrect payment can enable recovery without a handle
slot. Otherwise an active conflicting registration must be demonstrated by a
slot spend, guarded by `ASSERT_BEFORE_SECONDS_ABSOLUTE` at its expiration. It
sends message 0. Handle-record expiration and refund eligibility are therefore
distinct from the precommit's minimum age.

## Attack scope and lessons for the original demo

The following are inferences from the reviewed constructions, not executed
upstream attack tests:

- A copier learning an opening from a pending registration cannot immediately
  mature a new precommit. Destination and authorization bindings prevent simply
  changing the copied opening to claim it for someone else.
- This requires secrecy before reveal: XCHandles' salt must be unpredictable;
  CATalog's knowledge argument needs a TAIL not already known to competitors.
  Neither a hash nor a length check establishes entropy.
- A competing party with an independently prepared mature commitment can still
  contend. Relative age does not impose first-committer ordering, guarantee
  inclusion, prevent censorship or establish fair pricing. Reorgs, lost secrets,
  malformed spends and refund races need separate operational analysis.
- A consumed precommit cannot be reused as the same unspent coin; competing
  commitments and changing registry state still need protocol-specific handling.

Our teaching pair instead uses one fixed slot/name, fixed XCH price, exact coin
announcements, an owner `AGG_SIG_ME`, and a later permissionless refund to a
committed destination. Its independent Rust/Chialisp/Rue implementations and
recorded tests establish only that teaching contract. In particular, its refund
age is not the upstream refund policy, and its signature is not XCHandles'
owner/resolved singleton approval protocol. Keep these contracts explicit before
designing a reusable `Precommit<T>`.

## Search scope and remaining work

Reviewed the pinned shared precommit, both registration/refund pairs in Chialisp
and Rue, relevant Chialisp includes, root notices and registration CLI sources.
The [CATalog CLI](https://github.com/Yakuhito/slot-machine/blob/6edc94856306d0f77e065c6c99aacf1055e51e2f/src/cli/catalog/register.rs)
finds an existing commitment on a repeated command and waits with an additional
block margin. The [XCHandles CLI](https://github.com/Yakuhito/slot-machine/blob/6edc94856306d0f77e065c6c99aacf1055e51e2f/src/cli/xchandles/register.rs)
generates a secret when absent and asks the user to retain secret/start time for
the follow-up invocation. These are lifecycle examples for #12, not evidence
that our CLI already resumes registrations.

The upstream README also identifies Reward Distributor commitments as another
application; this review does not establish the same adversary model there.
Auction bids, raffle entry and delayed continuations remain possible original
applications, not researched ports. Deployment identity, complete include/build
closure, SDK module selection, artifact replay, pricing/expiration actions and
full-protocol adversarial validation remain outside this source review.
