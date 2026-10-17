# Hashing bytes and trees

**WIP prototype. Hashing experiments do not establish production readiness,
compiler correctness, collision resistance or security of a coin policy.**

The native facade provides ordinary safe, allocation-free Rust:

```rust
use chialisp_oxide::{sha256, sha256_parts};

let single = sha256(b"abc");
let parts = sha256_parts(b"a", b"b", b"c");
assert_eq!(single, parts);
```

`sha256_parts(a, b, c)` hashes exactly `a || b || c`. It adds no separator,
length prefix, integer conversion, hexadecimal decoding or CLVM serialization.
Part boundaries therefore provide no domain separation: `("a", "bc", "")`
and `("ab", "", "c")` hash the same bytes. All 32 digest bytes are retained,
including a leading zero. An atom containing zero bytes differs from nil.

## Current experiment boundary

The paired corpus executes a published `sha256tree` helper adapter and the
canonical singleton launcher and original password tutorial module through actual
Cargo exports, native Rust, Chialisp and both Rust/Rue optimization modes.
The complete twenty-one-port report records 389 named and 20,796 generated comparisons plus 40 ledger scenario
groups in three languages. Local macOS verification passes 459 workspace tests,
all-target Clippy with warnings denied, 168 hash-evidence corruption controls and
22,285 retained hash-artifact VM replays. The source-derived proof, error-study,
Lean and site gates also pass. These checks do not establish general external
library support, cryptographic security or compiler preservation.

The password module hashes arbitrary atom bytes through the same reviewed
helper, without a new compiler admission. Its reusable raw module is distinct
from the fixed-hash instances tested separately. The instance experiment
records successful unsigned destination changes: hash equality and currying do
not establish a secure authorization policy. Its lower-budget observations,
and consensus runtime umbrellas without semantic rejection credit, remain
separate from ordinary passing comparisons. See the
[password lesson](https://judeallred.github.io/chialisp-oxide/tutorials/password-coin/).

The IR operation is part of version 4's strict runtime semantics. Its arguments
evaluate once, in order; an unused digest does not discard the computation.
Every part must be an atom or fixed byte array. A malformed pair argument still
fails, including when a later argument diverges. Unchosen branches remain lazy.
The public helper has three parts. The IR's nominal arity limit is 64; existing
global CLVM expansion limits can reject large expressions below that limit.
Successful type validation is not a resource or code-generation guarantee.

SHA hashes arbitrary payload bytes. It does not decode integer-looking atoms,
require canonical numeric encodings, validate a destination, authorize a spend
or prove knowledge of a preimage. Domain separation and the policy around the
hash are the author's responsibility.

## Why the compiler needs implementation identity

An ordinary Rust function named `sha256_parts` can return anything. Replacing
such a function with opcode 11 merely because of its name or package would
change the source program's meaning. A macro marker would not solve this.

The bridge first locates a reserved SDK export as a candidate, then
requires its exact Rust signature and an independently reviewed, pinned
canonical encoding of its reachable typed MIR implementation. The export name
does not authorize behavior. The ordinary `sha256` wrapper retains its real
body; unrelated functions with the same name retain theirs.

The closed encoding records statement and operand order, control flow, concrete
types and layouts, constants and their bodies, calls, compiler/target/profile
configuration, and the actual supported standard-library leaf. Unsupported
constructs fail closed. Raw type/constant graphs and literal constant bodies
must be bounded before normalization or evaluation; counting only the final
serialized payload would leave earlier work unbounded.

The initial native kernel uses primitive arrays and loops plus the actual core
slice-length operation. It does not admit arbitrary RustCrypto dependencies,
traits, architecture dispatch or user helper calls. Changing a reserved kernel
must trigger a targeted diagnostic and review of new independently established
pins. Recomputing a digest from the candidate and accepting that digest would
provide no protection.

The initial reviewed hosts are `aarch64-apple-darwin` and
`x86_64-unknown-linux-gnu`, with the pinned compiler, overflow checks and debug
assertions enabled, and `panic=unwind`. Other profiles fail with an explicit
diagnostic. The standalone research frontend normally uses `panic=abort`; it
does not thereby acquire a hash pin. The supported authoring route is Cargo.

The installed pinned compiler and standard library remain trusted, as do
Cargo/rustc's consistency between dependency metadata and native objects and
the absence of concurrent mutation of build inputs. Location and freshness
checks do not authenticate installed machine code or sandbox compiler queries.
See [the toolchain trust boundary](toolchain-trust.md).

## The tree-hash algorithm

Chia's conventional tree hash gives atoms and pairs different one-byte domains:

```text
hash(Atom(payload))    = SHA256(0x01 || payload)
hash(Pair(left,right)) = SHA256(0x02 || hash(left) || hash(right))
```

These are exact raw-tree rules. Improper tails, padded numeric-looking bytes and
program-shaped data remain data; no finite integer adapter is required. Rust
can express the algorithm with a borrowed `Tree`, ordinary recursion and owned
`[u8; 32]` digest arrays. Named `ATOM_DOMAIN` and `PAIR_DOMAIN` constants make
the two wire values visible without leaving unexplained numbers in the logic.

This recursive helper uses SHA opcode 11. It does not assume activation of the
separate, flag-gated VM `sha256tree` opcode 63. Input-host allocation/depth/work
limits used by a native test oracle are inconclusive host limits; they must not
be inserted as new puzzle rejection rules.

The paired helper's finite comparison uses an explicit ten-million cost ceiling
per input in all five builds, retaining the deep generated inputs. Among named
inputs, the largest recorded Rust costs are 190,843 without optimization and
69,974 with it. These are finite measurements, not general bounds. Ordinary
comparisons must finish conclusively. Other puzzles retain their existing
one-million comparison budget.

Five deep inputs also run at one million as separate resource controls.
Unoptimized Rust exhausts that budget on all five; optimized Rust, Chialisp and
Rue complete. The largest optimized Rust cost in those five controls is 787,188.
Three optimized controls were previously inconclusive. Their improvement adds
no coverage or policy-rejection credit: all 25 typed observations remain
separate from ordinary totals. A `cost_exceeded` result is inconclusive.
Comparison budgets are report metadata, not compiled guards, consensus limits,
or changes to ledger scenario budgets. The signed before report and reviewed
transition are retained in `corpus/cost-optimization-baseline.json`.

## What the checks establish

Published known-answer vectors, every short multipart split, binary padding
boundaries, larger generated inputs, and independent OpenSSL executions test
the native implementation. Actual VM tests check bytes, strictness and lazy
branches with optimization off and on. Source-identity controls distinguish
harmless relocation from changed constants, control flow, signatures, types,
calls and unsupported constructs. Native Linux execution and cross-target MIR
comparison are separate observations.

RustCrypto is also used by the pinned VM's hash implementation. Agreement with
it is useful, but is not an independent second VM crypto implementation.
Published vectors and independently executed OpenSSL implementations provide
additional finite evidence. None of these tests proves the hash algorithm,
cryptographic assumptions, source-to-IR translation or compiler preservation.

The scalar, byte and structural-tree proof APIs reject every program containing
SHA, including dead branches and unused function definitions. They do not
silently replace it with an uninterpreted, injective or collision-free function.
Policies involving hashes need a separately specified model and assumptions.

Equal observed tree hashes also do not make two puzzle modules identical.
Canonical module and curried-instance identity, consensus-valid conditions,
announcements, signatures and full spends each require their own checks.
