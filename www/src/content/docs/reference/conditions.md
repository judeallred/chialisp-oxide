---
title: Conditions and authorization
description: Read the condition tuples used in the teaching puzzles without confusing shape checks, ownership, and consensus validation.
---

The current examples return integer-tagged tuples. These codes come from the
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

The signature examples use publicly known test keys derived from fixed seeds.
Those are reproducible fixtures, not key-management guidance. Never use them to
control real funds.

## Require time

`(81 seconds)` is `ASSERT_SECONDS_ABSOLUTE`. The puzzle cannot query the chain's
clock. It returns an obligation that the consensus engine checks against its
validation context. Absolute timestamp 100 is a small simulator fixture value,
not a production recovery delay or a relative lock.

Height and relative-time conditions have different semantics. Do not translate
a tutorial's absolute timestamp into an assumed number of seconds since coin
creation.

## Coordinate spends

New compositions should examine `SEND_MESSAGE` (66) and `RECEIVE_MESSAGE` (67).
Their mode selects which sender/receiver attributes are bound. The simulator
regression uses mode 63 to bind both complete coin IDs and rejects a different
sender.

Legacy `CREATE_COIN_ANNOUNCEMENT` (60) and `ASSERT_COIN_ANNOUNCEMENT` (61) are also
covered as consensus regressions. Hardcoding another coin's announcement into
a puzzle can lock funds forever if that other coin is spent first. The current
Chialisp reference recommends message conditions for new designs.

## A correct shape can encode a bad policy

The ABI rejects malformed trees and out-of-range integers. Rust checks types.
Neither automatically establishes the correct owner key, destination, fee
policy, lock, or economic invariant. Those obligations belong in explicit
policy tests, ledger scenarios, and, where applicable, formal specifications.
