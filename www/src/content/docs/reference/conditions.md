---
title: Conditions and authorization
description: Read the condition tuples used in the teaching puzzles without confusing shape checks, ownership, and consensus validation.
---

For a complete policy review, read the dedicated
[conditions and authority guide](/chialisp-oxide/validation/outputs/) and
[review checklist](/chialisp-oxide/validation/review/).

Author puzzle bodies with `create_coin`, `assert_my_amount`, `agg_sig_me`, and
the complementary `assert_seconds_absolute` and
`assert_before_seconds_absolute` helpers. Give policy amounts, recipient hashes,
signer keys, and timestamps names. Use `Amount::from_mojos` and
`Timestamp::from_unix_seconds` to make units explicit.

This page describes the serialized consensus conditions those helpers return,
including their numeric discriminants. Return aliases such as `CreateCoin`
still use tuples underneath; named constructors hide the opcode argument from
the authoring call without authenticating its policy. These codes come from the
[Chialisp conditions reference](https://chialisp.com/conditions/) and the pinned
[consensus opcode definitions](https://github.com/Chia-Network/chia_rs/blob/56f6b676a96de8659fe05d7e4f705df553f37fa5/crates/chia-consensus/src/opcodes.rs).

## Create value and assert coin context

`(51 puzzle_hash amount)` is `CREATE_COIN`. It requests a child coin locked to
the given 32-byte puzzle hash. The consensus engine checks aggregate
conservation across the bundle. Creating less value than the inputs can leave a
fee; creating more value rejects.

`(73 amount)` is `ASSERT_MY_AMOUNT`. It asserts the actual input coin's amount.
A `u64` solution parameter is only a caller's claimed number; it is not an
authenticated reading of the input coin. Use a consensus condition when the
policy needs to bind that context.

## Require authority

`(50 public_key message)` is `AGG_SIG_ME`. The required signature binds the
message to the coin ID and the network's signature domain. Returning the
condition does not validate a signature during plain CLVM execution; the spend
bundle must supply a valid aggregate signature.

An output `Option<AggSigMe>` can form the end of a `Cons` condition list: `Some`
adds the signature condition and `None` adds no condition. This is list
construction, not authorization inference. The policy must decide when a
signature is required. See [optional output lists](/chialisp-oxide/reference/optional-lists/)
for the checked helper example and the output-only compiler boundary.

The signature examples use publicly known test keys derived from fixed seeds.
Those are reproducible fixtures, not key-management guidance. Never use them to
control real funds.

## Require time

`assert_seconds_absolute(deadline)` returns `(81 seconds)`,
`ASSERT_SECONDS_ABSOLUTE`. Consensus permits a timestamp at or after the
deadline, including equality.

`assert_before_seconds_absolute(deadline)` returns `(85 seconds)`,
`ASSERT_BEFORE_SECONDS_ABSOLUTE`. Consensus requires a timestamp strictly
before the deadline; equality rejects.

The puzzle cannot query the chain's clock. It returns an obligation that the
consensus engine checks against its validation context. The pinned simulator
[boundary tests](https://github.com/judeallred/chialisp-oxide/blob/main/crates/chialisp-oxide-simulator/tests/deadline_boundaries.rs)
exercise both conditions at deadline minus one, the exact deadline and deadline
plus one, with exact consensus rejection codes and full simulator equality
after rejection. These condition tests do not establish a complete reclaimable
payment policy. Absolute timestamp 100 is a small simulator fixture value,
not a production recovery delay or a relative lock.

Height and relative-time conditions have different semantics. Do not translate
a tutorial's absolute timestamp into an assumed number of seconds since coin
creation.

## Require height or coin age

Block counts use two nominal types, so a swapped argument is a Rust type error:
`BlockHeight` is an absolute block number and `BlockAge` counts blocks since
the spent coin was created. Both wrap a `u32`.

| Helper | Condition | Consensus meaning |
|---|---|---|
| `assert_height_relative(age)` | `(82 blocks)` | Coin age at least `age` |
| `assert_before_height_relative(age)` | `(86 blocks)` | Coin age strictly below `age` |
| `assert_height_absolute(height)` | `(83 height)` | Block height at least `height` |
| `assert_before_height_absolute(height)` | `(87 height)` | Block height strictly below `height` |

A driver integration test checks each helper's output against the pinned
`chia-wallet-sdk` condition decoder in both compiler modes. It also captures
the rustc errors for swapped units and identifiers. The ledger behaviour of
`assert_height_relative` is exercised by the
[precommit pair](https://github.com/judeallred/chialisp-oxide/blob/main/docs/precommit.md).
The other three helpers have wire-format tests only.

## Coordinate spends

New compositions should examine `SEND_MESSAGE` (66) and `RECEIVE_MESSAGE` (67).
Their mode selects which sender/receiver attributes are bound. The simulator
regression uses mode 63 to bind both complete coin IDs and rejects a different
sender.

Legacy `CREATE_COIN_ANNOUNCEMENT` (60) and `ASSERT_COIN_ANNOUNCEMENT` (61) are also
covered as consensus regressions. The helpers `create_coin_announcement`,
`assert_coin_announcement`, `coin_announcement_id(origin, message)` and
`assert_my_coin_id` emit them for fixed 32-byte messages. A computed
`CoinAnnouncementId` authenticates nothing by itself: consensus must find a
matching announcement from that exact origin coin in the same bundle. Hardcoding another coin's announcement into
a puzzle can lock funds forever if that other coin is spent first. The current
Chialisp reference recommends message conditions for new designs.

## A correct shape can encode a bad policy

The ABI rejects malformed trees and out-of-range integers. Rust checks types.
Neither automatically establishes the correct owner key, destination, fee
policy, lock, or economic invariant. Those obligations belong in explicit
policy tests, ledger scenarios, and, where applicable, formal specifications.
