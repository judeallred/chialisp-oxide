# Precommit, wait, reveal

**WIP prototype; not ready for real funds.** This page documents the original
teaching pair `precommit-claim` and `precommit-registry` for
[issue #17](https://github.com/judeallred/chialisp-oxide/issues/17). They are
not ports of XCHandles or CATalog and do not copy their contracts.

## The problem

Registering a contested name in one step leaks the request: anyone watching the
mempool sees the name and can submit a competing spend with a higher fee. A
precommit splits registration into three phases:

1. **Commit.** The owner funds a precommit coin whose curried terms include a
   hash of the registration, and not the registration itself.
2. **Wait.** That coin must be at least `MINIMUM_AGE` blocks old before it can
   be claimed. A copier who first learns the name from a reveal cannot have a
   mature precommit of their own unless they prepared one earlier.
3. **Reveal.** The registry checks the opening against the commitment, requires
   the owner's signature and completes the registration in the same bundle as
   the precommit claim.

## The two coins

`puzzles/precommit_recipe.rs` holds the shared policy constants and hashes:

- a fixed name;
- a 1000-mojo price paid to a fixed treasury;
- a minimum age of 2 blocks and a refund age of 8 blocks.

The commitment and protocol messages use SHA-256 with named domain prefixes
and fixed-length fields. Coin IDs and curry/tree hashes follow their standard
encodings; the intermediate commitment context hashes fixed-length fields
without an additional prefix.

**Precommit coin.** Its curried terms are
`(registry coin ID, commitment C, refund destination)`, and its solution is
`(claim, my coin ID)`. Both paths assert its own ID and its 1000-mojo amount.

- *Claim:* `ASSERT_HEIGHT_RELATIVE 2`, pay the treasury, announce
  `ready(C, me)`, and assert the registry coin's announcement `accept(C, me)`.
- *Refund:* `ASSERT_HEIGHT_RELATIVE 8`, return all 1000 mojos to the refund
  destination, and announce `refunded(C, me)`. Anyone may submit it. Only the
  committed destination can receive the funds.

**Registry slot.** Its curried term is the exact precommit module hash. Its
solution is `(my coin ID, precommit parent, refund destination, opening)`, where
the opening is `(owner key, receipt destination, salt)`. It:

1. recomputes C from its own coin ID, the name, the refund destination and the
   opening;
2. recomputes the precommit's curried puzzle hash and coin ID from the parent and
   the price;
3. asserts that coin's `ready(C, precommit)` announcement;
4. announces `accept(C, precommit)`;
5. requires `AGG_SIG_ME` from the owner key over a message bound to C;
6. creates a one-mojo receipt at the receipt destination.

Each coin requires the other's announcement, so neither half completes alone.
The registry never trusts a precommit ID or a commitment from its solution. It
derives both, and the genuine precommit's own claim path enforces the age and
the payment.

## What the ledger scenarios show

The corpus runs optimized Rust, Chialisp and optimized Rue builds of one side
against the independently compiled Chialisp reference of the other side,
using `chia-sdk-test` 0.36.0. Template VM vectors separately cover both Rust
and Rue optimization modes.
Every rejected bundle must fail with its named consensus code and leave the
complete simulator state unchanged:

| Scenario | Expected result |
|---|---|
| Complete bundle one block before the minimum age | `AssertHeightRelativeFailed` |
| Registry alone; precommit claim alone | `AssertCoinAnnouncementFailed` each |
| Opening with a changed receipt destination, owner key or salt | `AssertCoinAnnouncementFailed`: no genuine precommit matches |
| Complete bundle with an empty signature | `BadAggregateSignature` |
| Refund before the refund age | `AssertHeightRelativeFailed` |
| Mature signed bundle | Treasury receives 1000 mojos; owner receipt receives 1 |
| Second mature precommit for the spent slot | `DoubleSpend` |
| Refund at exactly the refund age | Full amount returns; registry slot stays unspent |

The template vectors in each `puzzle.json` add exact-ABI rejections, such as
wrong widths, pairs for atoms, padded booleans, arity errors and improper
tails. An independent Python oracle (`scripts/precommit_oracle.py`) recomputes
every expected output from `hashlib`.

## Sizes and costs

The table shows module bytes for the template, before currying, and the
largest primary-VM cost among accepted template vectors:

| Export | Rust off | Rust on | Chialisp | Rue off | Rue on |
|---|---|---|---|---|---|
| `precommit-claim` bytes | 34,513 | 16,523 | 1,038 | 1,792 | 1,486 |
| `precommit-claim` max cost | 59,308 | 27,051 | 30,987 | 22,670 | 19,319 |
| `precommit-registry` bytes | 8,311 | 3,348 | 1,826 | 2,895 | 2,318 |
| `precommit-registry` max cost | 217,624 | 78,169 | 77,360 | 76,845 | 71,013 |

The claim's optional final condition makes it an optional-output program, so
it skips the Rust IR optimizer stages, as boomerang does. Its Rust module is
about sixteen times the Chialisp module. Reveal bytes are charged on chain, so
the Rust build is far from cost-competitive. This is the module-size gap
tracked by [#6](https://github.com/judeallred/chialisp-oxide/issues/6), not a
property of the policy.

## Assumptions and limits

- **One name, one slot.** This is not a dynamic namespace, renewable name service
  or transfer protocol. Funding several independent slots for the same name
  would not make the name globally unique.
- **No upper claim age.** A mature precommit stays claimable until someone
  refunds it. Someone who prepares mature precommits ahead of time for guessable
  names can still compete for a slot. Nothing here establishes
  first-committer-wins, fair ordering or censorship resistance, and a farmer can
  delay a registration until the refund age.
- **Salt entropy.** The commitment hides a guessable name only while its salt is
  unpredictable. The chain checks the salt's length, not its randomness. Generate
  32 bytes from an operating-system CSPRNG. Fixture salts are public test data.
- **What observers learn.** Before the reveal, an observer sees the precommit's
  puzzle hash, amount, ancestry and timing. Revealing its puzzle also reveals the
  registry coin ID, C and the refund destination. The registration reveals the
  full opening.
- **Cryptography.** SHA-256 collision and preimage resistance and BLS security
  are assumptions. No proof model covers these exports, SHA-256 or the two-coin
  protocol.
- **Announcements, not messages.** The pair uses legacy coin announcements,
  bound in both directions to exact coin IDs. The Chialisp reference recommends
  `SEND_MESSAGE`/`RECEIVE_MESSAGE` for new designs. The usual hazard of a
  hard-coded announcement, funds locked forever, does not arise here: if the
  slot is spent first, the precommit's refund path still returns its funds.
  A message-based variant remains open work.
- **Small ages for teaching.** The ages of 2 and 8 blocks keep scenarios short.
  They are not recommended production parameters.

## Not yet done

- A reusable `Precommit<T>` SDK building block. It should be designed from this
  concrete pair, not before it.
- Integration with the planned lifecycle CLI (#12), and a site tutorial built
  from recorded validation data.
- A naive one-step front-run comparison, actual source-mutation controls, and
  dedicated refund-after-completion replay. The current finite scenarios do
  not complete all of issue #17's acceptance criteria.
- XCHandles and CATalog precommit research has been recorded locally but is
  not published. Their source is under the Prosperity Public License, and this
  repository copies none of it.
