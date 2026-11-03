---
title: Understand the Chia coin model before writing a puzzle
description: Follow one coin from puzzle hash to solution, conditions, and replacement coins.
---

**WIP prototype. Not ready for real funds or production use.** This lesson needs
no wallet. Read it before the [Cargo quickstart](/chialisp-oxide/start/quickstart/).

## A coin commits to rules

A Chia coin records a parent coin ID, a puzzle hash, and an amount in mojos.
A puzzle is a CLVM program that defines its spending rules. The coin commits to
the program's tree hash, rather than storing a mutable contract account.
A coin can be spent once. State changes create new coins with new commitments.
See Chia's [coin-set introduction](https://docs.chia.net/chia-blockchain/coin-set-model/intro/).

A Chia address is a readable encoding of a puzzle hash. It is not a public key,
a coin ID, or the digest of a `.clvm.hex` file. One puzzle hash can lock many
coins. The address prefix distinguishes mainnet `xch` from testnet `txch`; the
program's hash does not itself select a network.

## A spend reveals and runs the rules

A coin spend supplies the coin, its puzzle reveal, and a solution: the input
value passed to that program. A spend bundle groups coin spends with an aggregate
signature. Required signatures depend on the returned conditions.

The puzzle returns **conditions**. For example, `CREATE_COIN` requests a new coin;
`ASSERT_MY_AMOUNT` requires a particular input amount. The consensus engine checks
them. The puzzle cannot read a node, query a wallet balance, or ask for the time;
it returns assertions that consensus checks. See the official
[condition reference](https://chialisp.com/conditions/).

## Trace a small payment

In [your first payment](/chialisp-oxide/tutorials/first-payment/), a 1,000-mojo
coin requests one 1,000-mojo recipient coin. The original coin becomes spent;
the recipient coin is a new object. The policy is permissionless: anyone can
trigger it, but cannot change its fixed recipient. A signing requirement is a
separate policy decision.

A successful VM result only shows that the program returned. A simulator must
also check the complete spend, including amount assertions, signatures and value
conservation. A node accepting a submission still does not mean it is confirmed.

## Recognize the three inputs to authoring

- **Committed policy:** fixed code and parameters chosen before funding. Currying
  binds parameters into a new program and changes its puzzle hash.
- **Solution:** caller-supplied data at spending time. Rust argument types constrain
  its encoding; the policy must still validate its meaning.
- **Coin context:** facts such as the actual amount and age, enforced through
  returned conditions rather than trusted solution fields.

Before compiling an example, identify each of these. Then follow
[build to coin spend](/chialisp-oxide/start/build-to-spend/) to see where local
compilation ends and deployment begins.
