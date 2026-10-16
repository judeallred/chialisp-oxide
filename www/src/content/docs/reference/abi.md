---
title: Puzzle ABI
description: Exact solution shape, canonical integers, booleans, bytes, tuples, and condition-list results.
---

Every solution is an attacker-controlled CLVM tree. A Rust signature describes
the accepted boundary values; it does not make raw CLVM input trusted. The
backend validates the complete solution shape and parameter encodings before
evaluating the function body.

## Function arguments form a proper list

```rust
pub fn puzzle(amount: u64) -> u64 { amount }
```

The solution is `(42)`, not the bare atom `42`, the pair `(42 . 1)`, or a list
with extra arguments. A zero-argument function takes `()`. Missing arguments,
improper tails, nested pairs in scalar positions, and additional arguments reject.

## Integers are canonical signed atoms with declared bounds

CLVM atoms represent integers in signed big-endian two's complement. The ABI
requires the shortest canonical encoding and the Rust type's numeric range.
Numeric zero is the empty atom `()`. Positive 128 needs a leading zero byte to
avoid becoming negative; positive one does not.

- `1` is accepted for `u64`.
- `0x0001` rejects as padded, even though CLVM arithmetic can treat it as one.
- `-1` rejects for an unsigned parameter.
- `18446744073709551616` rejects for `u64`.
- A pair rejects before numeric operations inspect it.

Signed parameters preserve signed bounds. Arithmetic uses those same declared
bounds and rejects on overflow. The VM's arbitrary-precision arithmetic is an
implementation mechanism, not permission to change Rust integer semantics.

## Booleans, bytes, and products

A boolean accepts exactly `()` or `1`; other truthy atoms such as `2` reject.
A `[u8; 32]` parameter is one atom containing exactly 32 bytes, not a list of 32
numeric atoms. Fixed-byte encodings are byte strings, so leading zeros are
meaningful and are not subject to numeric canonicalization.

Tuples are proper lists whose fields are recursively validated. Unit is the
empty list. These are explicit logical layouts, independent of Rust memory
layout, alignment, or endianness.

## Returning conditions

```rust
pub fn puzzle() -> ((u64, u64), (u64, [u8; 32], u64)) {
    ((73, 1000), (51, [0x42; 32], 1000))
}
```

The result is a proper list containing `(73 1000)` and
`(51 0x4242…4242 1000)`. Those are `ASSERT_MY_AMOUNT` and `CREATE_COIN` conditions.
The repeated destination bytes describe the recipient puzzle hash, not an
address string.

The current examples use ordinary integer-tagged tuples for conditions. They
do **not** yet provide nominal `Amount`, `PuzzleHash`, or `Condition` SDK types.
An incorrect numeric opcode can therefore compile. The
[condition reference](/chialisp-oxide/reference/conditions/) and simulator tests
remain necessary checks until a typed SDK replaces this raw interface.
