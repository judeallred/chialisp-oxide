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

## Nominal wrappers erase to the inner encoding

A supported local, single-field `#[repr(transparent)]` wrapper preserves Rust's
source-level type identity but adds no CLVM wrapper node or domain tag.
`PuzzleHash([u8; 32])` is the same 32-byte atom as its inner array; `Amount(u64)`
is the same canonical numeric atom as its inner integer. A one-argument
`puzzle(amount: Amount)` therefore takes `(500)`, not `((500))`.

ABI decoding does not run a Rust constructor or custom validator. It enforces
the erased inner type's encoding, length, and range, not a domain-specific
invariant. A correctly sized atom is not proof that its bytes name the intended
puzzle. Keep policy assertions and authorization checks explicit. Nested
supported wrappers erase recursively; general structs and enums remain
unsupported. See the [typed SDK tutorial](/chialisp-oxide/tutorials/typed-sdk/)
for supported declarations and observed compile-fail diagnostics.

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

The source-only SDK now provides nominal `Amount`, `PuzzleHash`, `CoinId`,
`PublicKey`, `Message32`, and `Timestamp` wrappers with named condition helpers.
The capped-payment fixture uses these helpers. Other fixtures still illustrate
the raw tuple interface, and SDK condition result aliases also remain tuples.
There is no closed nominal `Condition` enum: an incorrect manually constructed
opcode can still compile. The [condition reference](/chialisp-oxide/reference/conditions/)
and simulator tests remain necessary policy checks.
